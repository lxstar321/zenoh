# Zenoh mTLS 通信测试指南

## 📋 概述

本项目提供了完整的 Zenoh mTLS (双向 TLS) 通信测试环境，包括：
- 启用 mTLS 的路由器配置
- 客户端配置
- Python 发布者和订阅者客户端
- 自动化证书生成脚本

## 🛠️ 环境要求

- **Conda 环境**: `zenoh-test` (已包含 zenoh-python)
- **证书**: 自动生成的测试证书
- **端口**: 8447 (TLS)

## 🚀 快速开始

### 1. 生成证书
```bash
./generate_mtls_certs.sh
```

### 2. 运行测试检查
```bash
./test_mtls.sh
```

### 2.1 ACL 功能测试 (可选)
```bash
./test_acl.sh
```

### 3. 启动测试 (需要 3 个终端)

**终端 1 - 启动路由器:**
```bash
./start_router.sh
```

**终端 2 - 启动订阅者:**
```bash
conda activate zenoh-test
python subscriber_mtls.py
```

**终端 3 - 启动发布者:**
```bash
conda activate zenoh-test
python publisher_mtls.py
```

## 📁 文件说明

| 文件 | 说明 |
|------|------|
| `mtls_config_example.json5` | 路由器 mTLS 配置 |
| `mtls_acl_config.json5` | 路由器 mTLS + ACL 配置 |
| `client_mtls_config.json5` | 客户端 mTLS 配置 |
| `generate_mtls_certs.sh` | 证书生成脚本 |
| `start_router.sh` | 路由器启动脚本 |
| `publisher_mtls.py` | Python 发布者客户端 |
| `subscriber_mtls.py` | Python 订阅者客户端 |
| `test_mtls.sh` | 测试环境检查脚本 |
| `certs/` | 证书文件目录 |

## 🔧 配置详解

### 路由器配置要点
```json5
{
  mode: "router",
  listen: {
    endpoints: ["tls/0.0.0.0:8447"]
  },
  transport: {
    link: {
      tls: {
        enable_mtls: true,
        root_ca_certificate: "./certs/ca-cert.pem",
        listen_certificate: "./certs/server-cert.pem",
        listen_private_key: "./certs/server-key.pem"
      }
    }
  }
}
```

### ACL 配置要点
```json5
{
  access_control: {
    enabled: true,
    default_permission: "deny",  // 默认拒绝
    rules: [
      {
        id: "sensor-allow",
        messages: ["put", "declare_subscriber"],
        flows: ["ingress", "egress"],
        permission: "allow",
        key_exprs: ["sensor/*"]  // 允许传感器话题
      }
    ]
  }
}
```

### 客户端配置要点
```json5
{
  mode: "client",
  connect: {
    endpoints: ["tls/localhost:8447"]
  },
  transport: {
    link: {
      tls: {
        enable_mtls: true,
        root_ca_certificate: "./certs/ca-cert.pem",
        connect_certificate: "./certs/client-cert.pem",
        connect_private_key: "./certs/client-key.pem"
      }
    }
  }
}
```

## 📊 测试结果验证

### 成功标志
- ✅ 路由器启动成功，显示 TLS 监听
- ✅ 订阅者连接成功，无错误信息
- ✅ 发布者连接成功，开始发送消息
- ✅ 订阅者接收到消息并正确解析 JSON

### 示例输出

**路由器输出:**
```
[INFO] Router mode
[INFO] Listening on tls/0.0.0.0:8447
[INFO] mTLS authentication enabled
```

**订阅者输出:**
```
INFO:连接到 Zenoh 路由器 (使用 mTLS)...
INFO:成功建立 mTLS 连接！
INFO:订阅者已启动，正在监听消息...
INFO:收到消息: ID=0, 时间戳=1701436800.12
INFO:消息内容: Hello from mTLS publisher! Message #0
INFO:安全状态: mTLS encrypted
```

**发布者输出:**
```
INFO:连接到 Zenoh 路由器 (使用 mTLS)...
INFO:成功建立 mTLS 连接！
INFO:开始发布消息...
INFO:已发布消息 #0: Hello from mTLS publisher! Message #0
```

## 🔒 安全特性验证

通过本测试，您可以验证以下安全特性：

1. **双向认证**: 客户端和服务器都验证对方证书
2. **加密传输**: 所有消息都通过 TLS 1.3 加密
3. **证书验证**: 验证证书链和过期时间
4. **身份认证**: 只有持有有效证书的客户端才能连接

## 🐛 故障排除

### 常见问题

**问题**: 连接失败
```bash
# 检查证书文件权限
ls -la certs/

# 验证证书
openssl x509 -in certs/ca-cert.pem -text -noout
```

**问题**: 导入错误
```bash
# 确保使用正确的 conda 环境
conda activate zenoh-test
pip list | grep zenoh
```

**问题**: 端口占用
```bash
# 检查端口使用情况
netstat -tlnp | grep 8447

# 杀死占用进程
kill -9 <PID>
```

### 日志调试

启用详细日志：
```python
import logging
logging.basicConfig(level=logging.DEBUG)
```

## 🎯 扩展测试

### 多客户端测试
同时运行多个发布者和订阅者来测试并发通信。

### 性能测试
修改发布频率来测试高负载下的 mTLS 性能。

### 网络测试
在不同网络环境下测试 mTLS 连接稳定性。

### ACL 访问控制测试
```bash
# 运行ACL功能测试
./test_acl.sh
```

测试内容包括：
- 允许话题访问 (sensor/*)
- 拒绝话题访问 (blocked/*)
- ACL规则验证

## 📚 参考资料

- [Zenoh Python API](https://zenoh-python.readthedocs.io/)
- [Zenoh 安全配置](https://zenoh.io/docs/manual/security/)
- [TLS 1.3 规范](https://tools.ietf.org/rfc/rfc8446.txt)
