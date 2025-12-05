#!/usr/bin/env python3
import json
import zenoh

def main():
    # Zenoh TLS 配置
    config = {
        "mode": "client",
        "connect": {
            "endpoints": ["tls/127.0.0.1:7447"],
            "exit_on_failure": False,
            "timeout_ms": 5000
        },
        "transport": {
            "link": {
                "tls": {
                    "root_ca_certificate": "test_tls_certs/ca.pem",
                    "connect_private_key": "test_tls_certs/sensor-client-001key.pem",
                    "connect_certificate": "test_tls_certs/sensor-client-001.pem",
                    "enable_mtls": True
                }
            }
        }
    }

    config_str = json.dumps(config)
    session = zenoh.open(zenoh.Config.from_json5(config_str))

    # 订阅消息
    key = "demo/example/test"
    print(f"📥 订阅消息: {key}")

    def callback(sample):
        print(f"收到消息: {sample.key_expr} => {sample.payload.to_string()}")

    sub = session.declare_subscriber(key, callback)

    print("等待消息中... 按 Ctrl+C 退出")
    try:
        import time
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        pass

    sub.undeclare()
    session.close()

if __name__ == "__main__":
    main()
