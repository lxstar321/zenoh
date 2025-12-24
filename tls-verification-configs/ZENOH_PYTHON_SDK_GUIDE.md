# Zenoh Python SDK 使用指南

## 🎯 概述

本文档介绍如何使用 Zenoh Python SDK 实现多租户分布式网关的数据发布和订阅功能。通过 Python SDK，我们可以：

- ✅ **真实的数据发布**: 使用 `session.put()` 发布数据到指定路径
- ✅ **实时数据订阅**: 使用 `session.declare_subscriber()` 订阅数据
- ✅ **TLS加密通信**: 配置证书实现安全通信
- ✅ **多租户隔离**: 通过路径前缀实现数据隔离
- ✅ **异步处理**: 利用 asyncio 进行高效并发处理

## 🏗️ 架构组件

### 1. 数据生成器 (data_generator.py)

**功能**: 模拟IoT设备持续发布传感器数据

**核心代码**:
```python
# 连接到Zenoh网关
cfg = {
    "connect": {
        "endpoints": ["tls/172.20.0.10:7447"],  # TLS连接
    },
    "transport": {
        "link": {
            "tls": {
                "root_ca_certificate": "/app/certs/ca-cert.pem",
                "connect_private_key": "/app/certs/device-cert.pem",
                "connect_certificate": "/app/certs/device-cert.pem"
            }
        }
    }
}

session = await zenoh.open(cfg)

# 发布数据
await session.put(f"{tenant_id}/sensor/temperature", json.dumps(sensor_data))
```

**租户隔离**:
- 租户A: `tenant-a/sensor/*`
- 租户B: `tenant-b/sensor/*`

### 2. 数据处理器 (data_processor.py)

**功能**: 订阅传感器数据，进行业务处理并发布统计信息

**核心代码**:
```python
# 订阅传感器数据
def listener(sample):
    data = json.loads(sample.payload.decode('utf-8'))
    print(f"收到数据: {data}")

subscriber = await session.declare_subscriber(f"{tenant_id}/sensor/*", listener)

# 发布处理结果
await session.put(f"{tenant_id}/stats/processor", json.dumps(stats_data))
await session.put(f"stats/global/{tenant_id}", json.dumps(global_stats))
```

**处理逻辑**:
- 智能工厂: 温度/压力告警检测
- 智慧农业: 土壤湿度/PH值监控

### 3. 全局收集器 (data_collector.py)

**功能**: 收集所有租户的统计数据，生成全局监控报表

**核心代码**:
```python
# 订阅所有租户的全局统计
subscriber = await session.declare_subscriber("stats/global/*", stats_listener)

# 生成并发布全局报表
global_report = {
    "total_tenants": 2,
    "active_tenants": len(tenant_data),
    "total_messages_processed": sum_processed,
    "system_status": "operational"
}

await session.put("monitoring/global/report", json.dumps(global_report))
```

## 🔧 配置详解

### TLS配置

所有组件都使用相同的TLS配置模式：

```python
cfg = {
    "connect": {
        "endpoints": ["tls/gateway-ip:7447"]
    },
    "transport": {
        "link": {
            "tls": {
                "root_ca_certificate": "/app/certs/ca-cert.pem",
                "connect_private_key": "/app/certs/{component}-key.pem",
                "connect_certificate": "/app/certs/{component}-cert.pem"
            }
        }
    },
    "scouting": {
        "multicast": {"enabled": False},  # 生产环境禁用
        "gossip": {"enabled": False}
    }
}
```

### 证书映射

| 组件 | 私钥文件 | 证书文件 |
|-----|---------|---------|
| 租户A设备 | device-key.pem | device-cert.pem |
| 租户B设备 | tenant-b-device-key.pem | tenant-b-device-cert.pem |
| 租户A Agent | agent-key.pem | agent-cert.pem |
| 租户B Agent | tenant-b-agent-key.pem | tenant-b-agent-cert.pem |
| 数据收集器 | collector-key.pem | collector-cert.pem |

## 📊 数据流示例

### 租户A 智能工厂场景

```
1. 设备发布数据
   路径: tenant-a/sensor/industrial
   数据: {"temperature": 25.7, "pressure": 98.5, "device_id": "sensor-a-001"}

2. Agent处理数据
   订阅: tenant-a/sensor/*
   发布: tenant-a/stats/processor + stats/global/tenant-a

3. 收集器汇总
   订阅: stats/global/*
   发布: monitoring/global/report
```

### 租户B 智慧农业场景

```
1. 设备发布数据
   路径: tenant-b/sensor/agricultural
   数据: {"soil_moisture": 78.5, "soil_ph": 6.8, "device_id": "sensor-b-001"}

2. Agent处理数据
   订阅: tenant-b/sensor/*
   发布: tenant-b/stats/processor + stats/global/tenant-b

3. 收集器汇总
   订阅: stats/global/*
   发布: monitoring/global/report
```

## 🚀 运行和测试

### 1. 启动完整系统

```bash
# 生成TLS证书
./generate-tls-certs.sh

# 启动TLS多租户集群
./quick-verify-tls.sh start

# 等待系统稳定 (约30秒)
sleep 30
```

### 2. 验证数据流

```bash
# 监控数据流
./monitor-data-flow.sh

# 或者查看各组件日志
docker logs tenant-a-device -f    # 租户A设备数据发布
docker logs tenant-a-agent -f     # 租户A Agent数据处理
docker logs tenant-b-device -f    # 租户B设备数据发布
docker logs tenant-b-agent -f     # 租户B Agent数据处理
docker logs data-collector -f     # 全局数据收集
```

### 3. 手动测试发布订阅

```bash
# 外部测试发布 (需要安装zenoh CLI)
zenoh --connect tls/127.0.0.1:7447 \
  --tls-ca-certificate tls-certs/ca-cert.pem \
  --tls-client-private-key tls-certs/device-key.pem \
  --tls-client-certificate tls-certs/device-cert.pem \
  put test/manual "手动测试数据"

# 订阅测试数据
zenoh --connect tls/127.0.0.1:7447 \
  --tls-ca-certificate tls-certs/ca-cert.pem \
  --tls-client-private-key tls-certs/agent-key.pem \
  --tls-client-certificate tls-certs/agent-cert.pem \
  get test/manual
```

## 🔍 监控和调试

### 系统状态监控

```bash
# 查看所有容器状态
docker compose -f docker-compose.tls-verify.yaml ps

# 查看网络连接
docker network inspect zenoh_zenoh-network

# 查看容器资源使用
docker stats
```

### 日志分析

```bash
# 分析数据发布频率
docker logs tenant-a-device 2>&1 | grep "发布.*数据" | tail -10

# 分析处理延迟
docker logs tenant-a-agent 2>&1 | grep "processing_time_ms" | tail -5

# 分析告警情况
docker logs data-collector 2>&1 | grep "alerts_today\|total_alerts" | tail -3
```

### 性能监控

```bash
# 监控消息吞吐量
docker logs data-collector 2>&1 | grep "message_throughput_per_hour" | tail -1

# 监控系统健康状态
docker logs data-collector 2>&1 | grep "system_status\|system_efficiency" | tail -3
```

## 🐛 故障排除

### 常见问题

**1. TLS连接失败**
```
错误: certificate verify failed
解决: 检查证书文件是否存在且有效
```

**2. 数据发布失败**
```
错误: put operation failed
解决: 检查网络连接和路径权限
```

**3. 订阅无数据**
```
问题: 订阅者收不到数据
解决: 检查发布路径和订阅模式匹配
```

### 调试技巧

**启用详细日志**:
```python
import logging
logging.basicConfig(level=logging.DEBUG)
```

**测试连接**:
```bash
# 测试TLS握手
openssl s_client -connect localhost:7447 \
  -CAfile tls-certs/ca-cert.pem \
  -cert tls-certs/device-cert.pem \
  -key tls-certs/device-key.pem \
  -showcerts
```

**验证数据流**:
```python
# 在Python脚本中添加调试
print(f"发布到: {key_expr}")
print(f"数据内容: {json.dumps(data, indent=2)}")
```

## 📈 性能优化

### 连接池配置

```python
cfg = {
    "transport": {
        "unicast": {
            "max_sessions": 100,      # 最大并发会话
            "qos": {"enabled": True}   # 启用QoS
        }
    }
}
```

### 批量发布优化

```python
# 使用异步批量发布
tasks = []
for data in data_batch:
    task = session.put(key_expr, json.dumps(data))
    tasks.append(task)

await asyncio.gather(*tasks)
```

### 内存管理

```python
# 设置合适的队列大小
cfg = {
    "transport": {
        "link": {
            "tx": {
                "queue": {
                    "size": {
                        "data_high": 32,  # 高优先级队列
                        "data": 16        # 普通数据队列
                    }
                }
            }
        }
    }
}
```

## 🎯 最佳实践

### 1. 错误处理

```python
try:
    await session.put(key_expr, payload)
    print("✅ 数据发布成功")
except Exception as e:
    print(f"❌ 发布失败: {e}")
    # 实现重试逻辑
```

### 2. 资源清理

```python
# 正确关闭连接
await subscriber.undeclare()
await session.close()
```

### 3. 路径设计

```python
# 好的路径设计
tenant_a/sensor/{device_id}/temperature
tenant_a/stats/processor/{metric}
stats/global/{tenant_id}

# 避免的路径
sensor/data  # 无租户隔离
temp/value   # 含义不清
```

### 4. 安全性

```python
# 总是验证证书
cfg["transport"]["link"]["tls"]["verify_name_on_connect"] = True

# 使用强加密
# TLS 1.3 默认提供最佳安全性
```

## 🔗 相关资源

- [Zenoh Python SDK 文档](https://zenoh.io/docs/apis/python/)
- [Zenoh 配置参考](https://zenoh.io/docs/manual/configuration/)
- [TLS 配置指南](./TLS_GATEWAY_CONFIG_GUIDE.md)
- [分布式架构设计](./DISTRIBUTED_GATEWAY_ARCHITECTURE.md)

---

**🎉 Zenoh Python SDK 让多租户数据流实现变得如此简单和强大！**

通过 Python 的异步特性和 Zenoh 的高性能通信，我们可以构建出企业级的 IoT 数据处理系统，实现实时数据流、业务逻辑处理和全局监控。🚀
