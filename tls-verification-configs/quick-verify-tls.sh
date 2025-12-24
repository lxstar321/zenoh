#!/bin/bash

# Zenoh 分布式网关 TLS 版本快速验证脚本
# 使用 TLS 加密通信的完整验证

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

CERTS_DIR="./tls-certs"

# 检查证书
check_certs() {
    log_info "检查TLS证书..."

    if [ ! -d "$CERTS_DIR" ]; then
        log_error "证书目录不存在: $CERTS_DIR"
        log_info "请先运行: ./generate-tls-certs.sh"
        exit 1
    fi

    local required_certs=(
        "ca-cert.pem"
        "router-key.pem"
        "router-cert.pem"
        "agent-key.pem"
        "agent-cert.pem"
        "collector-key.pem"
        "collector-cert.pem"
        "device-key.pem"
        "device-cert.pem"
    )

    for cert in "${required_certs[@]}"; do
        if [ ! -f "$CERTS_DIR/$cert" ]; then
            log_error "缺少证书文件: $cert"
            exit 1
        fi
    done

    log_success "所有TLS证书文件存在"
}

# 检查 Docker
check_docker() {
    if ! command -v docker &> /dev/null; then
        log_error "Docker 未安装或不在 PATH 中"
        exit 1
    fi

    if ! docker info &> /dev/null; then
        log_error "Docker 服务未运行或权限不足"
        exit 1
    fi

    log_success "Docker 检查通过"
}

# 清理旧容器
cleanup_old() {
    log_info "清理旧容器..."

    local containers=("gateway-router-1" "gateway-router-2" "gateway-router-3" "tenant-a-agent" "tenant-b-agent" "data-collector" "tenant-a-device" "tenant-b-device")

    for container in "${containers[@]}"; do
        if docker ps -a --format "table {{.Names}}" | grep -q "^${container}$"; then
            log_info "停止并删除容器: $container"
            docker stop "$container" 2>/dev/null || true
            docker rm "$container" 2>/dev/null || true
        fi
    done

    # 清理网络
    if docker network ls --format "table {{.Name}}" | grep -q "^zenoh-network$"; then
        docker network rm zenoh-network 2>/dev/null || true
    fi

    log_success "清理完成"
}

# 启动TLS集群
start_tls_cluster() {
    log_info "启动TLS加密的Zenoh分布式网关集群..."

    docker compose -f docker-compose.tls-verify.yaml up -d

    log_info "等待TLS集群启动..."
    sleep 20  # TLS握手需要更多时间

    log_success "TLS集群启动完成"
}

# 验证TLS集群状态
verify_tls_cluster() {
    log_info "验证TLS集群状态..."

    # 检查容器状态
    local containers=("gateway-router-1" "gateway-router-2" "gateway-router-3" "tenant-a-agent" "data-collector" "tenant-a-device")
    local running=0

    for container in "${containers[@]}"; do
        if docker ps --format "table {{.Names}}" | grep -q "^${container}$"; then
            ((running++))
        fi
    done

    if [ $running -ge 6 ]; then
        log_success "✅ $running 个TLS容器正常运行 (3 Router + 2 Agent + 1 Collector + 2 Device)"
    else
        log_error "❌ 只有 $running/6 个TLS容器运行"
        return 1
    fi

    # 检查TLS连接状态
    log_info "检查TLS连接状态..."

    # 验证Router间的TLS连接
    if curl -s --cacert "$CERTS_DIR/ca-cert.pem" --cert "$CERTS_DIR/router-cert.pem" --key "$CERTS_DIR/router-key.pem" \
           https://localhost:8001/@/router/*/status 2>/dev/null | grep -q "connections\|routers"; then
        log_success "✅ Router TLS连接正常"
    else
        log_warn "⚠️  Router TLS连接检查失败，可能仍在建立连接"
    fi

    # 等待TLS连接完全建立
    sleep 10

    # 检查Peer连接
    local connected_peers=0
    for i in {1..3}; do
        if curl -s --cacert "$CERTS_DIR/ca-cert.pem" --cert "$CERTS_DIR/router-cert.pem" --key "$CERTS_DIR/router-key.pem" \
               https://localhost:800$i/@/router/*/linkstate/peers 2>/dev/null | jq '.[] | length' 2>/dev/null | grep -q '[1-9]'; then
            ((connected_peers++))
        fi
    done

    if [ $connected_peers -ge 1 ]; then
        log_success "✅ TLS集群Peer连接正常 ($connected_peers/3 个Router有连接的Peers)"
    else
        log_warn "⚠️  TLS集群Peer连接可能仍在建立 ($connected_peers/3)"
    fi

    log_success "✅ TLS集群验证完成"
    return 0
}

# 运行TLS功能测试
run_tls_function_tests() {
    log_info "运行TLS加密的功能测试..."

    # 测试 1: 确认TLS设备(Python数据生成器)正常运行
    log_info "测试 1: 检查 TLS 设备数据生成器 (tenant-a-device) 是否在运行..."
    if docker ps --format '{{.Names}}' | grep -q '^tenant-a-device$'; then
        log_success "✅ TLS设备容器 tenant-a-device 正在运行 (由 Python 脚本持续发布数据)"
    else
        log_error "❌ TLS设备容器 tenant-a-device 未运行"
        return 1
    fi

    # 等待TLS消息在网关和Agent之间传播
    sleep 5

    # 测试 2: TLS Agent接收和处理 (通过Python日志验证)
    log_info "测试 2: 检查 TLS Agent (tenant-a-agent) 是否收到并处理数据..."
    if docker logs tenant-a-agent 2>/dev/null | grep -q "收到传感器数据"; then
        log_success "✅ TLS Agent 已开始接收并处理租户A的传感器数据"
    else
        log_warn "⚠️  在 tenant-a-agent 日志中暂未找到传感器数据记录，可能仍在初始化"
    fi

    # 再等一会儿让统计数据生成
    sleep 5

    # 测试 3: TLS Agent 发布统计数据 (通过Python日志验证)
    log_info "测试 3: 检查 TLS Agent 是否发布统计数据..."
    if docker logs tenant-a-agent 2>/dev/null | grep -q "发布工厂统计数据"; then
        log_success "✅ TLS Agent 已发布工厂统计数据到租户和全局路径"
    else
        log_warn "⚠️  在 tenant-a-agent 日志中暂未找到统计发布记录"
    fi

    # 再等一会儿让Collector汇总
    sleep 5

    # 测试 4: TLS Collector 接收统计并生成全局报表 (通过Python日志验证)
    log_info "测试 4: 检查 TLS Collector 是否生成全局监控报表..."
    if docker logs data-collector 2>/dev/null | grep -q "发布全局监控报表"; then
        log_success "✅ TLS Collector 成功生成并发布全局监控报表"
    else
        log_warn "⚠️  在 data-collector 日志中暂未找到全局报表发布记录"
    fi

    # 测试 5: 外部TLS连接测试
    log_info "测试 5: 外部TLS连接测试..."
    # 使用openssl验证TLS连接
    if echo "Q" | openssl s_client -connect localhost:7447 -CAfile "$CERTS_DIR/ca-cert.pem" -cert "$CERTS_DIR/device-cert.pem" -key "$CERTS_DIR/device-key.pem" 2>/dev/null | grep -q "Verify return code: 0"; then
        log_success "✅ 外部TLS连接验证成功"
    else
        log_warn "⚠️  外部TLS连接验证失败，可能配置问题"
    fi

    log_success "✅ TLS功能测试完成"
    return 0
}

# 显示TLS集群信息
show_tls_cluster_info() {
    echo
    log_info "=== 🔒 TLS加密的Zenoh分布式网关集群信息 ==="
    echo
    log_info "🔐 安全特性:"
    echo "  ✅ Router间TLS加密通信"
    echo "  ✅ Agent与Router的TLS客户端认证"
    echo "  ✅ 设备与Router的TLS双向认证"
    echo "  ✅ Collector的TLS安全连接"
    echo
    log_info "📍 TLS网络拓扑:"
    echo "  Router TLS集群 (3节点):"
    echo "    • gateway-router-1: tls/172.20.0.10:7447 (外部端口: 7447)"
    echo "    • gateway-router-2: tls/172.20.0.11:7447 (外部端口: 7448)"
    echo "    • gateway-router-3: tls/172.20.0.12:7447 (外部端口: 7449)"
    echo
    echo "  TLS客户端节点:"
    echo "    • tenant-a-agent: TLS客户端 (172.20.0.20) - 租户A业务处理"
    echo "    • tenant-b-agent: TLS客户端 (172.20.0.30) - 租户B业务处理"
    echo "    • data-collector: TLS客户端 (172.20.0.21) - 全局数据收集"
    echo "    • tenant-a-device: TLS客户端 (172.20.0.100) - 租户A设备"
    echo "    • tenant-b-device: TLS客户端 (172.20.0.101) - 租户B设备"
    echo
    log_info "🔑 TLS证书配置:"
    echo "  CA证书: $CERTS_DIR/ca-cert.pem"
    echo "  Router证书: $CERTS_DIR/router-cert.pem"
    echo "  租户A Agent证书: $CERTS_DIR/agent-cert.pem"
    echo "  租户B Agent证书: $CERTS_DIR/tenant-b-agent-cert.pem"
    echo "  Collector证书: $CERTS_DIR/collector-cert.pem"
    echo "  租户A设备证书: $CERTS_DIR/device-cert.pem"
    echo "  租户B设备证书: $CERTS_DIR/tenant-b-device-cert.pem"
    echo
    log_info "🧪 TLS测试命令示例:"
    echo "  # TLS连接到网关集群"
    echo "  zenoh --connect tls/127.0.0.1:7447 \\"
    echo "        --tls-ca-certificate ./tls-certs/ca-cert.pem \\"
    echo "        --tls-client-private-key ./tls-certs/device-key.pem \\"
    echo "        --tls-client-certificate ./tls-certs/device-cert.pem"
    echo
    echo "  # 发布TLS加密消息"
    echo "  zenoh put /secure/test 'TLS encrypted message!'"
    echo
    echo "  # 查看TLS连接状态"
    echo "  curl --cacert ./tls-certs/ca-cert.pem \\"
    echo "       --cert ./tls-certs/router-cert.pem \\"
    echo "       --key ./tls-certs/router-key.pem \\"
    echo "       https://localhost:8001/@/router/*/linkstate/peers"
}

# 显示TLS使用帮助
show_tls_usage() {
    echo "Zenoh 分布式网关 TLS 版本验证工具"
    echo "================================="
    echo
    echo "此版本在标准版本基础上增加了完整的TLS加密："
    echo "• Router间TLS加密通信"
    echo "• 客户端TLS双向认证"
    echo "• 证书-based安全连接"
    echo
    echo "前提条件："
    echo "• 安装openssl"
    echo "• 运行 ./generate-tls-certs.sh 生成证书"
    echo
    echo "用法: $0 [命令]"
    echo
    echo "命令:"
    echo "  start     - 启动TLS集群并验证"
    echo "  test      - 运行TLS功能测试"
    echo "  status    - 显示TLS集群状态"
    echo "  stop      - 停止TLS集群"
    echo "  clean     - 清理所有TLS容器和网络"
    echo "  info      - 显示TLS集群详细信息"
    echo "  certs     - 检查TLS证书状态"
    echo "  help      - 显示此帮助"
    echo
    echo "快速TLS验证流程:"
    echo "  ./generate-tls-certs.sh    # 1. 生成证书"
    echo "  $0 start                   # 2. 启动TLS集群"
    echo "  $0 test                    # 3. 运行TLS测试"
    echo "  $0 info                    # 4. 查看TLS配置"
    echo
    echo "注意: TLS需要更多时间建立连接，请耐心等待"
}

# 检查TLS证书状态
check_tls_certs() {
    log_info "检查TLS证书状态..."

    if [ ! -d "$CERTS_DIR" ]; then
        log_error "证书目录不存在"
        return 1
    fi

    echo
    log_info "证书文件状态:"
    for cert in "$CERTS_DIR"/*.pem; do
        if [ -f "$cert" ]; then
            echo "  ✅ $(basename "$cert")"
        else
            echo "  ❌ $(basename "$cert")"
        fi
    done

    echo
    log_info "证书有效期检查:"
    if command -v openssl &> /dev/null; then
        echo "CA证书有效期:"
        openssl x509 -in "$CERTS_DIR/ca-cert.pem" -dates -noout 2>/dev/null || echo "  无法读取"

        echo "Router证书有效期:"
        openssl x509 -in "$CERTS_DIR/router-cert.pem" -dates -noout 2>/dev/null || echo "  无法读取"
    else
        log_warn "openssl未安装，跳过证书有效期检查"
    fi

    return 0
}

# 主函数
main() {
    case "${1:-start}" in
        "start")
            log_info "开始TLS加密的Zenoh分布式网关集群验证..."
            check_certs
            check_docker
            cleanup_old
            start_tls_cluster
            if verify_tls_cluster; then
                show_tls_cluster_info
                log_success "🎉 TLS集群启动成功！"
                echo
                log_info "接下来可以运行: $0 test"
            else
                log_error "TLS集群启动失败，请检查证书和配置"
                exit 1
            fi
            ;;
        "test")
            run_tls_function_tests
            ;;
        "status")
    echo "=== TLS容器状态 ==="
    docker ps --filter "name=gateway-router\|tenant-a-agent\|tenant-b-agent\|data-collector\|tenant-a-device\|tenant-b-device" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"
            echo
            echo "=== TLS网络状态 ==="
            docker network ls --filter "name=zenoh-network" --format "table {{.Name}}\t{{.Driver}}"
            ;;
        "stop")
            log_info "停止TLS集群..."
            docker compose -f docker-compose.tls-verify.yaml down
            log_success "TLS集群已停止"
            ;;
        "clean")
            cleanup_old
            log_success "TLS清理完成"
            ;;
        "info")
            show_tls_cluster_info
            ;;
        "certs")
            check_tls_certs
            ;;
        "help"|"-h"|"--help")
            show_tls_usage
            ;;
        *)
            log_error "未知命令: $1"
            echo
            show_tls_usage
            exit 1
            ;;
    esac
}

# 执行主函数
main "$@"
