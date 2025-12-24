#!/bin/bash

# Zenoh TLS 多租户验证环境启动脚本
# 从项目根目录启动完整的TLS验证环境

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

# 检查是否在正确的目录
check_directory() {
    if [ ! -d "tls-verification-configs" ]; then
        log_error "错误: tls-verification-configs 目录不存在"
        log_error "请确保在 Zenoh 项目根目录下运行此脚本"
        exit 1
    fi
}

# 显示使用帮助
show_usage() {
    echo "Zenoh TLS 多租户验证环境启动脚本"
    echo "=================================="
    echo
    echo "此脚本用于启动完整的Zenoh TLS多租户分布式网关验证环境。"
    echo
    echo "环境包含:"
    echo "• 3个TLS加密的Router节点 (分布式网关)"
    echo "• 2个租户的设备节点 (数据生成)"
    echo "• 2个租户的Agent节点 (数据处理)"
    echo "• 1个全局数据收集器 (监控统计)"
    echo
    echo "数据流:"
    echo "设备 → Router集群 → Agent处理 → 全局收集器"
    echo "tenant-a/sensor/* → tenant-a/stats/* → stats/global/*"
    echo "tenant-b/sensor/* → tenant-b/stats/* → stats/global/*"
    echo
    echo "用法: $0 [选项]"
    echo
    echo "选项:"
    echo "  start       - 启动TLS验证环境"
    echo "  stop        - 停止TLS验证环境"
    echo "  status      - 查看环境状态"
    echo "  monitor     - 启动数据流监控"
    echo "  logs        - 查看所有组件日志"
    echo "  clean       - 清理所有容器和证书"
    echo "  help        - 显示此帮助"
    echo
    echo "快速开始:"
    echo "  $0 start    # 启动环境"
    echo "  $0 monitor  # 监控数据流"
    echo "  $0 stop     # 停止环境"
    echo
    echo "配置目录: ./tls-verification-configs/"
}

# 生成TLS证书
generate_certs() {
    log_info "生成TLS证书..."
    cd tls-verification-configs
    if [ ! -f "generate-tls-certs.sh" ]; then
        log_error "generate-tls-certs.sh 脚本不存在"
        exit 1
    fi
    ./generate-tls-certs.sh
    cd ..
    log_success "TLS证书生成完成"
}

# 启动环境
start_environment() {
    log_info "启动TLS多租户验证环境..."

    # 检查证书
    if [ ! -d "tls-verification-configs/tls-certs" ]; then
        log_warn "TLS证书不存在，正在生成..."
        generate_certs
    fi

    # 启动环境
    cd tls-verification-configs
    ./quick-verify-tls.sh start
    cd ..

    log_success "TLS验证环境启动完成"
    echo
    log_info "🎉 环境已启动！"
    echo
    log_info "查看状态: $0 status"
    log_info "监控数据流: $0 monitor"
    log_info "查看日志: $0 logs"
    log_info "停止环境: $0 stop"
}

# 停止环境
stop_environment() {
    log_info "停止TLS验证环境..."
    cd tls-verification-configs
    ./quick-verify-tls.sh stop
    cd ..
    log_success "TLS验证环境已停止"
}

# 查看状态
show_status() {
    log_info "查看TLS验证环境状态..."
    echo

    # 检查证书
    if [ -d "tls-verification-configs/tls-certs" ]; then
        log_success "✅ TLS证书: 已生成"
    else
        log_warn "⚠️  TLS证书: 未生成"
    fi

    echo
    log_info "容器状态:"
    cd tls-verification-configs

    # 检查容器状态
    containers=$(docker compose -f docker-compose.tls-verify.yaml ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}" 2>/dev/null | tail -n +2)

    if [ -z "$containers" ]; then
        log_warn "⚠️  没有运行的容器"
        echo
        log_info "运行 '$0 start' 启动环境"
    else
        echo "$containers" | while read -r line; do
            if [[ $line == *"Up"* ]]; then
                echo -e "  ${GREEN}✅${NC} $line"
            else
                echo -e "  ${RED}❌${NC} $line"
            fi
        done

        # 统计运行的容器数量
        running_count=$(echo "$containers" | grep -c "Up" 2>/dev/null || echo "0")
        total_count=$(echo "$containers" | wc -l)

        echo
        if [ "$running_count" -eq 8 ]; then
            log_success "✅ 集群完整: $running_count/8 容器运行正常"
        elif [ "$running_count" -gt 0 ]; then
            log_warn "⚠️  集群不完整: $running_count/8 容器运行"
        else
            log_warn "⚠️  集群未启动: 0/8 容器运行"
        fi
    fi

    cd ..
}

# 监控数据流
monitor_data_flow() {
    log_info "启动数据流监控..."
    cd tls-verification-configs

    if [ ! -f "monitor-data-flow.sh" ]; then
        log_error "monitor-data-flow.sh 脚本不存在"
        exit 1
    fi

    ./monitor-data-flow.sh
    cd ..
}

# 查看日志
show_logs() {
    log_info "查看所有组件日志..."
    echo "按 Ctrl+C 退出日志查看"
    echo
    cd tls-verification-configs
    docker compose -f docker-compose.tls-verify.yaml logs -f
    cd ..
}

# 清理环境
clean_environment() {
    log_warn "清理TLS验证环境..."
    echo
    read -p "这将删除所有容器和TLS证书，确认? (y/N): " -n 1 -r
    echo

    if [[ $REPLY =~ ^[Yy]$ ]]; then
        # 停止容器
        stop_environment 2>/dev/null || true

        # 删除证书
        if [ -d "tls-verification-configs/tls-certs" ]; then
            log_info "删除TLS证书..."
            rm -rf tls-verification-configs/tls-certs
            log_success "TLS证书已删除"
        fi

        # 清理Docker资源
        log_info "清理Docker资源..."
        docker system prune -f >/dev/null 2>&1
        log_success "Docker资源已清理"

        log_success "清理完成"
    else
        log_info "已取消清理操作"
    fi
}

# 主函数
main() {
    check_directory

    case "${1:-help}" in
        "start")
            start_environment
            ;;
        "stop")
            stop_environment
            ;;
        "status")
            show_status
            ;;
        "monitor")
            monitor_data_flow
            ;;
        "logs")
            show_logs
            ;;
        "clean")
            clean_environment
            ;;
        "help"|"-h"|"--help"|*)
            show_usage
            ;;
    esac
}

# 执行主函数
main "$@"

