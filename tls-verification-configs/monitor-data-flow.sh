#!/bin/bash

# Zenoh 多租户数据流实时监控脚本
# 监控所有组件的数据生产和处理状态

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[$(date '+%H:%M:%S')]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[$(date '+%H:%M:%S')]${NC} $1"
}

log_error() {
    echo -e "${RED}[$(date '+%H:%M:%S')]${NC} $1"
}

log_data() {
    echo -e "${CYAN}[$(date '+%H:%M:%S')]${NC} $1"
}

# 检查集群状态
check_cluster_status() {
    log_info "🔍 检查集群状态..."

    local containers=("gateway-router-1" "gateway-router-2" "gateway-router-3" "tenant-a-agent" "tenant-b-agent" "data-collector" "tenant-a-device" "tenant-b-device")
    local running=0

    for container in "${containers[@]}"; do
        if docker ps --format "table {{.Names}}" | grep -q "^${container}$"; then
            ((running++))
        fi
    done

    if [ $running -eq 8 ]; then
        log_success "✅ 集群完整: 8/8 容器运行正常"
        echo "   • 3个Router (网关)"
        echo "   • 2个Agent (租户处理器)"
        echo "   • 1个Collector (全局收集器)"
        echo "   • 2个Device (租户设备)"
        echo
        return 0
    else
        log_error "❌ 集群不完整: $running/8 容器运行"
        return 1
    fi
}

# 监控数据流
monitor_data_flow() {
    log_info "📊 开始实时监控多租户数据流..."
    echo "按 Ctrl+C 退出监控"
    echo

    local cycle=0
    local start_time=$(date +%s)

    while true; do
        ((cycle++))
        current_time=$(date +%s)
        elapsed=$((current_time - start_time))

        echo "=================================================================================="
        log_info "🚀 监控周期 #$cycle (运行 ${elapsed}秒)"
        echo "=================================================================================="

        # 1. 监控设备数据生产
        echo
        log_info "📡 设备数据生产状态:"
        monitor_device "tenant-a-device" "🏭 租户A工厂设备" "tenant-a"
        monitor_device "tenant-b-device" "🌱 租户B农业设备" "tenant-b"

        # 2. 监控Agent数据处理
        echo
        log_info "🔧 Agent数据处理状态:"
        monitor_agent "tenant-a-agent" "🏭 租户A工厂Agent" "tenant-a"
        monitor_agent "tenant-b-agent" "🌱 租户B农业Agent" "tenant-b"

        # 3. 监控全局数据收集
        echo
        log_info "📊 全局数据收集状态:"
        monitor_collector "data-collector" "🌍 全局数据收集器"

        # 4. 监控网关状态
        echo
        log_info "🌐 网关集群状态:"
        monitor_gateways

        # 5. 显示数据流统计
        echo
        log_info "📈 数据流统计摘要:"
        show_flow_statistics "$cycle" "$elapsed"

        echo
        log_info "⏱️  等待 15 秒后进行下次监控..."
        sleep 15
    done
}

# 监控设备数据生产
monitor_device() {
    local container=$1
    local label=$2
    local tenant=$3

    if docker ps --format "table {{.Names}}" | grep -q "^${container}$"; then
        # 检查容器日志中的数据生产活动
        local recent_logs=$(docker logs --tail 10 "$container" 2>&1 | grep -E "(发布|数据|sensor)" | tail -3)

        if [ -n "$recent_logs" ]; then
            echo "   $label: ✅ 活跃生产数据"
            echo "      最新活动:"
            echo "$recent_logs" | while read -r line; do
                echo "        $line"
            done
        else
            echo "   $label: ⚠️  运行中但无近期数据生产"
        fi
    else
        echo "   $label: ❌ 容器未运行"
    fi
}

# 监控Agent数据处理
monitor_agent() {
    local container=$1
    local label=$2
    local tenant=$3

    if docker ps --format "table {{.Names}}" | grep -q "^${container}$"; then
        local recent_logs=$(docker logs --tail 10 "$container" 2>&1 | grep -E "(处理|统计|生成)" | tail -3)

        if [ -n "$recent_logs" ]; then
            echo "   $label: ✅ 活跃处理数据"
            echo "      最新活动:"
            echo "$recent_logs" | while read -r line; do
                echo "        $line"
            done
        else
            echo "   $label: ⚠️  运行中但无近期数据处理"
        fi
    else
        echo "   $label: ❌ 容器未运行"
    fi
}

# 监控全局数据收集器
monitor_collector() {
    local container=$1
    local label=$2

    if docker ps --format "table {{.Names}}" | grep -q "^${container}$"; then
        local recent_logs=$(docker logs --tail 5 "$container" 2>&1 | grep -E "(收集|报表|统计)" | tail -2)

        if [ -n "$recent_logs" ]; then
            echo "   $label: ✅ 活跃收集数据"
            echo "      最新活动:"
            echo "$recent_logs" | while read -r line; do
                echo "        $line"
            done
        else
            echo "   $label: ⚠️  运行中但无近期数据收集"
        fi
    else
        echo "   $label: ❌ 容器未运行"
    fi
}

# 监控网关集群
monitor_gateways() {
    local healthy_gateways=0

    for i in {1..3}; do
        local container="gateway-router-$i"
        if docker ps --format "table {{.Names}}" | grep -q "^${container}$"; then
            # 检查Router日志中的连接信息
            local connections=$(docker logs "$container" 2>&1 | grep -c "session opened\|peer connected" 2>/dev/null || echo "0")
            echo "   🌐 Router-$i: ✅ 运行中 (已建立 $connections 个连接)"
            ((healthy_gateways++))
        else
            echo "   🌐 Router-$i: ❌ 未运行"
        fi
    done

    if [ $healthy_gateways -ge 2 ]; then
        echo "   ✅ 网关集群健康 ($healthy_gateways/3 节点活跃)"
    else
        echo "   ⚠️  网关集群 degraded ($healthy_gateways/3 节点活跃)"
    fi
}

# 显示数据流统计
show_flow_statistics() {
    local cycle=$1
    local elapsed=$2

    # 计算理论数据量
    local tenant_a_expected=$((cycle * 60 / 3))  # 每3秒一个数据
    local tenant_b_expected=$((cycle * 60 / 4))  # 每4秒一个数据
    local agent_a_expected=$((cycle * 60 / 8))   # 每8秒处理一次
    local agent_b_expected=$((cycle * 60 / 12))  # 每12秒处理一次
    local collector_expected=$((cycle * 60 / 20)) # 每20秒收集一次

    echo "   📊 理论数据生产量 (基于 $elapsed 秒运行):"
    echo "      • 租户A设备: ~$tenant_a_expected 条传感器数据"
    echo "      • 租户B设备: ~$tenant_b_expected 条传感器数据"
    echo "      • 租户A Agent: ~$agent_a_expected 次数据处理"
    echo "      • 租户B Agent: ~$agent_b_expected 次数据处理"
    echo "      • 全局收集器: ~$collector_expected 次数据汇总"
    echo
    echo "   🔄 数据流向验证:"
    echo "      设备数据 → 网关路由 → Agent处理 → 全局收集"
    echo "      tenant-a/sensor/* → tenant-a/stats/* → stats/global/*"
    echo "      tenant-b/sensor/* → tenant-b/stats/* → stats/global/*"
}

# 显示使用帮助
show_usage() {
    echo "Zenoh 多租户数据流实时监控工具"
    echo "================================"
    echo
    echo "实时监控多租户架构中的数据生产和处理过程："
    echo "• 设备持续产生传感器数据"
    echo "• Agent实时处理业务逻辑"
    echo "• 收集器生成全局统计报表"
    echo "• 网关提供路由和负载均衡"
    echo
    echo "前提条件："
    echo "• TLS多租户集群已启动并运行数据生成器"
    echo "• 所有8个容器正常运行且有数据脚本"
    echo
    echo "监控内容："
    echo "• 设备数据生产状态"
    echo "• Agent数据处理状态"
    echo "• 全局数据收集状态"
    echo "• 网关集群健康状态"
    echo "• 数据流统计摘要"
    echo
    echo "用法: $0 [选项]"
    echo
    echo "选项:"
    echo "  (无参数)    - 开始实时监控"
    echo "  status      - 只检查集群状态"
    echo "  logs        - 查看各组件的详细日志"
    echo "  help        - 显示此帮助"
    echo
    echo "示例:"
    echo "  $0              # 开始实时监控 (推荐)"
    echo "  $0 status       # 快速状态检查"
    echo "  $0 logs         # 查看详细日志"
}

# 显示详细日志
show_logs() {
    echo "选择要查看的组件日志:"
    echo "1. tenant-a-device   - 租户A设备数据生产"
    echo "2. tenant-b-device   - 租户B设备数据生产"
    echo "3. tenant-a-agent    - 租户A Agent数据处理"
    echo "4. tenant-b-agent    - 租户B Agent数据处理"
    echo "5. data-collector    - 全局数据收集"
    echo "6. gateway-router-1  - 网关Router-1"
    echo "7. all               - 查看所有组件日志"
    echo
    read -p "请选择 (1-7): " choice

    case $choice in
        1) docker logs -f tenant-a-device ;;
        2) docker logs -f tenant-b-device ;;
        3) docker logs -f tenant-a-agent ;;
        4) docker logs -f tenant-b-agent ;;
        5) docker logs -f data-collector ;;
        6) docker logs -f gateway-router-1 ;;
        7)
            echo "=== 所有组件日志 (按 Ctrl+C 退出) ==="
            docker compose -f docker-compose.tls-verify.yaml logs -f
            ;;
        *) echo "无效选择" ;;
    esac
}

# 主函数
main() {
    case "${1:-monitor}" in
        "monitor")
            if check_cluster_status; then
                monitor_data_flow
            else
                log_error "集群状态异常，请先运行: ./quick-verify-tls.sh start"
                exit 1
            fi
            ;;
        "status")
            check_cluster_status
            ;;
        "logs")
            show_logs
            ;;
        "help"|"-h"|"--help")
            show_usage
            ;;
        *)
            log_error "未知选项: $1"
            echo
            show_usage
            exit 1
            ;;
    esac
}

# 捕获中断信号
trap 'echo -e "\n\n🛑 监控已停止"; exit 0' INT

# 执行主函数
main "$@"
