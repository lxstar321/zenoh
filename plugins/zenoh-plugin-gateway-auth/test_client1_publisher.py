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

    # 发布消息
    key = "demo/example/test"
    value = "Hello from publisher!"
    print(f"📤 发布消息: {key} => {value}")
    session.put(key, value)

    session.close()

if __name__ == "__main__":
    main()
