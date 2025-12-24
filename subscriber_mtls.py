#!/usr/bin/env python3
"""
Zenoh mTLS 订阅者客户端示例
用于测试 mTLS 加密通信的订阅者
"""

import asyncio
import json
import logging
import zenoh
from zenoh import Config, Sample

# 配置日志
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

def listener(sample):
    """消息监听器回调函数"""
    try:
        # 获取 payload 数据
        payload_bytes = bytes(sample.payload)
        payload_str = payload_bytes.decode('utf-8')

        # 解析 JSON 消息
        message = json.loads(payload_str)

        logger.info(f"收到消息: ID={message['message_id']}, "
                   f"时间戳={message['timestamp']:.2f}")
        logger.info(f"消息内容: {message['data']}")
        logger.info(f"安全状态: {message['security']}")
        logger.info("-" * 50)

    except json.JSONDecodeError as e:
        logger.error(f"JSON 解析失败: {e}")
        logger.info(f"原始消息: {payload_str}")
    except Exception as e:
        logger.error(f"处理消息时出错: {e}")
        logger.error(f"Payload 类型: {type(sample.payload)}")

async def main():
    # 加载 mTLS 客户端配置
    config = Config.from_file("client_mtls_config.json5")

    try:
        logger.info("连接到 Zenoh 路由器 (使用 mTLS)...")
        # 建立会话
        session = zenoh.open(config)
        logger.info("成功建立 mTLS 连接！")

        # 创建订阅者
        subscriber = session.declare_subscriber("sensor/temperature/room1", listener)

        logger.info("订阅者已启动，正在监听消息...")
        logger.info("按 Ctrl+C 停止")

        # 保持运行
        while True:
            await asyncio.sleep(1)

    except KeyboardInterrupt:
        logger.info("收到中断信号，正在关闭...")
    except Exception as e:
        logger.error(f"发生错误: {e}")
        raise
    finally:
        if 'session' in locals():
            session.close()
            logger.info("会话已关闭")

if __name__ == "__main__":
    asyncio.run(main())
