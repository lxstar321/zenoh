# Zenoh多客户端ACL配置指南

## 概述

本文档详细介绍如何为多个不同类型的客户端配置细粒度的访问控制权限。通过组合`rules`、`subjects`和`policies`，可以实现企业级的访问控制，既保证安全性又保持灵活性。

## 配置架构

```
规则层 (Rules)          主体层 (Subjects)          策略层 (Policies)
├── sensor-publish-only  ├── temperature-sensor  ├── temperature-sensor-policy
├── monitor-read-all     ├── monitoring-system   ├── monitoring-policy
├── control-commands     ├── control-system      ├── control-policy
└── admin-full-access    └── admin-client        └── admin-policy
```

## 完整配置文件

```json5
/// 多客户端ACL配置示例
/// 演示如何为不同类型的客户端配置不同的访问权限
{
  /// 节点模式设置为路由器
  mode: "router",

  /// 节点元数据
  metadata: {
    name: "multi-client-acl-router",
    location: "secure-zone",
  },

  /// 监听配置 - 使用 TLS 端点
  listen: {
    endpoints: ["tls/0.0.0.0:8447"],
    exit_on_failure: true,
  },

  /// 传输层配置 - 启用 mTLS
  transport: {
    link: {
      tls: {
        root_ca_certificate: "./certs/ca-cert.pem",
        listen_private_key: "./certs/server-key.pem",
        listen_certificate: "./certs/server-cert.pem",
        enable_mtls: true,
        verify_name_on_connect: true,
        close_link_on_expiration: true,
      },
    },
  },

  /// 管理空间配置
  adminspace: {
    enabled: true,
    permissions: {
      read: true,
      write: true,
    },
  },

  /// 访问控制列表配置
  access_control: {
    enabled: true,
    default_permission: "deny",  // 默认拒绝所有未明确允许的操作

    /// 规则定义 - 为不同客户端类型定义不同的权限规则
    rules: [
      /// 传感器客户端规则 - 只允许发布传感器数据
      {
        id: "sensor-publish-only",
        messages: ["put"],  // 只允许发布数据
        flows: ["ingress", "egress"],
        permission: "allow",
        key_exprs: ["sensor/**"],  // 只允许传感器相关话题
      },
      /// 传感器客户端规则 - 允许订阅配置更新
      {
        id: "sensor-config-subscribe",
        messages: ["declare_subscriber"],
        flows: ["ingress", "egress"],
        permission: "allow",
        key_exprs: ["config/sensor/**"],  // 配置相关话题
      },

      /// 监控客户端规则 - 允许读取所有传感器数据
      {
        id: "monitor-read-all",
        messages: ["declare_subscriber", "query"],
        flows: ["ingress", "egress"],
        permission: "allow",
        key_exprs: ["sensor/**", "status/**"],  // 传感器和状态数据
      },

      /// 监控客户端规则 - 允许发布监控事件
      {
        id: "monitor-publish-events",
        messages: ["put"],
        flows: ["ingress", "egress"],
        permission: "allow",
        key_exprs: ["monitor/events/**"],  // 监控事件话题
      },

      /// 控制客户端规则 - 允许发布控制命令
      {
        id: "control-publish-commands",
        messages: ["put"],
        flows: ["ingress", "egress"],
        permission: "allow",
        key_exprs: ["control/**"],  // 控制命令话题
      },
      /// 控制客户端规则 - 允许订阅传感器数据用于决策
      {
        id: "control-subscribe-sensors",
        messages: ["declare_subscriber"],
        flows: ["ingress", "egress"],
        permission: "allow",
        key_exprs: ["sensor/**", "status/**"],
      },

      /// 管理员规则 - 完全访问权限
      {
        id: "admin-full-access",
        messages: ["put", "delete", "declare_subscriber", "query", "declare_queryable", "reply"],
        flows: ["ingress", "egress"],
        permission: "allow",
        key_exprs: ["**"],  // 所有话题
      },

      /// 所有客户端都可以访问的管理空间
      {
        id: "admin-space-access",
        messages: ["put", "delete", "declare_subscriber", "query", "declare_queryable", "reply"],
        flows: ["ingress", "egress"],
        permission: "allow",
        key_exprs: ["@/**"],  // 管理空间
      },
    ],

    /// 主体定义 - 为不同类型的客户端定义身份标识
    subjects: [
      /// 传感器客户端 - 使用温度传感器证书
      {
        id: "temperature-sensor",
        cert_common_names: ["temperature-sensor-001"],
        interfaces: ["eth0", "wlan0"],  // 允许有线和无线连接
      },
      /// 传感器客户端 - 使用湿度传感器证书
      {
        id: "humidity-sensor",
        cert_common_names: ["humidity-sensor-001"],
        interfaces: ["eth0", "wlan0"],
      },
      /// 监控客户端 - 监控系统证书
      {
        id: "monitoring-system",
        cert_common_names: ["monitoring-system"],
        interfaces: ["eth0"],  // 只允许有线连接，安全性更高
      },
      /// 控制客户端 - 控制系统证书
      {
        id: "control-system",
        cert_common_names: ["control-system"],
        interfaces: ["eth0"],  // 只允许有线连接
      },
      /// 管理员客户端 - 管理员证书
      {
        id: "admin-client",
        cert_common_names: ["admin-console", "admin-api"],
        interfaces: ["lo", "eth0"],  // 允许本地和有线连接
      },
      /// 开发调试客户端 - 开发环境证书
      {
        id: "dev-client",
        cert_common_names: ["dev-client", "test-client"],
        interfaces: ["lo", "eth0", "wlan0"],  // 开发环境更宽松
      },
    ],

    /// 策略配置 - 将主体和规则关联起来
    policies: [
      /// 温度传感器策略 - 只允许发布温度数据和订阅配置
      {
        id: "temperature-sensor-policy",
        rules: ["sensor-publish-only", "sensor-config-subscribe", "admin-space-access"],
        subjects: ["temperature-sensor"],
      },

      /// 湿度传感器策略 - 只允许发布湿度数据和订阅配置
      {
        id: "humidity-sensor-policy",
        rules: ["sensor-publish-only", "sensor-config-subscribe", "admin-space-access"],
        subjects: ["humidity-sensor"],
      },

      /// 监控系统策略 - 可以读取所有传感器数据和发布监控事件
      {
        id: "monitoring-policy",
        rules: ["monitor-read-all", "monitor-publish-events", "admin-space-access"],
        subjects: ["monitoring-system"],
      },

      /// 控制系统策略 - 可以发布控制命令和读取传感器数据
      {
        id: "control-policy",
        rules: ["control-publish-commands", "control-subscribe-sensors", "admin-space-access"],
        subjects: ["control-system"],
      },

      /// 管理员策略 - 完全访问权限
      {
        id: "admin-policy",
        rules: ["admin-full-access"],
        subjects: ["admin-client"],
      },

      /// 开发调试策略 - 在开发环境中给予更宽松的权限
      {
        id: "dev-policy",
        rules: ["monitor-read-all", "control-publish-commands", "sensor-publish-only", "admin-space-access"],
        subjects: ["dev-client"],
      },
    ],
  },
}
```

## 权限矩阵

| 客户端类型 | sensor/** | control/** | monitor/** | config/** | @/** (管理空间) |
|-----------|-----------|------------|------------|-----------|----------------|
| 温度传感器 | 是 发布 | 否 | 否 | 是 订阅 | 是 只读 |
| 湿度传感器 | 是 发布 | 否 | 否 | 是 订阅 | 是 只读 |
| 监控系统 | 是 订阅查询 | 否 | 是 发布 | 否 | 是 只读 |
| 控制系统 | 是 订阅 | 是 发布 | 否 | 否 | 是 只读 |
| 管理员 | 是 完全访问 | 是 完全访问 | 是 完全访问 | 是 完全访问 | 是 完全访问 |
| 开发客户端 | 是 发布订阅 | 是 发布 | 否 | 否 | 是 只读 |

## 使用方法

### 1. 生成证书

为不同客户端生成对应的证书：

```bash
# 传感器证书
./generate_mtls_certs.sh temperature-sensor-001
./generate_mtls_certs.sh humidity-sensor-001

# 系统证书
./generate_mtls_certs.sh monitoring-system
./generate_mtls_certs.sh control-system

# 管理员证书
./generate_mtls_certs.sh admin-console
./generate_mtls_certs.sh admin-api

# 开发证书
./generate_mtls_certs.sh dev-client
```

### 2. 启动路由器

```bash
./target/release/zenohd --config multi_client_acl_config.json5
```

### 3. 客户端连接示例

#### Python - 温度传感器客户端

```python
#!/usr/bin/env python3
import zenoh
import time

def create_sensor_config():
    """创建温度传感器配置"""
    config = zenoh.Config()
    config.insert_json5('mode', '"client"')
    config.insert_json5('connect/endpoints', '["tls/localhost:8447"]')
    config.insert_json5('transport/link/tls/enable_mtls', 'true')
    config.insert_json5('transport/link/tls/root_ca_certificate', '"./certs/ca-cert.pem"')
    config.insert_json5('transport/link/tls/connect_certificate', '"./certs/temperature-sensor-001-cert.pem"')
    config.insert_json5('transport/link/tls/connect_private_key', '"./certs/temperature-sensor-001-key.pem"')
    return config

def main():
    session = zenoh.open(create_sensor_config())

    # 只允许发布传感器数据
    while True:
        temperature = 25.5 + (time.time() % 10)  # 模拟温度变化
        session.put("sensor/temperature/001", ".1f")
        time.sleep(1)

if __name__ == "__main__":
    main()
```

#### Python - 监控系统客户端

```python
#!/usr/bin/env python3
import zenoh

def message_handler(sample):
    """处理传感器数据"""
    print(f"收到数据: {sample.key_expr} = {sample.payload}")

def create_monitor_config():
    """创建监控系统配置"""
    config = zenoh.Config()
    config.insert_json5('mode', '"client"')
    config.insert_json5('connect/endpoints', '["tls/localhost:8447"]')
    config.insert_json5('transport/link/tls/enable_mtls', 'true')
    config.insert_json5('transport/link/tls/root_ca_certificate', '"./certs/ca-cert.pem"')
    config.insert_json5('transport/link/tls/connect_certificate', '"./certs/monitoring-system-cert.pem"')
    config.insert_json5('transport/link/tls/connect_private_key', '"./certs/monitoring-system-key.pem"')
    return config

def main():
    session = zenoh.open(create_monitor_config())

    # 可以订阅所有传感器数据
    subscriber = session.declare_subscriber("sensor/**", message_handler)

    # 可以发布监控事件
    session.put("monitor/events/system-health", "OK")

    input("按Enter退出...")

if __name__ == "__main__":
    main()
```

#### Python - 控制系统客户端

```python
#!/usr/bin/env python3
import zenoh

def create_control_config():
    """创建控制系统配置"""
    config = zenoh.Config()
    config.insert_json5('mode', '"client"')
    config.insert_json5('connect/endpoints', '["tls/localhost:8447"]')
    config.insert_json5('transport/link/tls/enable_mtls', 'true')
    config.insert_json5('transport/link/tls/root_ca_certificate', '"./certs/ca-cert.pem"')
    config.insert_json5('transport/link/tls/connect_certificate', '"./certs/control-system-cert.pem"')
    config.insert_json5('transport/link/tls/connect_private_key', '"./certs/control-system-key.pem"')
    return config

def main():
    session = zenoh.open(create_control_config())

    # 可以发布控制命令
    session.put("control/fan/speed", "medium")

    # 可以订阅传感器数据用于决策
    subscriber = session.declare_subscriber("sensor/temperature/001", lambda sample: print(f"温度: {sample.payload}"))

    input("按Enter退出...")

if __name__ == "__main__":
    main()
```

## 测试验证

### 运行ACL测试

```bash
# 测试脚本会自动验证不同客户端的权限
./test_acl.sh
```

### 手动测试

```bash
# 1. 启动路由器
./target/release/zenohd --config multi_client_acl_config.json5

# 2. 在另一个终端测试传感器客户端（应该成功）
python temperature_sensor.py

# 3. 在另一个终端测试监控客户端（应该成功）
python monitoring_system.py

# 4. 在另一个终端测试越权访问（应该失败）
# 尝试让温度传感器订阅control/**话题，应该被拒绝
```

## 配置扩展

### 添加新客户端类型

```json5
// 1. 添加新规则
{
  id: "new-sensor-rule",
  messages: ["put", "declare_subscriber"],
  flows: ["ingress", "egress"],
  permission: "allow",
  key_exprs: ["new/sensor/**"]
}

// 2. 添加新主体
{
  id: "pressure-sensor",
  cert_common_names: ["pressure-sensor-001"],
  interfaces: ["eth0"]
}

// 3. 创建新策略
{
  id: "pressure-sensor-policy",
  rules: ["new-sensor-rule", "admin-space-access"],
  subjects: ["pressure-sensor"]
}
```

### 修改现有权限

```json5
// 扩展温度传感器的权限
{
  id: "temperature-sensor-policy",
  rules: [
    "sensor-publish-only",
    "sensor-config-subscribe",
    "admin-space-access",
    "new-permission-rule"  // 添加新权限
  ],
  subjects: ["temperature-sensor"]
}
```

## 安全特性

1. **证书认证**: 每个客户端使用唯一证书进行身份验证
2. **接口限制**: 管理员和控制系统只允许特定网络接口
3. **话题隔离**: 不同客户端只能访问授权的话题
4. **操作限制**: 传感器只能发布，监控只能读取
5. **最小权限**: 遵循最小权限原则

## 故障排除

### 常见问题

1. **连接被拒绝**
   - 检查证书CN是否与subjects配置匹配
   - 确认证书未过期
   - 验证CA证书路径是否正确

2. **权限不足**
   - 检查policies是否正确关联了rules和subjects
   - 验证key_exprs模式是否正确
   - 确认消息类型是否在允许列表中

3. **配置错误**
   - 使用`--config multi_client_acl_config.json5`启动
   - 检查JSON5语法是否正确
   - 查看路由器日志中的错误信息

### 调试技巧

```bash
# 启用详细日志
export RUST_LOG=zenoh=debug
./target/release/zenohd --config multi_client_acl_config.json5

# 查看证书信息
openssl x509 -in certs/client-cert.pem -text -noout
```

## 相关文档

- [Zenoh ACL配置参考](https://docs.zenoh.io/manual/security/#access-control-list-acl)
- [TLS证书生成指南](./MTLS_README.md)
- [ACL测试指南](./MTLS_TEST_README.md)

---

*本文档提供了完整的多客户端ACL配置示例和使用指南。配置基于最小权限原则，既保证了系统的安全性，又提供了灵活的权限管理能力。*
