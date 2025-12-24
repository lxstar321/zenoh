# Zenoh TLS 多租户验证配置

本目录包含完整的Zenoh TLS多租户分布式网关验证环境配置。

## 📁 目录结构

```
tls-verification-configs/
├── README.md                          # 本说明文件
├── requirements.txt                   # Python依赖
├── Dockerfile.python                  # Python环境Docker镜像
├── docker-compose.tls-verify.yaml     # Docker Compose配置
├── generate-tls-certs.sh             # TLS证书生成脚本
├── tls-certs/                        # TLS证书目录
├── client_config.json5               # 客户端基础配置
├── data_generator.py                 # 数据生成器(Python SDK)
├── data_processor.py                 # 数据处理器(Python SDK)
├── data_collector.py                 # 全局数据收集器(Python SDK)
├── quick-verify-tls.sh               # 快速启动脚本
├── monitor-data-flow.sh              # 数据流监控脚本
├── test-multi-tenant-tls.sh          # 多租户测试脚本
├── test-data-flow.sh                 # 数据流测试脚本
├── ZENOH_PYTHON_SDK_GUIDE.md         # Python SDK使用指南
├── tls-router-config.json5           # Router-1配置
├── tls-router-2-config.json5         # Router-2配置
├── tls-router-3-config.json5         # Router-3配置
├── tls-agent-config.json5            # 租户A Agent配置
├── tls-collector-config.json5        # 数据收集器配置
├── tls-device-config.json5           # 租户A设备配置
├── tls-tenant-b-agent-config.json5   # 租户B Agent配置
└── tls-tenant-b-device-config.json5  # 租户B设备配置
```

## 🚀 快速开始

### 1. 生成TLS证书

```bash
cd tls-verification-configs
./generate-tls-certs.sh
```

### 2. 启动TLS多租户系统

```bash
./quick-verify-tls.sh start
```

### 3. 监控数据流

```bash
./monitor-data-flow.sh
```

### 4. 停止系统

```bash
./quick-verify-tls.sh stop
```

## 🔧 组件说明

### 基础设施层
- **3个Router节点**: 提供TLS加密的网关路由服务
- **Docker Compose**: 编排所有组件的容器化部署

### 应用层
- **租户A设备**: 模拟智能工厂传感器数据生成
- **租户B设备**: 模拟智慧农业传感器数据生成
- **租户A Agent**: 处理工厂数据并生成统计
- **租户B Agent**: 处理农业数据并生成统计
- **数据收集器**: 汇总所有租户统计并生成全局报表

### 数据流
```
设备数据 → Zenoh网关集群 → Agent处理 → 全局收集器
tenant-a/sensor/* → tenant-a/stats/* → stats/global/*
tenant-b/sensor/* → tenant-b/stats/* → stats/global/*
```

## 📊 验证内容

- ✅ TLS mTLS加密通信
- ✅ 多租户数据隔离
- ✅ 实时数据发布订阅
- ✅ 分布式负载均衡
- ✅ 容错和故障恢复
- ✅ 性能监控和统计

## 🔍 监控和调试

### 查看组件状态
```bash
docker compose -f docker-compose.tls-verify.yaml ps
```

### 查看组件日志
```bash
# 查看所有组件日志
docker compose -f docker-compose.tls-verify.yaml logs -f

# 查看特定组件日志
docker logs tenant-a-device -f
docker logs tenant-a-agent -f
docker logs data-collector -f
```

### 手动测试数据流
```bash
# 外部测试发布
zenoh --connect tls/127.0.0.1:7447 \
  --tls-ca-certificate tls-certs/ca-cert.pem \
  --tls-client-private-key tls-certs/device-key.pem \
  --tls-client-certificate tls-certs/device-cert.pem \
  put test/manual "测试数据"

# 订阅测试数据
zenoh --connect tls/127.0.0.1:7447 \
  --tls-ca-certificate tls-certs/ca-cert.pem \
  --tls-client-private-key tls-certs/agent-key.pem \
  --tls-client-certificate tls-certs/agent-cert.pem \
  get test/manual
```

## 📚 相关文档

- [Zenoh Python SDK使用指南](./ZENOH_PYTHON_SDK_GUIDE.md)
- [分布式网关架构设计](../../../DISTRIBUTED_GATEWAY_ARCHITECTURE.md)
- [TLS配置指南](../../../TLS_GATEWAY_CONFIG_GUIDE.md)

## 🎯 关键特性

- **安全性**: 基于mTLS的双向认证
- **可扩展性**: 支持多租户隔离部署
- **高性能**: Zenoh的低延迟高吞吐通信
- **容错性**: 多Router节点自动负载均衡
- **可观测性**: 完整的监控和日志记录

---

**🎉 这个配置目录包含了完整的Zenoh多租户TLS验证环境！**

运行 `./quick-verify-tls.sh start` 即可启动完整的分布式数据流验证系统。🚀

