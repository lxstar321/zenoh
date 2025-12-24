#!/usr/bin/env python3
"""
Zenoh mTLS 发布者客户端示例
用于测试 mTLS 加密通信的发布者
"""

import asyncio
import json
import time
import logging
import zenoh
from zenoh import Config

# 配置日志
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

async def main():
    # 加载 mTLS 客户端配置
    config = Config.from_file("client_mtls_config.json5")

    try:
        logger.info("连接到 Zenoh 路由器 (使用 mTLS)...")
        # 建立会话
        session = zenoh.open(config)
        logger.info("成功建立 mTLS 连接！")

        # 创建发布者
        publisher = session.declare_publisher("sensor/temperature/room1")

        logger.info("开始发布消息...")
        message_count = 0

        while True:
            # 构建测试消息
            message = {
                "timestamp": time.time(),
                "message_id": message_count,
                "data": f"Hello from mTLS publisher! Message #{message_count}",
                "security": "mTLS encrypted"
            }

            # 发布消息
            publisher.put(json.dumps(message))
            logger.info(f"已发布消息 #{message_count}: {message['data']}")

            message_count += 1

            # 每秒发布一条消息
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
