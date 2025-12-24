# 动态 ACL 鉴权方案实现文档

## 1. 概述

本文档详细描述了 Zenoh Router 中动态 ACL（Access Control List）鉴权方案的实现，包括设备接入、设备断联和权限策略管理的完整流程。

### 1.1 核心特性

- **动态鉴权**：设备连接时从外部认证服务获取 ACL 规则
- **自动策略管理**：策略随连接生命周期自动创建和清理
- **去重机制**：防止重复处理断联事件
- **RAII 资源管理**：使用 Rust 的所有权机制确保资源正确释放

### 1.2 关键组件

- **AuthClient**：与外部认证服务通信的 HTTP 客户端
- **DynamicAclIntegrator**：处理动态 ACL 规则，生成 Subject 和 Policy
- **PolicyEnforcer**：执行 ACL 权限检查
- **RuntimeSession**：管理设备连接和断联事件

## 2. 架构设计

### 2.1 整体架构

```
┌─────────────────┐
│  Zenoh Client   │
│  (TLS/mTLS)     │
└────────┬────────┘
         │
         │ 连接请求
         ▼
┌─────────────────────────────────────┐
│      Zenoh Router (Gateway)         │
│                                     │
│  ┌───────────────────────────────┐  │
│  │   RuntimeSession             │  │
│  │   - new_link()               │  │
│  │   - del_link()               │  │
│  └───────────┬───────────────────┘  │
│              │                      │
│              ▼                      │
│  ┌───────────────────────────────┐  │
│  │   AclEnforcer                 │  │
│  │   - new_transport_unicast()   │  │
│  └───────────┬───────────────────┘  │
│              │                      │
│              ▼                      │
│  ┌───────────────────────────────┐  │
│  │   AuthClient                  │  │
│  │   - authenticate()            │  │
│  └───────────┬───────────────────┘  │
│              │                      │
│              ▼                      │
│  ┌───────────────────────────────┐  │
│  │   DynamicAclIntegrator        │  │
│  │   - integrate_acl_config()    │  │
│  └───────────┬───────────────────┘  │
│              │                      │
│              ▼                      │
│  ┌───────────────────────────────┐  │
│  │   PolicyEnforcer              │  │
│  │   - 权限检查                  │  │
│  └───────────────────────────────┘  │
└─────────────┬───────────────────────┘
              │
              │ HTTP 请求
              ▼
┌─────────────────────────────┐
│   认证服务 (Auth Service)    │
│   - /auth                    │
│   - /device/connect          │
│   - /device/disconnect       │
└─────────────────────────────┘
```

### 2.1.1 原始网关组件结构（静态 ACL）

**组件层次结构：**

```
外部系统
└── Zenoh Client (TLS/TCP 连接)

Zenoh Router
├── 传输层
│   └── Transport Layer (连接管理)
├── 运行时层
│   ├── RuntimeSession (连接生命周期管理)
│   └── TransportPeerEventHandler (事件处理)
├── ACL 层
│   └── AclEnforcer (拦截器工厂，使用静态配置)
├── 策略执行层
│   ├── PolicyEnforcer (权限检查引擎，预配置规则)
│   ├── IngressAclEnforcer (入站拦截器)
│   └── EgressAclEnforcer (出站拦截器)
└── 资源管理层
    ├── InterceptorsChain (拦截器链)
    └── DeMux/Face (消息路由)
```

**处理流程步骤：**

1. **连接建立：**
   - Client → Transport Layer → RuntimeSession
   - RuntimeSession → TransportPeerEventHandler
   - Handler → AclEnforcer 创建拦截器
   - AclEnforcer 查询 PolicyEnforcer 获取匹配的 Subjects
   - 创建 IngressAclEnforcer 和 EgressAclEnforcer
   - 添加到 InterceptorsChain 并注册到 DeMux/Face

2. **消息处理：**
   - Client 发送消息 → Transport Layer
   - Transport → DeMux/Face 路由消息
   - DeMux → IngressAclEnforcer 入站检查
   - IngressAclEnforcer → PolicyEnforcer 权限检查
   - 根据检查结果允许/拒绝消息转发

3. **断联清理：**
   - Client 断开连接 → Transport Layer
   - Transport → RuntimeSession::del_link
   - 触发 InterceptorsChain 销毁
   - IngressAclEnforcer 和 EgressAclEnforcer 被销毁
   - PolicyEnforcer 引用计数减少（RAII 自动清理）

### 2.1.2 加入动态 ACL 的组件交互图

#### 图表说明

文档中的流程图使用 **Mermaid** 语法编写，这是一种基于文本的图表描述语言。

**如何查看图表：**

1. **GitHub/GitLab**：自动渲染 Mermaid 图表
2. **VS Code**：安装 "Mermaid Preview" 扩展
3. **在线工具**：复制代码到 [mermaid.live](https://mermaid.live) 查看
4. **浏览器插件**：安装 Mermaid 渲染插件

**语法解释：**
- `graph TB`：从上到下 (Top-Bottom) 的流程图
- `subgraph "名称"`：分组容器
- `A[标签]`：矩形节点
- `A -->|描述| B`：带描述的连接线
- `style A fill:#color`：设置节点样式

**如果看不到图表，可以查看纯文本描述或时序图部分。**

#### 纯文本流程图（作为备选）

```
外部系统:
├── Zenoh Client (TLS/mTLS)
└── 认证服务 (Auth Service)

Zenoh Router:
├── 传输层:
│   └── Transport Layer (TLS 连接管理)
├── 运行时层:
│   ├── RuntimeSession (连接生命周期管理 + 动态事件处理)
│   └── TransportPeerEventHandler (事件处理)
├── ACL 层:
│   ├── AclEnforcer (拦截器工厂 + 动态鉴权) 🔴[新增]
│   ├── AuthClient (HTTP 客户端) 🔴[新增]
│   └── DynamicAclIntegrator (规则处理) 🔴[新增]
├── 策略执行层:
│   ├── PolicyEnforcer (权限检查引擎，动态创建)
│   ├── IngressAclEnforcer (入站拦截器)
│   └── EgressAclEnforcer (出站拦截器)
└── 资源管理:
    ├── InterceptorsChain (拦截器链)
    └── DeMux/Face (消息路由)

流程:
1. 连接建立: Client → Transport → Runtime → ACL → AuthClient → 认证服务
2. 规则处理: 认证服务 → AuthClient → ACL → Integrator → Policy
3. 消息处理: Client → Transport → DeMux → Ingress → Policy → 转发/拒绝
4. 断联处理: Client断开 → Transport → Runtime → 去重检查 → 资源清理

🔴[新增]: 相对于原始静态 ACL 流程新增的组件

**完整组件结构（加入动态 ACL 后）：**

```
外部系统
├── Zenoh Client (TLS/mTLS 连接)
└── 认证服务 (Auth Service) 🔴[新增]

Zenoh Router
├── 传输层
│   └── Transport Layer (TLS 连接管理)
├── 运行时层
│   ├── RuntimeSession (连接生命周期管理 + 动态事件处理) 🔴[增强]
│   └── TransportPeerEventHandler (事件处理)
├── ACL 层 🔴[增强]
│   ├── AclEnforcer (拦截器工厂 + 动态鉴权) 🔴[增强]
│   ├── AuthClient (HTTP 客户端) 🔴[新增]
│   └── DynamicAclIntegrator (规则处理) 🔴[新增]
├── 策略执行层
│   ├── PolicyEnforcer (权限检查引擎，动态创建) 🔴[变化]
│   ├── IngressAclEnforcer (入站拦截器)
│   └── EgressAclEnforcer (出站拦截器)
└── 资源管理层
    ├── InterceptorsChain (拦截器链)
    └── DeMux/Face (消息路由)
```

**增强后的处理流程：**

1. **连接建立（新增鉴权）：**
   - Client TLS 连接 → Transport Layer
   - Transport → RuntimeSession::new_link
   - RuntimeSession → Handler → AclEnforcer
   - AclEnforcer 提取证书信息 🔴[新增步骤]
   - 创建 AuthClient → 调用外部认证服务 🔴[新增步骤]
   - 认证服务返回动态 ACL 规则 🔴[新增步骤]
   - DynamicAclIntegrator 处理规则并生成配置 🔴[新增步骤]
   - 创建 IngressAclEnforcer 和 EgressAclEnforcer
   - 注册到 InterceptorsChain 和 DeMux/Face

2. **消息处理（保持不变）：**
   - Client → Transport → DeMux → IngressAclEnforcer
   - IngressAclEnforcer → PolicyEnforcer 权限检查
   - 根据检查结果转发或拒绝消息

3. **断联处理（新增去重）：**
   - Client 断开 → Transport → RuntimeSession::del_link
   - RuntimeSession 执行去重检查 🔴[新增步骤]
   - 触发资源清理：InterceptorsChain → 拦截器销毁 → PolicyEnforcer Arc 引用计数归零 → 自动清理

### 2.2 时序图

#### 2.2.1 原始网关处理流程（静态 ACL）

**完整时序流程：**

1. **连接建立阶段：**
   - Client → Transport Layer: 建立 TLS/TCP 连接
   - Transport Layer → RuntimeSession: 调用 new_link(link)
   - RuntimeSession → AclEnforcer: 调用 new_static_transport_unicast(transport)
   - AclEnforcer: 提取认证信息（Auth IDs, Links）
   - AclEnforcer → PolicyEnforcer: 查询 Subject Store（SubjectQuery）
   - PolicyEnforcer → AclEnforcer: 返回匹配的 Subjects
   - AclEnforcer: 创建 IngressAclEnforcer 和 EgressAclEnforcer
   - AclEnforcer → Transport Layer: 返回拦截器配置

2. **消息处理阶段：**
   - Client → Transport Layer: 发送消息
   - Transport Layer → Interceptor: 入站权限检查
   - Interceptor → PolicyEnforcer: 执行权限检查
   - PolicyEnforcer → Interceptor: 返回允许/拒绝结果
   - Interceptor → Transport Layer: 根据结果转发消息
   - Transport Layer → Client: 发送响应消息

3. **断联清理阶段：**
   - 连接断开时触发 RAII 机制自动清理资源

#### 2.2.2 加入动态 ACL 的设备接入时序图

**动态 ACL 连接建立完整流程：**

1. **TLS 握手阶段：**
   - Client → Transport Layer: TLS 握手连接建立

2. **连接初始化：**
   - Transport Layer → RuntimeSession: 调用 new_link(link)
   - RuntimeSession: 执行 handle_device_connect(link) 记录连接信息

3. **动态鉴权开始：**
   - RuntimeSession → AclEnforcer: 调用 new_transport_unicast(transport)
   - AclEnforcer: 提取证书信息 (CertificateInfo::from_link)
   - AclEnforcer → AuthClient: 创建 AuthClient 实例
   - AclEnforcer → AuthClient: 调用 authenticate(client_id, cert_info)

4. **外部服务鉴权：**
   - AuthClient → 认证服务: 发送 POST /auth 请求
   - 认证服务 → AuthClient: 返回 AuthResponse {is_authorized, rules, subject}

5. **鉴权结果处理：**
   - **如果鉴权失败：** 回退到静态 ACL 处理
   - **如果鉴权成功：** 继续动态规则处理

6. **动态规则集成：**
   - AclEnforcer → DynamicAclIntegrator: 调用 integrate_acl_config()
   - DynamicAclIntegrator: 执行规则去重 (deduplicate_and_assign_rule_ids)
   - DynamicAclIntegrator: 生成 Subject (generate_subject_for_client)
   - DynamicAclIntegrator: 生成 Policy (generate_policy_for_client)
   - DynamicAclIntegrator → AclEnforcer: 返回处理后的配置

7. **策略执行器创建：**
   - AclEnforcer → PolicyEnforcer: 创建新实例并初始化
   - PolicyEnforcer: 初始化 SubjectStore 和 PolicyMap
   - AclEnforcer: 创建 IngressAclEnforcer 和 EgressAclEnforcer

8. **拦截器注册：**
   - AclEnforcer → Transport Layer: 返回完整的拦截器链配置

9. **消息处理就绪：**
   - 连接建立完成，可以开始正常的权限检查消息处理

#### 2.2.2 设备断联时序图

**设备断联完整流程（包含去重机制）：**

**断联触发场景（4种）：**

1. **RX 任务失败：** Transport Layer 接收任务异常 → 调用 del_link()
2. **TX 任务失败：** Transport Layer 发送任务异常 → 调用 del_link()
3. **收到 Close 消息：** Client 发送断开消息 → Transport Layer 处理 → 调用 del_link()
4. **TLS 连接错误：** TLS 连接出现错误 → 连接关闭 → 调用 del_link()

**去重检查流程：**

1. **初始化检查：**
   - RuntimeSession 执行 handle_device_disconnect(link)
   - 检查去重机制：查询 HashSet 是否已处理过该 endpoint
   - 检查时间窗口：确保在 10 秒内未重复处理

2. **去重结果：**
   - **已处理过：** 跳过后续处理，直接返回
   - **未处理过：** 继续执行断联逻辑

**断联处理流程：**

3. **证书信息提取：**
   - 调用 CertificateInfo::from_link() 提取客户端信息
   - 获取 client_id 和 client_type

4. **日志记录：**
   - 记录设备断联事件到日志系统

5. **资源自动清理（RAII）：**
   - 连接资源释放触发清理链
   - InterceptorsChain 销毁
   - IngressAclEnforcer 和 EgressAclEnforcer 被销毁
   - PolicyEnforcer Arc 引用计数减 1
   - 当引用计数归零时，PolicyEnforcer 自动清理释放内存

#### 2.2.3 流程对比分析

| 对比维度 | 原始网关流程（静态 ACL） | 动态 ACL 流程 |
|---------|-------------------------|--------------|
| **配置方式** | 预配置 ACL 规则<br/>静态 Subject/Policy | 动态获取 ACL 规则<br/>按连接生成 Subject/Policy |
| **鉴权时机** | 连接建立时<br/>本地规则匹配 | 连接建立时<br/>外部服务鉴权 |
| **证书处理** | 可选 TLS 证书<br/>用于 Subject 匹配 | 必需 TLS 证书<br/>用于客户端身份验证 |
| **规则来源** | 配置文件 | 外部认证服务 API |
| **策略管理** | 全局共享<br/>预加载 | 连接独占<br/>动态创建/销毁 |
| **断联处理** | 仅资源清理 | 去重机制 + 资源清理<br/>（可选断联通知） |
| **性能影响** | 低延迟<br/>本地查询 | 网络延迟<br/>外部服务依赖 |
| **扩展性** | 静态配置<br/>变更需重启 | 动态配置<br/>运行时更新 |

**关键新增组件**：
- 🔴 `AuthClient`：HTTP 客户端，负责与认证服务通信
- 🔴 `DynamicAclIntegrator`：规则处理器，处理动态 ACL 配置
- 🔴 `RuntimeSession::handle_device_connect/disconnect`：连接生命周期事件处理

**核心改进**：
1. **动态鉴权**：从静态配置转向外部服务鉴权
2. **连接隔离**：每个连接的策略相互独立
3. **自动清理**：策略随连接生命周期自动管理
4. **去重机制**：防止重复处理断联事件

#### 2.2.4 数据流

1. **设备接入流程**：
   ```
   Client 连接 → RuntimeSession::new_link()
   → AclEnforcer::new_transport_unicast()
   → AuthClient::authenticate()
   → DynamicAclIntegrator::integrate_acl_config()
   → PolicyEnforcer::init()
   → 创建 IngressAclEnforcer/EgressAclEnforcer
   ```

2. **设备断联流程**：
   ```
   Link 断开 → RuntimeSession::del_link()
   → RuntimeSession::handle_device_disconnect()
   → 去重检查
   → 提取证书信息
   → 策略自动清理 (RAII)
   ```

## 3. 设备接入流程

### 3.1 连接建立

当设备通过 TLS/mTLS 连接到 Zenoh Router 时，会触发以下流程：

#### 3.1.1 RuntimeSession::new_link()

**位置**：`zenoh/src/net/runtime/mod.rs:737-748`

```rust
fn new_link(&self, link: Link) {
    self.main_handler.new_link(link.clone());
    for handler in &self.slave_handlers {
        handler.new_link(link.clone());
    }

    // Handle device connect notifications for dynamic ACL
    #[cfg(feature = "dynamic_acl")]
    {
        self.handle_device_connect(&link);
    }
}
```

**功能**：
- 通知所有 handler 新连接建立
- 调用 `handle_device_connect()` 记录连接信息（当前仅记录日志，不发送通知）

#### 3.1.2 AclEnforcer::new_transport_unicast()

**位置**：`zenoh/src/net/routing/interceptor/access_control.rs:371-550`

**关键步骤**：

1. **提取证书信息**：
   ```rust
   let cert_info = match links.into_iter()
       .find_map(|link| CertificateInfo::from_link(&link)) {
       Some(info) => info,
       None => {
           tracing::warn!("Could not extract certificate information");
           return self.new_static_transport_unicast(transport);
       }
   };
   ```

2. **创建 AuthClient 并鉴权**：
   ```rust
   let auth_client = match AuthClient::new(dynamic_config.clone()) {
       Ok(client) => client,
       Err(e) => {
           tracing::error!("Failed to create auth client: {}", e);
           return self.new_static_transport_unicast(transport);
       }
   };

   let dynamic_acl_result = match tokio::runtime::Handle::try_current() {
       Ok(handle) => {
           tokio::task::block_in_place(|| {
               handle.block_on(async {
                   auth_client.authenticate(
                       &cert_info.common_name,
                       Some(&cert_info.common_name),
                       cert_info.organizational_unit.as_deref(),
                       cert_info.interface.as_deref(),
                   ).await
               })
           })
       },
       Err(_) => {
           let rt = tokio::runtime::Runtime::new().unwrap();
           rt.block_on(async {
               auth_client.authenticate(...).await
           })
       }
   };
   ```

3. **处理鉴权响应**：
   - 检查 `is_authorized` 标志
   - 提取 ACL 规则列表
   - 使用 `DynamicAclIntegrator` 处理规则

#### 3.1.3 DynamicAclIntegrator::integrate_acl_config()

**位置**：`zenoh/src/net/dynamic_acl/integrator.rs:54-83`

**功能**：
- **规则去重**：相同内容的规则复用 ID，不同规则分配唯一 ID
- **生成 Subject**：根据证书信息生成 `AclConfigSubjects`
- **生成 Policy**：为客户端创建统一的 Policy 管理所有规则

**关键代码**：
```rust
pub fn integrate_acl_config(
    &mut self,
    client_id: &str,
    response: &DynamicAclResponse,
) -> ZResult<(Vec<AclConfigRule>, Vec<AclConfigSubjects>, Vec<AclConfigPolicyEntry>)> {
    if !response.is_authorized {
        bail!("Client {} is not authorized", client_id);
    }

    match &response.rules {
        Some(rules) => {
            // 1. 智能去重：相同规则内容复用，不同规则分配唯一ID
            let processed_rules = self.deduplicate_and_assign_rule_ids(client_id, rules)?;

            // 2. 自动生成对应的subject
            let subject = self.generate_subject_for_client(client_id, &response.subject_config)?;

            // 3. 为客户端生成一个policy - 统一管理所有权限规则
            let policy = self.generate_policy_for_client(client_id, &processed_rules, &subject)?;

            Ok((processed_rules, vec![subject], vec![policy]))
        }
        None => {
            Ok((vec![], vec![], vec![]))
        }
    }
}
```

#### 3.1.4 创建 PolicyEnforcer

**位置**：`zenoh/src/net/routing/interceptor/access_control.rs:465-540`

**关键步骤**：

1. **创建独立的 PolicyEnforcer 实例**：
   ```rust
   let mut ingress_policy_enforcer = PolicyEnforcer::new();
   let mut egress_policy_enforcer = PolicyEnforcer::new();
   ```

2. **初始化 PolicyEnforcer**：
   ```rust
   let acl_config = AclConfig {
       enabled: true,
       default_permission: self.enforcer.default_permission,
       rules: Some(rules.clone()),
       subjects: Some(vec![dynamic_subject.clone()]),
       policies: Some(vec![dynamic_policy.clone()]),
       query_strategy: None,
       dynamic_config: None,
   };

   ingress_policy_enforcer.init(&acl_config)?;
   egress_policy_enforcer.init(&acl_config)?;
   ```

3. **创建 Interceptor**：
   ```rust
   let ingress_interceptor = Box::new(IngressAclEnforcer {
       policy_enforcer: Arc::new(ingress_policy_enforcer),
       zid,
       subject: auth_subjects.clone(),
   });

   let egress_interceptor = Box::new(EgressAclEnforcer {
       policy_enforcer: Arc::new(egress_policy_enforcer),
       zid,
       subject: auth_subjects,
   });
   ```

### 3.2 策略存储机制

**关键点**：
- 每个连接都有独立的 `PolicyEnforcer` 实例
- `PolicyEnforcer` 存储在 `Arc<PolicyEnforcer>` 中，由 `IngressAclEnforcer` 和 `EgressAclEnforcer` 持有
- Interceptor 存储在 `InterceptorsChain` 中，由 `DeMux`/`Face` 管理
- 当连接断开时，`DeMux`/`Face` 被销毁，`Arc` 引用计数归零，`PolicyEnforcer` 自动清理

## 4. 设备断联流程

### 4.1 断联触发点

设备断联可能由以下场景触发：

1. **RX 任务失败**：`io/zenoh-transport/src/unicast/universal/rx.rs`
2. **TX 任务失败**：`io/zenoh-transport/src/unicast/universal/link.rs`
3. **收到 Close 消息**：`io/zenoh-transport/src/unicast/universal/rx.rs`
4. **TLS 连接错误**：`io/zenoh-links/zenoh-link-tls/src/unicast.rs`

所有这些场景最终都会调用 `transport.del_link()`，进而触发 `RuntimeSession::del_link()`。

### 4.2 RuntimeSession::del_link()

**位置**：`zenoh/src/net/runtime/mod.rs:750-763`

```rust
fn del_link(&self, link: Link) {
    self.main_handler.del_link(link.clone());
    for handler in &self.slave_handlers {
        handler.del_link(link.clone());
    }

    // Handle device disconnect notifications for dynamic ACL
    #[cfg(feature = "dynamic_acl")]
    {
        self.handle_device_disconnect(&link);
    }

    Runtime::closed_link(self, link.dst.to_endpoint());
}
```

### 4.3 handle_device_disconnect()

**位置**：`zenoh/src/net/runtime/mod.rs:780-863`

#### 4.3.1 去重机制

为了防止多个断联触发点导致重复处理，实现了基于时间窗口的去重机制：

```rust
static RECENT_DISCONNECTS: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));
static LAST_CLEANUP: Lazy<Mutex<Option<Instant>>> = Lazy::new(|| Mutex::new(None));

let should_process = {
    let mut recents = RECENT_DISCONNECTS.lock().unwrap();
    let mut last_cleanup = LAST_CLEANUP.lock().unwrap();

    // 每 10 秒清理一次旧记录
    let now = Instant::now();
    if let Some(cleanup_time) = *last_cleanup {
        if now.duration_since(cleanup_time) > Duration::from_secs(10) {
            recents.clear();
            *last_cleanup = Some(now);
        }
    } else {
        *last_cleanup = Some(now);
    }

    // 检查是否最近处理过该 endpoint
    if recents.contains(&endpoint) {
        tracing::debug!("Skipping duplicate disconnect processing for endpoint: {}", endpoint);
        false
    } else {
        recents.insert(endpoint.clone());
        true
    }
};
```

**去重逻辑**：
- 使用 `HashSet<String>` 记录最近处理的 endpoint
- 每 10 秒清理一次旧记录
- 如果 endpoint 在 10 秒内已处理过，则跳过

#### 4.3.2 证书信息提取

```rust
if let Some(cert_info) = crate::net::dynamic_acl::provider::CertificateInfo::from_link(link) {
    let client_id = cert_info.common_name.clone();
    let client_type = cert_info.organizational_unit.clone().unwrap_or_else(|| "unknown".to_string());

    tracing::info!(
        "[RuntimeSession] Device disconnected: client_id={}, client_type={}, endpoint={}",
        client_id,
        client_type,
        endpoint
    );
}
```

#### 4.3.3 策略自动清理

**关键点**：
- 策略清理是**自动的**，通过 Rust 的 RAII（Resource Acquisition Is Initialization）机制实现
- `PolicyEnforcer` 存储在 `Arc<PolicyEnforcer>` 中，由 `IngressAclEnforcer` 持有
- `IngressAclEnforcer` 存储在 `InterceptorsChain` 中，由 `DeMux`/`Face` 管理
- 当连接断开时：
  1. `DeMux`/`Face` 被销毁
  2. `InterceptorsChain` 被销毁
  3. `IngressAclEnforcer` 被销毁
  4. `Arc<PolicyEnforcer>` 引用计数归零
  5. `PolicyEnforcer` 自动清理

**代码注释**：
```rust
// Policy cleanup is automatic:
// - PolicyEnforcer is stored in Arc<PolicyEnforcer> within IngressAclEnforcer
// - IngressAclEnforcer is stored in InterceptorsChain
// - When DeMux/Face is destroyed, interceptor chain is destroyed
// - Arc reference count decreases, PolicyEnforcer is automatically cleaned up
// - Next connection will re-authenticate and create a new PolicyEnforcer
tracing::info!(
    "[RuntimeSession] Dynamic ACL policy for client '{}' will be automatically cleaned up when connection resources are released. Next connection will re-authenticate.",
    client_id
);
```

### 4.4 断联通知（可选）

**当前实现**：
- 代码中已移除 `notify_device_disconnect()` 的调用
- 仅保留日志记录，用于调试和审计

**如需启用断联通知**，可以在 `handle_device_disconnect()` 中添加：

```rust
// 创建 AuthClient 并发送断联通知
let auth_client = match AuthClient::new(dynamic_config.clone()) {
    Ok(client) => client,
    Err(e) => {
        tracing::error!("Failed to create auth client: {}", e);
        return;
    }
};

let endpoint_str = endpoint.clone();
let client_id_clone = client_id.clone();
let client_type_clone = client_type.clone();
let reason = format!("Connection to {} closed", endpoint);

tokio::spawn(async move {
    match auth_client.notify_device_disconnect(&client_id_clone, &client_type_clone, Some(&reason)).await {
        Ok(_) => {
            tracing::info!("Sent device disconnect notification for {}", client_id_clone);
        }
        Err(e) => {
            tracing::error!("Failed to send device disconnect notification: {}", e);
        }
    }
});
```

## 5. 动态 ACL 鉴权机制

### 5.1 AuthClient

**位置**：`zenoh/src/net/dynamic_acl/client.rs`

#### 5.1.1 鉴权请求

**API**：`POST /auth`

**请求结构**：
```rust
pub struct AuthRequest {
    pub client_id: String,
    pub client_type: String,
}
```

**响应结构**：
```rust
pub struct AuthResponse {
    pub is_authorized: bool,
    pub rules: Option<Vec<DynamicAclRule>>,
    pub subject: Option<SubjectConfig>,
    pub client_info: Option<ClientInfo>,
    pub error_message: Option<String>,
}
```

#### 5.1.2 重试机制

**配置参数**：
- `retry_attempts`：最大重试次数
- `retry_delay_seconds`：重试延迟（秒）
- `timeout_seconds`：请求超时时间（秒）

**实现**：
```rust
async fn request_with_retry<T>(
    &self,
    method: &str,
    endpoint: &str,
    request_body: &T,
) -> ZResult<String>
where
    T: Serialize + std::fmt::Debug,
{
    let mut attempts = 0;
    let max_attempts = self.config.retry_attempts;

    loop {
        attempts += 1;
        match self.make_request_once(method, endpoint, request_body).await {
            Ok(response) => {
                tracing::debug!("Request succeeded: {} {} (attempt {})", method, endpoint, attempts);
                return Ok(response);
            }
            Err(e) => {
                if attempts >= max_attempts {
                    tracing::error!("Request failed after {} attempts: {} {} - {}", max_attempts, method, endpoint, e);
                    return Err(e);
                }
                tokio::time::sleep(Duration::from_millis(self.config.retry_delay_seconds * 1000)).await;
            }
        }
    }
}
```

### 5.2 DynamicAclIntegrator

**位置**：`zenoh/src/net/dynamic_acl/integrator.rs`

#### 5.2.1 规则去重

**目的**：避免相同内容的规则重复存储，节省内存。

**实现**：
```rust
fn deduplicate_and_assign_rule_ids(
    &mut self,
    client_id: &str,
    rules: &[AclConfigRule],
) -> ZResult<Vec<AclConfigRule>> {
    let mut result = Vec::new();
    let mut client_rule_ids = std::collections::HashSet::new();

    for rule in rules {
        let rule_content = RuleContent::from_rule(rule);

        // 检查全局规则注册表：是否存在相同内容的规则？
        let assigned_id = if let Some(existing_id) = self.global_rule_registry.get(&rule_content) {
            // ✅ 规则内容相同，复用现有 ID
            existing_id.clone()
        } else {
            // ❌ 规则内容不同，需要新 ID
            let mut candidate_id = rule.id.clone();
            if !client_rule_ids.insert(candidate_id.clone()) {
                candidate_id = format!("{}-{}", client_id, rule.id);
            }
            // 注册到全局规则表
            self.global_rule_registry.insert(rule_content, candidate_id.clone());
            candidate_id
        };

        result.push(AclConfigRule {
            id: assigned_id,
            key_exprs: rule.key_exprs.clone(),
            messages: rule.messages.clone(),
            flows: rule.flows.clone(),
            permission: rule.permission,
        });
    }

    Ok(result)
}
```

#### 5.2.2 Subject 生成

**默认值**：
- `id`：`"subject-{client_id}"`
- `cert_common_names`：`[client_id]`
- `link_protocols`：`[TLS]`

**可覆盖字段**（如果认证服务返回 `subject_config`）：
- `interfaces`
- `usernames`
- `link_protocols`
- `zids`

#### 5.2.3 Policy 生成

**结构**：
```rust
AclConfigPolicyEntry {
    id: Some(format!("policy-{}", client_id)),
    rules: vec![rule_id1, rule_id2, ...],  // 所有规则的 ID
    subjects: vec![subject_id],             // 关联的 Subject ID
}
```

### 5.3 PolicyEnforcer

**位置**：`zenoh/src/net/routing/interceptor/authorization.rs`

#### 5.3.1 初始化

```rust
pub fn init(&mut self, acl_config: &AclConfig) -> ZResult<()> {
    self.acl_enabled = acl_config.enabled;
    self.default_permission = acl_config.default_permission;
    if self.acl_enabled {
        // 根据 query_strategy 决定使用哪种 ACL 模式
        let query_strategy = acl_config.query_strategy.as_ref()
            .cloned()
            .unwrap_or(QueryStrategy::Static);

        match query_strategy {
            QueryStrategy::Static => {
                self.init_static_acl(acl_config)?;
            }
            QueryStrategy::Dynamic => {
                self.init_dynamic_acl(acl_config)?;
            }
        }
    }
    Ok(())
}
```

#### 5.3.2 权限检查

`PolicyEnforcer` 在 `IngressAclEnforcer` 和 `EgressAclEnforcer` 中使用，对每个消息进行权限检查：

```rust
impl InterceptorTrait for IngressAclEnforcer {
    fn intercept(&mut self, message: &mut ZenohMessage) -> InterceptorResult {
        // 使用 policy_enforcer 检查权限
        let permission = self.policy_enforcer.check_permission(
            &self.subject,
            message,
            InterceptorFlow::Ingress,
        );
        // ...
    }
}
```

## 6. 配置说明

### 6.1 动态 ACL 配置

**配置文件示例**（JSON5 格式）：

```json5
{
  access_control: {
    enabled: true,
    default_permission: "deny",
    query_strategy: "dynamic",
    dynamic_config: {
      endpoint: "http://localhost:8080",
      timeout_seconds: 5,
      retry_attempts: 3,
      retry_delay_seconds: 1,
    },
  },
}
```

### 6.2 TLS 证书配置

**Router 端配置**：
```json5
{
  transport: {
    unicast: {
      tls: {
        root_ca_certificate: "path/to/ca.crt",      // CA 证书（验证客户端）
        listen_private_key: "path/to/server.key",   // 服务器私钥
        listen_certificate: "path/to/server.crt",   // 服务器证书
        connect_private_key: "path/to/client.key",  // 连接其他 Router 时的私钥（可选）
        connect_certificate: "path/to/client.crt",  // 连接其他 Router 时的证书（可选）
        enable_mtls: true,                          // 启用双向 TLS
      },
    },
  },
}
```

**说明**：
- `root_ca_certificate`：用于验证客户端证书的 CA 证书
- `listen_*`：Router 作为服务器时的证书
- `connect_*`：Router 连接其他 Router 时的证书（仅 Router 模式需要）
- `enable_mtls`：启用双向 TLS，要求客户端提供证书

## 7. 关键代码位置总结

### 7.1 设备接入

| 组件 | 文件 | 关键方法 |
|------|------|----------|
| RuntimeSession | `zenoh/src/net/runtime/mod.rs` | `new_link()`, `handle_device_connect()` |
| AclEnforcer | `zenoh/src/net/routing/interceptor/access_control.rs` | `new_transport_unicast()` |
| AuthClient | `zenoh/src/net/dynamic_acl/client.rs` | `authenticate()` |
| DynamicAclIntegrator | `zenoh/src/net/dynamic_acl/integrator.rs` | `integrate_acl_config()` |
| PolicyEnforcer | `zenoh/src/net/routing/interceptor/authorization.rs` | `init()` |

### 7.2 设备断联

| 组件 | 文件 | 关键方法 |
|------|------|----------|
| RuntimeSession | `zenoh/src/net/runtime/mod.rs` | `del_link()`, `handle_device_disconnect()` |
| Transport Layer | `io/zenoh-transport/src/unicast/universal/rx.rs` | RX 任务失败处理 |
| Transport Layer | `io/zenoh-transport/src/unicast/universal/link.rs` | TX 任务失败处理 |

### 7.3 策略管理

| 组件 | 文件 | 说明 |
|------|------|------|
| PolicyEnforcer | `zenoh/src/net/routing/interceptor/authorization.rs` | 权限检查引擎 |
| IngressAclEnforcer | `zenoh/src/net/routing/interceptor/access_control.rs` | 入站拦截器 |
| EgressAclEnforcer | `zenoh/src/net/routing/interceptor/access_control.rs` | 出站拦截器 |

## 8. 设计决策说明

### 8.1 为什么不在 AclEnforcer 中处理断联通知？

**原因**：
- `AclEnforcer` 是 ACL 权限检查组件，断联通知属于设备生命周期管理，应该由更高层的 `RuntimeSession` 处理
- 保持关注点分离：ACL 组件专注于权限，Runtime 组件专注于连接管理

### 8.2 为什么使用去重机制？

**原因**：
- 断联可能由多个触发点导致（RX 失败、TX 失败、Close 消息等）
- 每个触发点都可能调用 `del_link()`，导致重复处理
- 使用时间窗口去重可以确保每个 endpoint 只处理一次

### 8.3 为什么策略清理是自动的？

**原因**：
- 使用 Rust 的 RAII 机制，资源自动管理
- `PolicyEnforcer` 存储在 `Arc` 中，当连接断开时，引用计数归零，自动清理
- 无需手动清理，减少内存泄漏风险

### 8.4 为什么每个连接创建独立的 PolicyEnforcer？

**原因**：
- 不同设备可能有不同的 ACL 规则
- 独立的 `PolicyEnforcer` 实例可以隔离不同设备的权限
- 当设备断开时，其 `PolicyEnforcer` 自动清理，不影响其他设备

## 9. 测试建议

### 9.1 设备接入测试

1. **正常接入**：
   - 使用有效证书连接
   - 验证鉴权成功
   - 验证策略正确创建
   - 验证权限检查正常工作

2. **鉴权失败**：
   - 使用无效证书连接
   - 验证连接被拒绝
   - 验证策略未创建

3. **认证服务不可用**：
   - 停止认证服务
   - 验证重试机制
   - 验证超时处理

### 9.2 设备断联测试

1. **正常断联**：
   - 客户端主动断开
   - 验证去重机制工作
   - 验证策略自动清理

2. **异常断联**：
   - 网络中断
   - 进程崩溃
   - 验证策略自动清理

3. **重复断联**：
   - 模拟多个断联触发点
   - 验证去重机制防止重复处理

### 9.3 性能测试

1. **并发连接**：
   - 测试大量设备同时连接
   - 验证内存使用
   - 验证性能表现

2. **频繁连接/断联**：
   - 测试设备频繁连接和断联
   - 验证资源正确释放
   - 验证无内存泄漏

## 10. 故障排查

### 10.1 常见问题

1. **鉴权失败**：
   - 检查认证服务是否运行
   - 检查网络连接
   - 检查证书配置
   - 查看日志中的错误信息

2. **策略未创建**：
   - 检查鉴权响应中的 `is_authorized` 标志
   - 检查 `rules` 字段是否为空
   - 查看 `DynamicAclIntegrator` 的日志

3. **断联通知重复**：
   - 检查去重机制是否正常工作
   - 查看日志中的去重信息
   - 检查是否有多个断联触发点

4. **策略未清理**：
   - 检查 `Arc` 引用计数
   - 检查 `DeMux`/`Face` 是否正确销毁
   - 使用内存分析工具检查内存泄漏

### 10.2 日志级别

建议在生产环境使用 `INFO` 级别，在调试时使用 `DEBUG` 或 `TRACE` 级别：

```bash
export RUST_LOG=zenoh=debug,zenoh::net::dynamic_acl=trace
```

## 11. 未来改进方向

1. **断联通知**：
   - 可选启用断联通知到认证服务
   - 支持批量通知
   - 支持异步通知队列

2. **策略缓存**：
   - 支持策略缓存，减少重复鉴权
   - 支持策略过期时间
   - 支持策略刷新机制

3. **监控和指标**：
   - 添加连接数统计
   - 添加鉴权成功率统计
   - 添加策略数量统计

4. **性能优化**：
   - 优化规则去重算法
   - 优化策略查询性能
   - 支持策略预加载

---

**文档版本**：1.0  
**最后更新**：2026-01-19  
**维护者**：Zenoh 开发团队

