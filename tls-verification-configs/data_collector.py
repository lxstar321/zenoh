#!/usr/bin/env python3

# Zenoh Python SDK 全局数据收集器
# 收集所有租户的统计数据并生成全局报表

import asyncio
import json
import time
import logging
import os
from datetime import datetime
import zenoh
from zenoh import Config, Sample

# 配置日志
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

class GlobalDataCollector:
    def __init__(self, collect_interval=20.0):
        self.collect_interval = collect_interval
        self.session = None

        # 全局统计数据
        self.total_tenants = 2
        self.active_tenants = 0
        self.total_alerts = 0
        self.total_messages_processed = 0
        self.system_start_time = time.time()

        # 租户数据缓存
        self.tenant_data = {}

        print("📊 启动 Zenoh Python 全局数据收集器")
        print(f"   收集间隔: {self.collect_interval}秒")
        print("   订阅路径: stats/global/*")
        print("   输出: 全局监控报表")
        print()

    def connect_zenoh(self):
        """连接到Zenoh网关"""
        try:
            # 从配置文件加载基础配置
            config = Config.from_file("client_config.json5")

            # 设置数据收集器的TLS证书
            config.insert_json5("transport/link/tls/connect_private_key", '"/app/certs/collector-cert.pem"')
            config.insert_json5("transport/link/tls/connect_certificate", '"/app/certs/collector-cert.pem"')

            print("🔗 连接到Zenoh网关集群...")
            self.session = zenoh.open(config)
            print("✅ 成功连接到Zenoh网关集群")

        except Exception as e:
            print(f"❌ 连接Zenoh失败: {e}")
            raise

    def global_stats_listener(self, sample: Sample):
        """全局统计数据接收回调函数"""
        try:
            # key_expr 是 KeyExpr 对象，这里先转成字符串再做 split
            key_expr_str = str(sample.key_expr)
            tenant_id = key_expr_str.split('/')[-1]  # 从路径中提取租户ID
            payload_bytes = bytes(sample.payload)
            payload_str = payload_bytes.decode('utf-8')
            data = json.loads(payload_str)

            self.tenant_data[tenant_id] = data

            print(f"📥 [{datetime.now().strftime('%H:%M:%S')}] 收到租户 {tenant_id} 统计数据:")
            print(f"   处理消息数: {data.get('global_stats', {}).get('total_processed', 0)}")
            print(f"   活跃告警: {data.get('global_stats', {}).get('active_alerts', 0)}")
            print(f"   运行时间: {data.get('global_stats', {}).get('uptime_hours', 0)} 小时")
            print()

        except Exception as e:
            print(f"❌ 处理统计数据失败: {e}")

    def subscribe_global_stats(self):
        """订阅所有租户的全局统计数据"""
        # 订阅所有租户的全局统计数据
        subscribe_path = "stats/global/*"
        print(f"📡 订阅全局统计数据: {subscribe_path}")

        subscriber = self.session.declare_subscriber(subscribe_path, self.global_stats_listener)
        return subscriber

    def generate_global_report(self):
        """生成全局监控报表"""
        current_time = int(time.time())

        # 重新计算全局统计
        self.active_tenants = len(self.tenant_data)
        self.total_alerts = sum(
            tenant.get('global_stats', {}).get('active_alerts', 0)
            for tenant in self.tenant_data.values()
        )
        self.total_messages_processed = sum(
            tenant.get('global_stats', {}).get('total_processed', 0)
            for tenant in self.tenant_data.values()
        )

        # 计算系统运行时间
        uptime_seconds = current_time - self.system_start_time
        uptime_hours = round(uptime_seconds / 3600, 1)

        # 生成全局报表
        global_report = {
            "report_type": "global_monitoring",
            "timestamp": current_time,
            "system_uptime_hours": uptime_hours,
            "summary": {
                "total_tenants": self.total_tenants,
                "active_tenants": self.active_tenants,
                "total_messages_processed": self.total_messages_processed,
                "total_alerts_today": self.total_alerts,
                "system_status": "operational" if self.active_tenants >= self.total_tenants * 0.8 else "degraded",
                "data_freshness": "current"
            },
            "tenant_details": self.tenant_data,
            "performance_metrics": {
                "message_throughput_per_hour": round(self.total_messages_processed / uptime_hours, 1) if uptime_hours > 0 else 0,
                "alert_rate_per_hour": round(self.total_alerts / uptime_hours, 2) if uptime_hours > 0 else 0,
                "system_efficiency": "optimal" if self.active_tenants == self.total_tenants else "degraded"
            },
            "recommendations": []
        }

        # 生成建议
        recommendations = []
        if self.total_alerts > 2:
            recommendations.append("检查高频告警租户的传感器配置")
        else:
            recommendations.append("系统运行正常")

        if self.active_tenants < self.total_tenants:
            recommendations.append("检查租户连接状态")
        else:
            recommendations.append("所有租户正常连接")

        recommendations.append("数据收集周期运行良好")

        global_report["recommendations"] = recommendations

        return global_report

    async def publish_report_loop(self):
        """定期发布全局监控报表"""
        try:
            while True:
                # 生成全局报表
                global_report = self.generate_global_report()

                # 发布全局监控报表
                report_path = "monitoring/global/report"
                publisher = self.session.declare_publisher(report_path)
                publisher.put(json.dumps(global_report))
                publisher.undeclare()

                print(f"🌍 [{datetime.now().strftime('%H:%M:%S')}] 发布全局监控报表")
                print(f"   活跃租户: {self.active_tenants}/{self.total_tenants}")
                print(f"   累计处理消息: {self.total_messages_processed}")
                print(f"   今日告警总数: {self.total_alerts}")
                print(f"   系统运行时间: {global_report['system_uptime_hours']} 小时")
                print()

                # 详细显示报表摘要
                print("📋 报表摘要:")
                summary = global_report['summary']
                print(f"   状态: {summary['system_status']}")
                print(f"   数据新鲜度: {summary['data_freshness']}")

                metrics = global_report['performance_metrics']
                print(f"   消息吞吐量: {metrics['message_throughput_per_hour']} 条/小时")
                print(f"   告警率: {metrics['alert_rate_per_hour']} 个/小时")
                print(f"   系统效率: {metrics['system_efficiency']}")

                print("   建议:")
                for rec in global_report['recommendations']:
                    print(f"     • {rec}")
                print()

                # 等待下次收集
                await asyncio.sleep(self.collect_interval)

        except KeyboardInterrupt:
            print("\n🛑 全局数据收集器已停止")
        except Exception as e:
            print(f"❌ 报表发布循环异常: {e}")

    def run(self):
        """运行全局数据收集器"""
        self.connect_zenoh()

        # 启动订阅
        subscriber = self.subscribe_global_stats()

        try:
            asyncio.run(self.publish_report_loop())
        except KeyboardInterrupt:
            print("\n🛑 全局数据收集器已停止")
        except Exception as e:
            print(f"❌ 全局数据收集器异常: {e}")
        finally:
            if self.session:
                self.session.close()
                print("🔌 Zenoh会话已关闭")

def main():
    # 从环境变量获取配置
    interval = float(os.getenv("COLLECT_INTERVAL", "20.0"))

    # 创建并运行全局数据收集器
    collector = GlobalDataCollector(interval)
    collector.run()

if __name__ == "__main__":
    main()
