#!/usr/bin/env python3

# Zenoh Python SDK 数据生成器
# 使用Zenoh Python SDK实现真实的数据发布

import asyncio
import json
import random
import time
import logging
import os
from datetime import datetime
import zenoh
from zenoh import Config

# 配置日志
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

class DataGenerator:
    def __init__(self, tenant_id="tenant-a", device_id="sensor-001", publish_interval=3.0):
        self.tenant_id = tenant_id
        self.device_id = device_id
        self.publish_interval = publish_interval
        self.session = None
        self.publisher = None
        self.message_count = 0

        print(f"🚀 启动 Zenoh Python 数据生成器")
        print(f"   租户: {self.tenant_id}")
        print(f"   设备: {self.device_id}")
        print(f"   发布间隔: {self.publish_interval}秒")
        print(f"   发布路径: {self.tenant_id}/sensor/*")
        print()

    def connect_zenoh(self):
        """连接到Zenoh网关"""
        try:
            # 从配置文件加载基础配置（与代码位于同一挂载目录）
            config = Config.from_file("client_config.json5")

            # 根据租户设置TLS证书
            if self.tenant_id == "tenant-a":
                cert_file = "device-cert.pem"
                key_file = "device-cert.pem"
            else:
                cert_file = "tenant-b-device-cert.pem"
                key_file = "tenant-b-device-cert.pem"

            config.insert_json5("transport/link/tls/connect_private_key", f'"/app/certs/{key_file}"')
            config.insert_json5("transport/link/tls/connect_certificate", f'"/app/certs/{cert_file}"')

            print("🔗 连接到Zenoh网关...")
            self.session = zenoh.open(config)
            print("✅ 成功连接到Zenoh网关")

        except Exception as e:
            print(f"❌ 连接Zenoh失败: {e}")
            raise

    def generate_factory_data(self):
        """生成智能工厂传感器数据"""
        temperature = round(20 + random.random() * 20, 1)  # 20-40°C
        pressure = round(90 + random.random() * 20, 1)     # 90-110 bar
        vibration = round(0.1 + random.random() * 0.1, 2)  # 0.1-0.2 mm/s

        return {
            "device_id": self.device_id,
            "tenant": self.tenant_id,
            "sensor_type": "industrial",
            "temperature": temperature,
            "pressure": pressure,
            "vibration": vibration,
            "status": "normal",
            "timestamp": int(time.time()),
            "location": "factory-floor-1"
        }

    def generate_agriculture_data(self):
        """生成智慧农业传感器数据"""
        soil_moisture = round(30 + random.random() * 50, 1)  # 30-80%
        soil_ph = round(5.5 + random.random() * 2.0, 1)      # 5.5-7.5
        air_temp = round(15 + random.random() * 15, 1)       # 15-30°C
        humidity = round(40 + random.random() * 40, 1)       # 40-80%

        return {
            "device_id": self.device_id,
            "tenant": self.tenant_id,
            "sensor_type": "agricultural",
            "soil_moisture": soil_moisture,
            "soil_ph": soil_ph,
            "air_temperature": air_temp,
            "humidity": humidity,
            "light_level": random.randint(1000, 9000),
            "status": "normal",
            "timestamp": int(time.time()),
            "location": "field-sector-a"
        }

    async def publish_data_loop(self):
        """主数据发布循环"""
        try:
            while True:
                self.message_count += 1

                # 根据租户类型生成不同数据
                if self.tenant_id == "tenant-a":
                    data = self.generate_factory_data()
                    sensor_path = f"{self.tenant_id}/sensor/industrial"
                    icon = "🏭"
                    data_type = "工厂"
                elif self.tenant_id == "tenant-b":
                    data = self.generate_agriculture_data()
                    sensor_path = f"{self.tenant_id}/sensor/agricultural"
                    icon = "🌱"
                    data_type = "农业"
                else:
                    data = self.generate_factory_data()
                    sensor_path = f"{self.tenant_id}/sensor/data"
                    icon = "📊"
                    data_type = "通用"

                # 创建发布者并发布数据
                try:
                    publisher = self.session.declare_publisher(sensor_path)
                    publisher.put(json.dumps(data))

                    print(f"{icon} [{datetime.now().strftime('%H:%M:%S')}] 发布{data_type}数据 #{self.message_count} 到 {sensor_path}")
                    print(f"   数据内容: {json.dumps(data, indent=None, ensure_ascii=False)}")
                    print()

                    # 清理发布者
                    publisher.undeclare()

                except Exception as e:
                    print(f"❌ 发布数据失败: {e}")

                # 等待下次发布
                await asyncio.sleep(self.publish_interval)

        except KeyboardInterrupt:
            print("\n🛑 数据生成器已停止")
        except Exception as e:
            print(f"❌ 数据生成循环异常: {e}")

    def run(self):
        """运行数据生成器"""
        self.connect_zenoh()

        try:
            asyncio.run(self.publish_data_loop())
        except KeyboardInterrupt:
            print("\n🛑 数据生成器已停止")
        except Exception as e:
            print(f"❌ 数据生成器异常: {e}")
        finally:
            if self.session:
                self.session.close()
                print("🔌 Zenoh会话已关闭")

def main():
    # 从环境变量获取配置
    tenant_id = os.getenv("TENANT_ID", "tenant-a")
    device_id = os.getenv("DEVICE_ID", "sensor-001")
    interval = float(os.getenv("PUBLISH_INTERVAL", "3.0"))

    # 创建并运行数据生成器
    generator = DataGenerator(tenant_id, device_id, interval)
    generator.run()

if __name__ == "__main__":
    main()
