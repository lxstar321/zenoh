#!/usr/bin/env python3

# Zenoh Python SDK 数据处理器
# 使用Zenoh Python SDK订阅和处理数据

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

class DataProcessor:
    def __init__(self, tenant_id="tenant-a", process_interval=8.0):
        self.tenant_id = tenant_id
        self.process_interval = process_interval
        self.session = None

        # 统计数据
        self.total_messages = 0
        self.processed_messages = 0
        self.alerts_count = 0
        self.start_time = time.time()

        print(f"🔧 启动 Zenoh Python 数据处理器")
        print(f"   租户: {self.tenant_id}")
        print(f"   处理间隔: {self.process_interval}秒")
        print(f"   订阅路径: {self.tenant_id}/sensor/*")
        print(f"   发布路径: {self.tenant_id}/stats/* 和 stats/global/{self.tenant_id}")
        print()

    def connect_zenoh(self):
        """连接到Zenoh网关"""
        try:
            # 从配置文件加载基础配置
            config = Config.from_file("client_config.json5")

            # 根据租户设置TLS证书
            if self.tenant_id == "tenant-a":
                cert_file = "agent-cert.pem"
                key_file = "agent-cert.pem"
            else:
                cert_file = "tenant-b-agent-cert.pem"
                key_file = "tenant-b-agent-cert.pem"

            config.insert_json5("transport/link/tls/connect_private_key", f'"/app/certs/{key_file}"')
            config.insert_json5("transport/link/tls/connect_certificate", f'"/app/certs/{cert_file}"')

            print("🔗 连接到Zenoh网关集群...")
            self.session = zenoh.open(config)
            print("✅ 成功连接到Zenoh网关集群")

        except Exception as e:
            print(f"❌ 连接Zenoh失败: {e}")
            raise

    def sensor_data_listener(self, sample: Sample):
        """传感器数据接收回调函数"""
        try:
            # 解析接收到的数据
            payload_bytes = bytes(sample.payload)
            payload_str = payload_bytes.decode('utf-8')
            data = json.loads(payload_str)

            self.total_messages += 1

            print(f"📥 [{datetime.now().strftime('%H:%M:%S')}] 收到传感器数据:")
            print(f"   路径: {sample.key_expr}")
            print(f"   设备: {data.get('device_id', 'unknown')}")
            print(f"   类型: {data.get('sensor_type', 'unknown')}")
            print(f"   状态: {data.get('status', 'unknown')}")
            print()

        except Exception as e:
            print(f"❌ 处理接收数据失败: {e}")

    def subscribe_sensor_data(self):
        """订阅传感器数据"""
        # 订阅租户的所有传感器数据
        subscribe_path = f"{self.tenant_id}/sensor/*"
        print(f"📡 订阅传感器数据: {subscribe_path}")

        subscriber = self.session.declare_subscriber(subscribe_path, self.sensor_data_listener)
        return subscriber

    def process_factory_data(self):
        """处理智能工厂数据"""
        # 模拟处理逻辑
        temp_threshold = 35.0  # 温度告警阈值
        pressure_threshold = 105.0  # 压力告警阈值

        # 模拟数据处理结果
        temp = round(25 + (self.processed_messages % 10), 1)
        pressure = round(95 + (self.processed_messages % 15), 1)

        # 告警检查
        alerts = []
        if temp > temp_threshold:
            alerts.append("温度过高")
            self.alerts_count += 1
        if pressure > pressure_threshold:
            alerts.append("压力异常")
            self.alerts_count += 1

        # 生成处理结果
        processing_time = round(50 + (self.processed_messages % 50), 1)

        return {
            "tenant": self.tenant_id,
            "processor": "factory-agent",
            "timestamp": int(time.time()),
            "metrics": {
                "total_messages": self.total_messages,
                "processed_messages": self.processed_messages,
                "alerts_today": self.alerts_count,
                "avg_temperature": temp,
                "avg_pressure": pressure,
                "system_status": "normal"
            },
            "alerts": alerts,
            "processing_time_ms": processing_time
        }

    def process_agriculture_data(self):
        """处理智慧农业数据"""
        # 模拟农业数据处理
        moisture_min = 40.0
        ph_min = 6.0
        ph_max = 7.5

        # 模拟处理结果
        moisture = round(50 + (self.processed_messages % 30), 1)
        ph = round(6.0 + (self.processed_messages % 15) / 10, 1)

        # 告警检查
        alerts = []
        if moisture < moisture_min:
            alerts.append("土壤干燥")
            self.alerts_count += 1
        if ph < ph_min or ph > ph_max:
            alerts.append("PH值异常")
            self.alerts_count += 1

        # 生成农业处理结果
        processing_time = round(100 + (self.processed_messages % 100), 1)

        return {
            "tenant": self.tenant_id,
            "processor": "agriculture-agent",
            "timestamp": int(time.time()),
            "metrics": {
                "total_messages": self.total_messages,
                "processed_messages": self.processed_messages,
                "alerts_today": self.alerts_count,
                "avg_soil_moisture": moisture,
                "avg_soil_ph": ph,
                "irrigation_recommendations": moisture < 50,
                "system_status": "normal"
            },
            "alerts": alerts,
            "processing_time_ms": processing_time
        }

    async def publish_stats_loop(self):
        """定期发布统计数据"""
        try:
            while True:
                self.processed_messages += 1

                # 根据租户类型处理不同数据
                if self.tenant_id == "tenant-a":
                    stats_data = self.process_factory_data()
                    icon = "🏭"
                    processor_type = "工厂"
                elif self.tenant_id == "tenant-b":
                    stats_data = self.process_agriculture_data()
                    icon = "🌱"
                    processor_type = "农业"
                else:
                    stats_data = self.process_factory_data()
                    icon = "📊"
                    processor_type = "通用"

                # 发布租户统计数据
                tenant_stats_path = f"{self.tenant_id}/stats/processor"
                publisher = self.session.declare_publisher(tenant_stats_path)
                publisher.put(json.dumps(stats_data))
                publisher.undeclare()

                # 发布全局统计摘要
                global_stats = {
                    "tenant": self.tenant_id,
                    "global_stats": {
                        "total_processed": self.processed_messages,
                        "active_alerts": self.alerts_count,
                        "uptime_hours": round((time.time() - self.start_time) / 3600, 1),
                        "last_update": int(time.time())
                    }
                }
                global_stats_path = f"stats/global/{self.tenant_id}"
                publisher = self.session.declare_publisher(global_stats_path)
                publisher.put(json.dumps(global_stats))
                publisher.undeclare()

                print(f"{icon} [{datetime.now().strftime('%H:%M:%S')}] 发布{processor_type}统计数据 #{self.processed_messages}")
                print(f"   租户统计: {json.dumps(stats_data, indent=None, ensure_ascii=False)}")
                print(f"   全局统计: {json.dumps(global_stats, indent=None, ensure_ascii=False)}")
                print()

                # 等待下次处理
                await asyncio.sleep(self.process_interval)

        except KeyboardInterrupt:
            print("\n🛑 数据处理器已停止")
        except Exception as e:
            print(f"❌ 统计发布循环异常: {e}")

    def run(self):
        """运行数据处理器"""
        self.connect_zenoh()

        # 启动订阅
        subscriber = self.subscribe_sensor_data()

        try:
            asyncio.run(self.publish_stats_loop())
        except KeyboardInterrupt:
            print("\n🛑 数据处理器已停止")
        except Exception as e:
            print(f"❌ 数据处理器异常: {e}")
        finally:
            if self.session:
                self.session.close()
                print("🔌 Zenoh会话已关闭")

def main():
    # 从环境变量获取配置
    tenant_id = os.getenv("TENANT_ID", "tenant-a")
    interval = float(os.getenv("PROCESS_INTERVAL", "8.0"))

    # 创建并运行数据处理器
    processor = DataProcessor(tenant_id, interval)
    processor.run()

if __name__ == "__main__":
    main()
