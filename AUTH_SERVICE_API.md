# Zenoh 认证服务 REST API 规范

## 概述

认证服务为Zenoh动态ACL系统提供客户端身份验证和权限查询服务。服务直接返回Zenoh兼容的ACL配置结构，减少中间格式转换。

## API端点

### POST /auth

客户端身份验证和权限查询接口。

#### 请求格式

```json
{
  "client_id": "sensor-client-001",
  "client_type": "sensor"
}
```

**字段说明：**
- `client_id`: 客户端标识符（通常来自TLS证书的CN字段）
- `client_type`: 客户端类型（可选，用于区分不同类型的客户端）

#### 成功响应格式

```json
{
  "is_authorized": true,
  "rules": [
    {
      "id": "sensor-temp-pub",
      "key_exprs": ["sensor/temperature/*"],
      "messages": ["put"],
      "flows": ["ingress"],
      "permission": "allow"
    },
    {
      "id": "sensor-status-pub",
      "key_exprs": ["sensor/status/sensor-001"],
      "messages": ["put"],
      "flows": ["ingress"],
      "permission": "allow"
    },
    {
      "id": "sensor-control-sub",
      "key_exprs": ["control/sensor-001/*"],
      "messages": ["declare_subscriber"],
      "flows": ["ingress"],
      "permission": "allow"
    }
  ],
  "client_info": {
    "name": "温度传感器",
    "organization": "智能制造部"
  }
}
```

**重要说明**：
- ✅ **subjects和policies由Zenoh自动生成**，认证服务无需提供
- ✅ **rules.id唯一性由Zenoh检查和修复**，避免冲突
- ✅ **格式大幅简化**，认证服务只需关注权限规则定义

### subject_config字段说明

`subject_config` 是可选字段，用于提供额外的subject配置信息，让Zenoh生成的subject更加精确和安全。

#### 字段结构
```json
{
  "subject_config": {
    "interfaces": ["eth0", "wlan0"],
    "link_protocols": ["tls", "quic"],
    "usernames": ["admin", "operator"],
    "zids": ["instance-123", "instance-456"]
  }
}
```

#### 子字段说明

- `interfaces` (可选): 允许的网络接口名称数组
  - 限制客户端只能从指定网络接口连接
  - 例如：`["eth0"]` 表示只能从eth0接口连接

- `link_protocols` (可选): 允许的链路协议数组
  - 限制客户端只能使用指定的传输协议
  - 可选值：`"tcp"`, `"udp"`, `"tls"`, `"quic"`, `"serial"`, `"unixpipe"`, `"unixsock-stream"`, `"vsock"`, `"ws"`, `"http"`

- `usernames` (可选): 允许的用户名数组
  - 用于用户名密码认证的额外验证
  - 例如：`["admin", "operator"]`

- `zids` (可选): 允许的Zenoh实例ID数组
  - 限制客户端只能连接到指定的Zenoh实例
  - 格式为Zenoh实例ID的字符串表示

#### 错误响应格式

```json
{
  "is_authorized": false,
  "error_message": "客户端不存在或未激活",
  "error_code": "CLIENT_NOT_FOUND"
}
```

## 数据结构说明

### Rules智能去重和唯一性处理

**Zenoh采用内容-based的智能去重策略**：

#### 去重原理
```rust
// 比较规则内容（忽略ID）：
RuleContent {
    key_exprs: ["sensor/temperature/*"],
    messages: ["put"],
    flows: ["ingress"],
    permission: "allow"
}

// 如果内容相同 → 复用已有规则ID
// 如果内容不同 → 分配新的唯一ID
```

#### 场景1: 相同权限规则的多个客户端
```json
// 客户端A请求：
{
  "client_id": "sensor-001",
  "client_type": "sensor"
}

// 返回规则：
"rules": [
  {
    "id": "temp-sensor-pub",
    "key_exprs": ["sensor/temperature/*"],
    "messages": ["put"],
    "flows": ["ingress"],
    "permission": "allow"
  }
]

// 客户端B请求相同规则：
"rules": [
  {
    "id": "temp-sensor-pub",  // 相同ID
    "key_exprs": ["sensor/temperature/*"], // 相同内容
    "messages": ["put"],
    "flows": ["ingress"],
    "permission": "allow"
  }
]
```

**处理结果**：
- ✅ **复用规则**: 两个客户端共享同一个规则ID，无冗余
- ✅ **自动关联**: Policy会自动关联到相应的rules

#### 场景2: 不同权限规则的ID冲突
```json
// 客户端A返回：
"rules": [
  {
    "id": "sensor-rule",
    "key_exprs": ["sensor/temperature/*"],
    "messages": ["put"],
    "permission": "allow"
  }
]

// 客户端B返回不同规则但相同ID：
"rules": [
  {
    "id": "sensor-rule",  // 相同ID
    "key_exprs": ["sensor/humidity/*"], // 不同内容
    "messages": ["put"],
    "permission": "allow"
  }
]
```

**处理结果**：
- ✅ **内容不同**: 为客户端B分配新ID（如`"sensor-b-sensor-rule"`）
- ✅ **避免冲突**: 确保全局ID唯一性

#### 场景3: 跨客户端规则内容复用
```json
// 🔸 客户端A（传感器）返回：
"rules": [
  {
    "id": "sensor-status-sub",  // 客户端A的ID命名
    "key_exprs": ["system/status"],
    "messages": ["declare_subscriber"],
    "flows": ["ingress"],
    "permission": "allow"
  }
]

// 🔸 客户端B（控制器）返回相同规则内容：
"rules": [
  {
    "id": "controller-status-monitor",  // 客户端B的ID命名（不同）
    "key_exprs": ["system/status"],     // 但内容完全相同
    "messages": ["declare_subscriber"],
    "flows": ["ingress"],
    "permission": "allow"
  }
]
```

**处理结果**：
- ✅ **全局复用**: 虽然两个客户端使用了不同的ID，但规则内容相同，Zenoh只创建一条规则实例
- ✅ **跨客户端共享**: 两个客户端的权限都基于同一条规则定义
- ✅ **内存优化**: 避免重复规则定义，提升系统性能

### 三个场景对比

| 场景 | 范围 | 冲突原因 | 处理策略 | 目标 |
|------|------|----------|----------|------|
| **场景1**: 单客户端ID唯一性 | 单个客户端内部 | 同一客户端的rules有重复ID | 重命名冲突ID | 避免单个客户端的配置错误 |
| **场景2**: 不同权限的ID冲突 | 单个客户端内部 | 同一客户端有不同权限但相同ID | 重命名以区分不同规则 | 确保权限规则的唯一区分 |
| **场景3**: 跨客户端内容复用 | 多个客户端之间 | 不同客户端的规则内容完全相同 | 复用规则实例，忽略ID差异 | 优化内存使用，避免冗余 |

### 处理策略

1. **基于内容而非ID**: Zenoh比较规则的实际内容（key_exprs + messages + flows + permission）
2. **全局去重**: 相同内容规则无论来自哪个客户端都复用同一个实例
3. **ID智能处理**: 仅在规则内容不同时才需要ID唯一性
4. **内存优化**: 通过复用避免创建重复的规则定义
5. **配置灵活性**: 认证服务可以为相同权限使用不同的ID命名，提高可读性

**核心思想**: ID是给人看的标签，内容才是系统关心的本质

### Rule结构完整定义

**认证服务API中的Rule格式**：

```json
{
  "id": "sensor-temp-pub",
  "key_exprs": ["sensor/temperature/*"],
  "messages": ["put", "delete"],
  "flows": ["ingress"],
  "permission": "allow"
}
```

**字段详细说明**：

#### 必需字段

- `id` (string): 规则标识符
  - 用于规则的唯一标识
  - Zenoh会自动处理ID冲突，确保全局唯一性
  - 建议使用描述性名称，如 `"sensor-temp-pub"`

- `key_exprs` (array of strings): key表达式列表
  - 至少包含一个key表达式
  - 支持Zenoh通配符：`*` (任意级别), `**` (递归)
  - 示例：`["sensor/temperature/*", "sensor/humidity/**"]`

- `messages` (array of strings): 允许的消息类型
  - 至少包含一个消息类型
  - 可选值：
    - `"put"`: 发布消息
    - `"delete"`: 删除消息
    - `"declare_subscriber"`: 订阅声明
    - `"query"`: 查询请求
    - `"declare_queryable"`: 查询处理器声明
    - `"reply"`: 查询响应
    - `"liveliness_token"`: 活跃度令牌声明
    - `"declare_liveliness_subscriber"`: 活跃度订阅声明
    - `"liveliness_query"`: 活跃度查询

- `permission` (string): 权限类型
  - `"allow"`: 允许访问
  - `"deny"`: 拒绝访问

#### 可选字段

- `flows` (array of strings, 可选): 流量方向
  - 默认值：`["ingress"]`
  - 可选值：
    - `"ingress"`: 入站流量（客户端发送到Zenoh的流量）
    - `"egress"`: 出站流量（Zenoh发送到客户端的流量）
  - 大多数情况下使用 `"ingress"` 即可

### 高级配置示例

#### 传感器客户端规则
```json
{
  "id": "sensor-comprehensive",
  "key_exprs": [
    "sensor/temperature/*",
    "sensor/humidity/**",
    "sensor/status/sensor-001"
  ],
  "messages": ["put", "declare_subscriber"],
  "flows": ["ingress"],
  "permission": "allow"
}
```

#### 控制器客户端规则（包含管理权限）
```json
{
  "id": "controller-full-access",
  "key_exprs": ["control/**", "system/status"],
  "messages": [
    "put",
    "delete",
    "query",
    "declare_subscriber",
    "declare_queryable",
    "reply"
  ],
  "flows": ["ingress", "egress"],
  "permission": "allow"
}
```

#### 监控客户端规则
```json
{
  "id": "monitor-read-only",
  "key_exprs": [
    "sensor/**",
    "control/**",
    "system/monitoring/**",
    "alert/**"
  ],
  "messages": ["declare_subscriber", "query", "reply"],
  "flows": ["ingress"],
  "permission": "allow"
}
```

#### 拒绝规则示例
```json
{
  "id": "deny-admin-access",
  "key_exprs": ["admin/**"],
  "messages": ["put", "delete", "query"],
  "flows": ["ingress", "egress"],
  "permission": "deny"
}
```

### Subjects和Policies自动生成

**认证服务无需提供subjects和policies**，Zenoh会根据以下规则自动生成：

#### Subject自动生成规则
```rust
// 基于客户端证书CN自动生成
{
  "id": "dynamic-subject-{client_id}",
  "cert_common_names": ["{client_id}"],
  "interfaces": null,           // 可扩展：限制网络接口
  "usernames": null,            // 可扩展：用户名认证
  "link_protocols": null,       // 可扩展：限制传输协议
  "zids": null                  // 可扩展：限制Zenoh实例
}
```

#### Subject字段详细说明

**自动生成字段**：
- `id`: `"dynamic-subject-{client_id}"` - 自动生成唯一标识符
- `cert_common_names`: `["{client_id}"]` - 基于TLS证书CN自动设置

**可扩展字段（当前设为null，未来可配置）**：
- `interfaces`: 网络接口限制（如`["eth0", "wlan0"]`）
- `usernames`: 用户名认证（如`["admin", "operator"]`）
- `link_protocols`: 传输协议限制
  - `"tcp"`, `"udp"`, `"tls"`, `"quic"`
  - `"serial"`, `"unixpipe"`, `"unixsock-stream"`
  - `"vsock"`, `"ws"`, `"http"`
- `zids`: Zenoh实例ID限制（如`["1234567890abcdef"]`）

### 未来扩展场景

#### 场景1: 网络接口限制
```json
// 只允许从特定网络接口连接
{
  "id": "dynamic-subject-sensor-001",
  "cert_common_names": ["sensor-client-001"],
  "interfaces": ["eth0"],              // 只能从eth0接口连接
  "link_protocols": ["tls"],           // 只能使用TLS连接
  "usernames": null,
  "zids": null
}
```

#### 场景2: 多租户隔离
```json
// 不同租户使用不同的Zenoh实例
{
  "id": "dynamic-subject-tenant-a-sensor",
  "cert_common_names": ["tenant-a-sensor-*"],
  "interfaces": null,
  "link_protocols": null,
  "usernames": null,
  "zids": ["tenant-a-instance-id"]     // 只允许连接到特定Zenoh实例
}
```

#### 场景3: 混合认证
```json
// TLS证书 + 用户名双重认证
{
  "id": "dynamic-subject-admin-user",
  "cert_common_names": ["admin-client"],
  "interfaces": ["eth0"],
  "link_protocols": ["tls"],
  "usernames": ["admin", "root"],      // 证书+用户名双重验证
  "zids": null
}
```

#### Policy自动生成规则

**设计选择：每个client一个policy，统一管理权限过期**

```rust
// 为每个客户端生成一个policy，关联该客户端的所有rules
// 这样可以统一管理客户端权限的过期时间
{
  "id": "dynamic-policy-sensor-001",
  "rules": ["sensor-temp-pub", "sensor-status-pub", "sensor-control-sub"],
  "subjects": ["dynamic-subject-sensor-001"]
}
```

**为什么选择每个client一个policy？**
- ✅ **权限一致性**: 客户端的所有权限一起生效或过期
- ✅ **缓存管理**: 通过统一的TTL控制客户端权限生命周期
- ✅ **业务合理性**: 客户端权限通常是整体的概念

## 基础日志存档

### 认证服务日志格式

认证服务应以结构化格式记录关键事件，便于后续分析：

#### 认证请求
```
2024-01-15 10:30:45 INFO [auth_service] AUTH_REQUEST client_id=sensor-001 client_type=sensor request_id=req-12345
```

#### 认证成功响应
```
2024-01-15 10:30:45 INFO [auth_service] AUTH_SUCCESS client_id=sensor-001 rules_count=3 processing_time_ms=150
```

#### 认证失败响应
```
2024-01-15 10:30:45 ERROR [auth_service] AUTH_FAILURE client_id=invalid-sensor error_code=CLIENT_NOT_FOUND error_message="客户端不存在" processing_time_ms=50
```

### 日志文件管理

#### 文件轮转配置
```python
import logging.handlers

# 按大小轮转（推荐）
handler = logging.handlers.RotatingFileHandler(
    'logs/auth_service.log',
    maxBytes=100*1024*1024,  # 100MB
    backupCount=30  # 保留30个历史文件
)

# 或者按时间轮转
handler = logging.handlers.TimedRotatingFileHandler(
    'logs/auth_service.log',
    when='midnight',  # 每天午夜轮转
    interval=1,
    backupCount=30   # 保留30天的日志
)
```

#### 存储结构
```
logs/
├── dynamic_acl.log          # 动态ACL统一日志
├── dynamic_acl.log.1        # 轮转文件
├── dynamic_acl.log.2.gz     # 压缩历史文件
└── ...                      # 最多保留30个历史文件
```

### 日志分析命令

#### 统计认证成功率
```bash
# 计算成功率
total=$(grep "\[AUTH_" logs/dynamic_acl.log | wc -l)
success=$(grep "\[AUTH_SUCCESS\]" logs/dynamic_acl.log | wc -l)
echo "成功率: $((success * 100 / total))%"

# 查看失败原因分布
grep "\[AUTH_FAILURE\]" logs/dynamic_acl.log | grep "error_code" | sort | uniq -c
```

#### 分析响应时间
```bash
# 平均响应时间
grep "\[AUTH_SUCCESS\]" logs/dynamic_acl.log | grep "processing_time_ms" | awk -F'processing_time_ms=' '{sum+=$2; count++} END {print "平均响应时间:", sum/count, "ms"}'

# 响应时间分布
grep "\[AUTH_SUCCESS\]" logs/dynamic_acl.log | grep "processing_time_ms" | awk -F'processing_time_ms=' '{print $2}' | sort -n | awk '
BEGIN {bin_width=50}
{
    bin=int($1/bin_width);
    count[bin]++;
}
END {
    for (bin in count) {
        start = bin * bin_width;
        end = (bin + 1) * bin_width;
        print start "-" end "ms:", count[bin];
    }
}
'

**为什么不关联所有rules到一个policy？**

1. **更细粒度的控制**: 可以对每个权限规则独立管理
2. **更好的可审计性**: 每个权限都有独立的policy记录
3. **灵活的权限组合**: 未来可以支持更复杂的权限逻辑
4. **避免单点故障**: 一个rule的问题不会影响其他rules

### 消息类型映射

| 认证服务Action | Zenoh消息类型 | 说明 |
|----------------|---------------|------|
| pub | put | 发布消息 |
| sub | declare_subscriber | 订阅声明 |
| admin | put,delete,query | 管理权限 |

### 权限配置示例

#### 传感器客户端
```json
{
  "is_authorized": true,
  "rules": [
    {
      "id": "sensor-temp-pub",
      "key_exprs": ["sensor/temperature/*"],
      "messages": ["put"],
      "flows": ["ingress"],
      "permission": "allow"
    },
    {
      "id": "sensor-status-pub",
      "key_exprs": ["sensor/status/sensor-001"],
      "messages": ["put"],
      "flows": ["ingress"],
      "permission": "allow"
    },
    {
      "id": "sensor-control-sub",
      "key_exprs": ["control/sensor-001/*"],
      "messages": ["declare_subscriber"],
      "flows": ["ingress"],
      "permission": "allow"
    }
  ],
  "subject_config": {
    "interfaces": ["eth0"],
    "link_protocols": ["tls"],
    "usernames": null,
    "zids": null
  },
  "client_info": {
    "name": "温度传感器",
    "organization": "智能制造部"
  }
}
```

**Zenoh自动生成的完整ACL配置**：
```json
{
  "rules": [
    {"id": "sensor-temp-pub", "key_exprs": ["sensor/temperature/*"], ...},
    {"id": "sensor-status-pub", "key_exprs": ["sensor/status/sensor-001"], ...},
    {"id": "sensor-control-sub", "key_exprs": ["control/sensor-001/*"], ...}
  ],
  "subjects": [
    {
      "id": "dynamic-subject-sensor-001",
      "cert_common_names": ["sensor-client-001"],
      "interfaces": ["eth0"],
      "link_protocols": ["tls"]
    }
  ],
  "policies": [
    {
      "id": "dynamic-policy-sensor-001",
      "rules": ["sensor-temp-pub", "sensor-status-pub", "sensor-control-sub"],
      "subjects": ["dynamic-subject-sensor-001"]
    }
  ]
}
```

**关键特性**：
- ✅ **每个客户端一个Policy**: 统一管理客户端的所有权限规则
- ✅ **权限统一过期**: 通过缓存TTL控制整个客户端权限的生命周期
- ✅ **Subject增强**: 支持interfaces、link_protocols等高级配置

#### 控制器客户端
```json
{
  "is_authorized": true,
  "rules": [
    {
      "id": "controller-pub-all",
      "key_exprs": ["control/*"],
      "messages": ["put"],
      "flows": ["ingress"],
      "permission": "allow"
    },
    {
      "id": "controller-sub-sensor",
      "key_exprs": ["sensor/*"],
      "messages": ["declare_subscriber"],
      "flows": ["ingress"],
      "permission": "allow"
    },
    {
      "id": "controller-admin",
      "key_exprs": ["system/*"],
      "messages": ["put", "delete", "query"],
      "flows": ["ingress"],
      "permission": "allow"
    }
  ],
  "client_info": {
    "name": "中央控制器",
    "organization": "控制中心"
  }
}
```

#### 监控客户端
```json
{
  "is_authorized": true,
  "rules": [
    {
      "id": "monitor-sub-all",
      "key_exprs": ["sensor/*", "control/*", "system/*", "gateway/*"],
      "messages": ["declare_subscriber"],
      "flows": ["ingress"],
      "permission": "allow"
    },
    {
      "id": "monitor-alert-pub",
      "key_exprs": ["alert/*"],
      "messages": ["put"],
      "flows": ["ingress"],
      "permission": "allow"
    }
  ],
  "client_info": {
    "name": "监控系统",
    "organization": "运维部"
  }
}
```

## 错误码和状态码说明

### HTTP状态码

- `200`: 成功 - 客户端已授权
- `400`: 请求错误 - 参数无效或格式错误
- `401`: 未授权 - 客户端不存在或未激活
- `403`: 禁止访问 - 客户端类型不匹配或其他业务逻辑错误
- `500`: 服务器错误 - 服务内部错误

### 错误响应格式

```json
{
  "is_authorized": false,
  "error_message": "详细的错误描述信息",
  "error_code": "ERROR_CODE"
}
```

### 错误码详细说明

| 错误码 | HTTP状态码 | 说明 | 处理建议 |
|--------|-----------|------|---------|
| `CLIENT_NOT_FOUND` | 401 | 客户端ID不存在 | 检查客户端ID是否正确注册 |
| `CLIENT_INACTIVE` | 401 | 客户端已被禁用 | 联系管理员激活客户端 |
| `TYPE_MISMATCH` | 403 | 客户端类型与注册不符 | 检查client_type参数是否正确 |
| `INVALID_REQUEST` | 400 | 请求参数无效 | 检查client_id和client_type格式 |
| `RULE_FORMAT_ERROR` | 400 | 规则格式错误 | 检查rules数组的格式和内容 |
| `KEYEXPR_INVALID` | 400 | key表达式格式错误 | 检查key_exprs中的通配符语法 |
| `MESSAGE_TYPE_INVALID` | 400 | 消息类型无效 | 检查messages数组中的值 |
| `PERMISSION_INVALID` | 400 | 权限值无效 | 只能使用"allow"或"deny" |
| `INTERNAL_ERROR` | 500 | 服务内部错误 | 重试请求，如持续失败联系管理员 |

### 常见错误示例

#### 客户端不存在
```json
{
  "is_authorized": false,
  "error_message": "客户端 'sensor-999' 不存在",
  "error_code": "CLIENT_NOT_FOUND"
}
```

#### 规则格式错误
```json
{
  "is_authorized": false,
  "error_message": "消息类型 'invalid_type' 无效，有效值: put, delete, declare_subscriber...",
  "error_code": "MESSAGE_TYPE_INVALID"
}
```

#### Key表达式错误
```json
{
  "is_authorized": false,
  "error_message": "key表达式 'sensor/temp[' 格式无效",
  "error_code": "KEYEXPR_INVALID"
}
```

## 缓存机制

**重要**: ACL配置的缓存TTL由Zenoh服务器统一配置，不需要在认证服务响应中指定。

Zenoh服务器配置示例：
```json5
{
  access_control: {
    dynamic_acl: {
      cache: {
        ttl_seconds: 3600,     // 生产环境：1小时
        max_entries: 10000
      }
    }
  }
}
```

缓存策略：
- **生产环境**: 3600秒（1小时）- 平衡性能和时效性
- **开发环境**: 300秒（5分钟）- 快速更新便于调试
- **高安全要求**: 1800秒（30分钟）- 更频繁的权限更新

## 实现注意事项

### 认证服务实现

1. **安全性**: 使用HTTPS传输敏感信息，验证客户端身份
2. **性能**: 实现适当的缓存机制，避免频繁认证服务调用
3. **日志存档**: 实现基础日志记录和自动轮转，便于问题排查
4. **错误处理**: 提供详细的错误信息和错误码，便于调试
5. **Rules设计**: 专注于定义权限规则，确保key_exprs和messages的正确性
6. **Subject配置**: 可选提供subject_config以实现更精确的访问控制
7. **验证逻辑**: 在返回规则前验证所有字段的格式正确性

#### Rules验证要求

**必需字段验证**：
- **id**: 非空字符串，建议使用描述性名称，避免特殊字符
- **key_exprs**: 至少一个字符串数组，每个字符串必须是有效的Zenoh key表达式
- **messages**: 至少一个字符串数组，只能包含预定义的消息类型
- **permission**: 必须是"allow"或"deny"（大小写敏感）

**可选字段验证**：
- **flows**: 可选字符串数组，只能包含"ingress"和"egress"

**Zenoh端验证流程**：
1. JSON结构完整性检查
2. 字段类型和格式验证
3. Key表达式语法验证
4. 枚举值有效性检查
5. 业务逻辑一致性检查

### Zenoh端实现

1. **智能去重**: 基于完整规则内容进行去重，考虑所有字段
2. **ID管理**: 仅在规则内容不同时确保ID唯一性，通过重命名解决冲突
3. **Subject生成**: 使用认证服务提供的subject_config或自动生成，支持所有字段扩展
4. **Policy生成**: 为每个客户端生成一个policy，统一管理权限过期
5. **缓存管理**: 统一的TTL配置，避免每个客户端单独管理
6. **日志记录**: 记录规则复用、ID重命名和验证错误
7. **格式验证**: 对认证服务返回的规则和subject_config进行完整性验证
8. **高级控制**: 支持网络接口、传输协议、用户名、实例ID等多维度访问控制
