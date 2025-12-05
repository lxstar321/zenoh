# Zenoh 网关认证插件

一个强大的Zenoh插件，实现基于外部鉴权服务的mTLS客户端认证和权限控制。

## 🎯 功能特性

### ✅ **已实现功能**

- **🔐 mTLS证书认证**: 自动从客户端证书中提取CN和客户端类型
- **🌐 HTTP客户端集成**: 调用外部REST API鉴权服务验证客户端身份
- **💾 权限缓存管理**: Session-based权限缓存，提高性能
- **🛡️ 消息拦截控制**: 基于权限控制发布/订阅操作，支持通配符匹配
- **📊 详细统计**: 完整的认证和访问控制统计信息
- **🧹 缓存管理**: 自动清理过期会话和权限缓存
- **🔍 调试日志**: 可配置的详细认证和访问控制日志
- **⚡ 高性能**: 异步HTTP请求和高效缓存机制

## 🏗️ 架构设计

```
┌─────────────────┐    mTLS    ┌──────────────────┐    REST API   ┌──────────────────┐
│   Zenoh Client  │────────────│   Zenoh Router   │──────────────▶│  Auth Service    │
│   (证书认证)    │            │  (网关插件)     │               │  (鉴权服务)      │
└─────────────────┘            └─────────────────┘               └──────────────────┘
         │                             │                                      │
         │                             │                                      │
         ▼                             ▼                                      ▼
   ┌─────────────────┐           ┌─────────────────┐                   ┌──────────────────┐
   │ 证书信息提取    │           │ 权限缓存        │                   │ 权限验证        │
   │ (CN + 类型)     │           │ (Session Map)   │                   │ (RBAC)          │
   └─────────────────┘           └─────────────────┘                   └──────────────────┘
```

## 📋 工作流程

### 1. 客户端连接
```
客户端 → mTLS握手 → 证书验证 → 插件激活
```

### 2. 身份提取
```
插件 → 解析证书 → 提取CN → 推断client_type
```

### 3. 鉴权调用
```
插件 → 调用REST API → 获取认证结果和权限
```

### 4. 权限缓存
```
成功认证 → 缓存到Session Map → 返回成功
```

### 5. 消息控制
```
客户端消息 → 插件检查Session缓存 → 权限匹配验证 → 允许/拒绝/记录日志
```

## 🔧 安装和编译

### 1. 编译插件

```bash
# 编译网关认证插件
cargo build --release --manifest-path plugins/zenoh-plugin-gateway-auth/Cargo.toml

# 验证编译结果
ls -la target/release/libzenoh_plugin_gateway_auth.so
```

### 2. 安装依赖

```bash
# 安装Python依赖
pip install -r requirements_auth.txt
```

## ⚙️ 配置说明

### Router配置 (`zenoh_router_gateway_config.json5`)

```json5
{
  mode: "router",
  listen: {
    endpoints: { router: ["tls/0.0.0.0:7447"] }
  },
  transport: {
    link: {
      tls: {
        root_ca_certificate: "/path/to/ca.pem",
        listen_certificate: "/path/to/server.pem",
        listen_private_key: "/path/to/server.key",
        enable_mtls: true,
        verify_name_on_connect: false
      }
    }
  },
  plugins: {
    gateway_auth: {
      auth_service_url: "http://localhost:8080",
      cache_ttl_seconds: 3600,
      enable_debug_logging: true,
      request_timeout_seconds: 10
    }
  }
}
```

### 插件配置参数

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `auth_service_url` | string | "http://localhost:8080" | 鉴权服务URL |
| `cache_ttl_seconds` | number | 3600 | 缓存过期时间(秒) |
| `enable_debug_logging` | boolean | false | 启用调试日志 |
| `request_timeout_seconds` | number | 10 | 请求超时时间(秒) |

## 🚀 使用方法

### 1. 启动鉴权服务

```bash
python3 auth_service.py --host 0.0.0.0 --port 8080
```

### 2. 启动Router (带插件)

```bash
./target/release/zenohd --config zenoh_router_gateway_config.json5
```

### 3. 启动客户端

```bash
# 传感器客户端
./target/release/zenohd --config zenoh_client1_config.json5

# 控制器客户端
./target/release/zenohd --config zenoh_client2_config.json5
```

### 4. 运行测试

```bash
# 基础功能测试
python3 test_gateway_plugin.py

# 完整认证功能测试
python3 test_gateway_plugin_auth.py
```

### 5. 测试认证功能

**启动完整测试环境：**
```bash
# 终端1: 鉴权服务
python3 auth_service.py --host 0.0.0.0 --port 8080

# 终端2: Router (带认证插件)
./target/release/zenohd --config zenoh_router_gateway_config.json5

# 终端3: 客户端测试
conda activate zenoh-test
python3 test_gateway_plugin_auth.py
```

**测试结果示例：**
```
🧪 测试鉴权API...
   ✅ 有效传感器客户端: True (期望: True)
   ✅ 有效控制器客户端: True (期望: True)
   ✅ 类型不匹配: False (期望: False)
   ✅ 未知客户端: False (期望: False)
鉴权API测试完成: 4/4 通过
```

## 📊 监控和日志

### Router日志示例

```
[GatewayAuth] 🔍 Extracted client info - ID: sensor-client-001, Type: sensor
[GatewayAuth] Calling auth service: http://localhost:8080/auth
[GatewayAuth] ✅ Client authenticated: sensor-client-001 (sensor) for session abc123
[GatewayAuth] 📝 Cached permissions for session: abc123
[GatewayAuth] ✅ Permission granted: pub:sensor/temperature/room1 for session abc123
[GatewayAuth] ❌ Permission denied: pub:control/heater/on for session abc123
```

### 插件统计信息

通过Admin Space查看：
```bash
# 查看插件状态
zenoh-cli get "@/router/plugins/gateway_auth"

# 查看统计信息
zenoh-cli get "@/router/plugins/gateway_auth/stats"
```

## 🔍 权限检查逻辑

### 支持的权限格式

```
pub:sensor/*          - 发布所有传感器数据
sub:control/*         - 订阅所有控制命令
admin:system/*        - 系统管理权限
pub:sensor/temperature/room1  - 发布特定传感器数据
```

### 通配符匹配

- `*` 表示任意字符序列
- 例如: `pub:sensor/*` 匹配 `pub:sensor/temperature/room1`

## 🧪 测试场景

### 成功认证场景

1. **传感器客户端**:
   - 证书CN: `sensor-client-001`
   - 推断类型: `sensor`
   - 权限: `pub:sensor/*`, `sub:control/sensor-client-001/*`

2. **控制器客户端**:
   - 证书CN: `control-client-001`
   - 推断类型: `controller`
   - 权限: `pub:control/*`, `sub:sensor/*`, `admin:*`

### 认证失败场景

1. **未知客户端**: 证书不在鉴权服务中
2. **类型不匹配**: CN和client_type不对应
3. **服务不可用**: 鉴权服务宕机
4. **证书无效**: 证书无法解析

## 🔧 故障排除

### 常见问题

**Q: 插件无法加载**
A: 检查插件是否正确编译，路径是否正确

**Q: 鉴权服务调用失败**
A: 检查网络连接，确认鉴权服务正在运行

**Q: 证书解析失败**
A: 确认证书格式正确，CN字段存在

**Q: 权限检查失败**
A: 检查权限格式，确认通配符匹配逻辑

### 调试模式

启用调试日志查看详细过程：

```json5
plugins: {
  gateway_auth: {
    enable_debug_logging: true
  }
}
```

## 📈 性能优化

### 缓存策略

- **Session-based缓存**: 每个会话缓存权限，避免重复认证
- **TTL过期**: 自动清理过期缓存
- **内存限制**: 防止缓存过大

### 并发处理

- **异步HTTP请求**: 非阻塞的鉴权服务调用
- **读写锁**: 高效的缓存并发访问
- **连接池**: HTTP客户端连接复用

## 🔒 安全特性

### 传输安全
- **mTLS加密**: 端到端加密通信
- **证书验证**: 双向证书认证
- **会话隔离**: 每个客户端独立会话

### 访问控制
- **最小权限**: 只授予必要权限
- **权限检查**: 每个操作都验证权限
- **审计日志**: 完整的安全事件记录

### 异常处理
- **降级策略**: 服务不可用时拒绝访问
- **超时控制**: 防止长时间等待
- **错误隔离**: 单客户端错误不影响其他客户端

## 🎯 最佳实践

### 生产环境部署

1. **高可用**: 部署多个鉴权服务实例
2. **负载均衡**: 在多个Router间分担负载
3. **监控告警**: 设置关键指标监控
4. **日志聚合**: 集中收集和分析日志

### 权限设计

1. **最小权限原则**: 只授予完成任务所需的最小权限
2. **角色分离**: 将权限按角色分组
3. **定期审查**: 定期检查和更新权限分配

### 证书管理

1. **证书轮换**: 定期更新证书
2. **吊销检查**: 实现证书吊销列表检查
3. **安全存储**: 安全存储私钥

## 📚 相关文档

- [Zenoh插件开发](https://zenoh.io/docs/manual/plugins/)
- [mTLS配置](https://zenoh.io/docs/manual/tls/)
- [REST API设计](https://restfulapi.net/)

---

**🎊 现在你拥有了一个企业级的Zenoh安全网关系统！**

该插件提供了完整的mTLS客户端认证和权限控制功能，可以直接用于生产环境的物联网、智能制造等场景。 🚀
