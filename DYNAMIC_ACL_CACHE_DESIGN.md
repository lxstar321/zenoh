# 动态 ACL 缓存数据结构设计

## 1. 概述

动态 ACL 系统实现了多层次的缓存机制，用于优化性能、减少重复计算和防止重复处理。主要包含全局规则缓存、连接级策略缓存和去重缓存。

## 2. 缓存数据结构总表

| 缓存名称 | 数据结构类型 | 存储位置 | 键类型 | 值类型 | 生命周期 | 并发访问 | 清理策略 |
|---------|-------------|---------|-------|-------|---------|----------|----------|
| **全局规则缓存** | `HashMap<RuleContent, String>` | `DynamicAclIntegrator` | `RuleContent` (规则内容哈希) | `String` (规则ID) | 进程级别 | 单线程 (`&mut self`) | 永久缓存 |
| **Subject缓存** | `Vec<SubjectEntry>` | `PolicyEnforcer.subject_store` | `usize` (Subject ID) | `SubjectEntry` (Subject内容) | 连接级别 | 连接独占 | 连接断开自动清理 |
| **Policy缓存** | `HashMap<usize, PolicyForSubject>` | `PolicyEnforcer.policy_map` | `usize` (Subject ID) | `PolicyForSubject` (权限配置) | 连接级别 | 连接独占 | 连接断开自动清理 |
| **去重缓存** | `HashSet<String>` | 全局静态变量 | `String` (endpoint) | - (集合成员) | 进程级别 | `Mutex`保护 | 10秒定时清理 |

## 3. 详细数据结构说明

### 3.1 缓存字段详细表格

#### 全局规则缓存字段表

| 字段名称 | 数据类型 | 说明 | 用途 | 示例 |
|---------|---------|------|------|------|
| `rule_processor` | `RuleProcessor` | 规则处理器实例 | 处理规则逻辑 | - |
| `global_rule_registry` | `HashMap<RuleContent, String>` | 全局规则注册表 | 规则去重存储 | `{"rule1": "clientA-rule1"}` |

#### RuleContent 结构字段表

| 字段名称 | 数据类型 | 说明 | 用途 | 示例 |
|---------|---------|------|------|------|
| `key_exprs` | `Vec<String>` | Key 表达式列表 | 定义访问的资源路径 | `["/sensor/**", "/data/temp"]` |
| `messages` | `Vec<String>` | 消息类型列表 | 定义允许的消息操作 | `["PUT", "GET"]` |
| `flows` | `Option<Vec<String>>` | 流量方向列表 | 定义入站/出站限制 | `Some(["ingress"])` |
| `permission` | `String` | 权限类型 | 允许/拒绝操作 | `"ALLOW"` |

#### SubjectStore 结构字段表

| 字段名称 | 数据类型 | 说明 | 用途 | 示例 |
|---------|---------|------|------|------|
| `inner` | `Vec<SubjectEntry>` | Subject 条目列表 | 存储所有 Subject | 包含多个 SubjectEntry |

#### SubjectEntry 结构字段表

| 字段名称 | 数据类型 | 说明 | 用途 | 示例 |
|---------|---------|------|------|------|
| `id` | `usize` | Subject 唯一标识 | 用于 PolicyMap 关联 | `1` |
| `subject` | `Subject` | Subject 具体内容 | 匹配条件定义 | 包含证书、接口等信息 |

#### PolicyForSubject 结构字段表

| 字段名称 | 数据类型 | 说明 | 用途 | 示例 |
|---------|---------|------|------|------|
| `ingress` | `ActionPolicy` | 入站权限策略 | 入站消息权限控制 | 包含各种消息类型的权限 |
| `egress` | `ActionPolicy` | 出站权限策略 | 出站消息权限控制 | 包含各种消息类型的权限 |

#### ActionPolicy 结构字段表

| 字段名称 | 数据类型 | 说明 | 用途 | 示例 |
|---------|---------|------|------|------|
| `put` | `PermissionMap` | PUT 消息权限 | 控制数据发布权限 | key_expr → bool 映射 |
| `delete` | `PermissionMap` | DELETE 消息权限 | 控制数据删除权限 | key_expr → bool 映射 |
| `get` | `PermissionMap` | GET 消息权限 | 控制数据查询权限 | key_expr → bool 映射 |
| `reply` | `PermissionMap` | REPLY 消息权限 | 控制响应消息权限 | key_expr → bool 映射 |

#### PermissionMap 结构字段表

| 字段名称 | 数据类型 | 说明 | 用途 | 示例 |
|---------|---------|------|------|------|
| `inner` | `HashMap<String, bool>` | 权限映射表 | key_expr → 权限状态 | `{"/sensor/temp": true}` |

### 3.2 缓存操作方法表

#### DynamicAclIntegrator 方法表

| 方法名称 | 参数 | 返回类型 | 说明 | 复杂度 |
|---------|------|---------|------|--------|
| `new()` | - | `Self` | 创建新的集成器实例 | O(1) |
| `deduplicate_and_assign_rule_ids()` | `client_id, rules` | `ZResult<Vec<AclConfigRule>>` | 规则去重并分配ID | O(n) |
| `generate_subject_for_client()` | `client_id, subject_config` | `ZResult<AclConfigSubjects>` | 生成客户端Subject | O(1) |
| `generate_policy_for_client()` | `client_id, rules, subject` | `ZResult<AclConfigPolicyEntry>` | 生成客户端Policy | O(1) |
| `integrate_acl_config()` | `client_id, response` | `ZResult<(Vec<AclConfigRule>, Vec<AclConfigSubjects>, Vec<AclConfigPolicyEntry>)>` | 集成完整ACL配置 | O(n) |

#### PolicyEnforcer 方法表

| 方法名称 | 参数 | 返回类型 | 说明 | 复杂度 |
|---------|------|---------|------|--------|
| `new()` | - | `PolicyEnforcer` | 创建策略执行器实例 | O(1) |
| `init()` | `acl_config` | `ZResult<()>` | 初始化ACL配置 | O(n) |
| `check_permission()` | `subjects, message, flow` | `Permission` | 检查消息权限 | O(1) |
| `init_static_acl()` | `acl_config` | `ZResult<()>` | 初始化静态ACL | O(n) |
| `init_dynamic_acl()` | `acl_config` | `ZResult<()>` | 初始化动态ACL | O(1) |

#### SubjectStore 方法表

| 方法名称 | 参数 | 返回类型 | 说明 | 复杂度 |
|---------|------|---------|------|--------|
| `query()` | `SubjectQuery` | `Iterator<&SubjectEntry>` | 查询匹配的Subject | O(n) - 可优化 |

### 3.3 缓存性能指标表

#### 内存使用表

| 缓存类型 | 内存增长模式 | 平均对象大小 | 清理频率 | 内存峰值 |
|---------|-------------|-------------|---------|---------|
| 全局规则缓存 | 随规则种类线性增长 | ~200字节/规则 | 不清理 | 长期稳定 |
| 连接级Subject缓存 | 随Subject数量线性增长 | ~100字节/Subject | 连接断开时 | 连接期间 |
| 连接级Policy缓存 | 随权限规则数量线性增长 | ~500字节/Policy | 连接断开时 | 连接期间 |
| 去重缓存 | 固定上限（时间窗口） | ~50字节/条目 | 每10秒 | 低峰值 |

#### 查询性能表

| 操作类型 | 数据结构 | 时间复杂度 | 空间复杂度 | 并发安全性 |
|---------|---------|-----------|-----------|-----------|
| 规则去重查找 | HashMap | O(1) | O(n) | 单线程保证 |
| Subject查询 | Vec线性搜索 | O(n) | O(n) | 连接独占 |
| 权限检查 | HashMap | O(1) | O(n) | 连接独占 |
| 去重检查 | HashSet | O(1) | O(n) | Mutex保护 |
| 规则注册 | HashMap插入 | O(1) | O(n) | 单线程保证 |

### 3.4 缓存生命周期管理表

#### 缓存实例管理表

| 缓存实例 | 创建时机 | 销毁时机 | 管理方式 | 资源所有权 |
|---------|---------|----------|---------|-----------|
| `DynamicAclIntegrator` | AclEnforcer初始化时 | 程序退出 | 单例模式 | AclEnforcer持有 |
| `PolicyEnforcer` | 每个连接创建时 | 连接断开时 | 连接独占 | Arc智能指针 |
| `SubjectStore` | PolicyEnforcer初始化时 | PolicyEnforcer销毁时 | 值语义 | PolicyEnforcer持有 |
| `PolicyMap` | PolicyEnforcer初始化时 | PolicyEnforcer销毁时 | 值语义 | PolicyEnforcer持有 |
| `RECENT_DISCONNECTS` | 首次访问时 | 程序退出 | Lazy静态 | 全局静态 |

#### 缓存数据生命周期表

| 数据类型 | 创建时机 | 更新时机 | 清理时机 | 持久性 |
|---------|---------|----------|---------|-------|
| 规则注册表条目 | 首次遇到新规则时 | 永不更新 | 永不清里 | 进程持久 |
| Subject条目 | ACL配置初始化时 | 永不更新 | 连接断开时 | 连接持久 |
| Policy映射 | ACL配置初始化时 | 永不更新 | 连接断开时 | 连接持久 |
| 去重缓存条目 | 断联事件发生时 | 永不更新 | 10秒后或程序退出 | 临时持久 |

## 2. 核心缓存数据结构

### 2.1 全局规则缓存 (DynamicAclIntegrator)

**数据结构：**
```rust
pub struct DynamicAclIntegrator {
    rule_processor: RuleProcessor,
    global_rule_registry: HashMap<RuleContent, String>,  // 🔑 核心缓存
}

/// Rule content for deduplication (ignores ID)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RuleContent {
    pub key_exprs: Vec<String>,              // key表达式列表
    pub messages: Vec<String>,               // 消息类型列表
    pub flows: Option<Vec<String>>,          // 流量方向（可选）
    pub permission: String,                  // 权限类型
}
```

**缓存策略：**
- **键 (Key)**: `RuleContent` - 基于规则内容的哈希值（忽略规则ID）
- **值 (Value)**: `String` - 分配的唯一规则ID
- **生命周期**: 随 `DynamicAclIntegrator` 实例存在（通常为单例）
- **清理策略**: 无主动清理，规则一旦注册即永久缓存

**设计目标：**
- 消除相同规则内容的重复存储
- 跨客户端复用相同的权限规则
- 减少内存占用

### 2.2 连接级策略缓存 (PolicyEnforcer)

**数据结构：**
```rust
pub struct PolicyEnforcer {
    pub(crate) acl_enabled: bool,
    pub(crate) default_permission: Permission,
    pub(crate) subject_store: SubjectStore,      // 🔑 Subject 缓存
    pub(crate) policy_map: PolicyMap,            // 🔑 Policy 缓存
    pub(crate) interface_enabled: InterfaceEnabled,
}

// 类型定义
type PolicyMap = HashMap<usize, PolicyForSubject, RandomState>;
pub(crate) struct SubjectStore {
    inner: Vec<SubjectEntry>,  // Subject 条目列表
}
```

**缓存层次：**

#### 2.2.1 SubjectStore 缓存
```rust
pub(crate) struct SubjectStore {
    inner: Vec<SubjectEntry>,  // 存储所有 Subject 条目
}

pub(crate) struct SubjectEntry {
    pub(crate) id: usize,           // 唯一ID
    pub(crate) subject: Subject,    // Subject 内容
}
```

- **存储方式**: 向量 (Vec) 存储所有 Subject
- **查询方式**: 线性搜索（当前实现，后续可优化为索引）
- **生命周期**: 每个 PolicyEnforcer 实例独立

#### 2.2.2 PolicyMap 缓存
```rust
type PolicyMap = HashMap<usize, PolicyForSubject>;

// 完整的权限映射结构
PolicyForSubject {
    ingress: ActionPolicy,  // 入站权限
    egress: ActionPolicy,   // 出站权限
}

ActionPolicy {
    put: PermissionMap,     // PUT 消息权限
    delete: PermissionMap,  // DELETE 消息权限
    // ... 其他消息类型
}

PermissionMap {
    // key_expr -> permission 的映射
    inner: HashMap<String, bool>
}
```

**缓存策略：**
- **键 (Key)**: `usize` - Subject ID
- **值 (Value)**: `PolicyForSubject` - 该 Subject 的完整权限配置
- **生命周期**: 每个连接的 PolicyEnforcer 实例独立

### 2.3 去重缓存 (RuntimeSession)

**数据结构：**
```rust
// 全局静态缓存（进程级别）
static RECENT_DISCONNECTS: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));
static LAST_CLEANUP: Lazy<Mutex<Option<Instant>>> = Lazy::new(|| Mutex::new(None));
```

**缓存策略：**
- **键 (Key)**: `String` - endpoint 字符串 (如 "tls/127.0.0.1:52374")
- **值 (Value)**: 无（使用 HashSet 仅存储键）
- **生命周期**: 进程级别，定期清理
- **清理策略**: 每 10 秒清理一次过期记录

## 3. 缓存设计原则

### 3.1 分层缓存架构

```
┌─────────────────────────────────────────┐
│  全局规则缓存 (DynamicAclIntegrator)   │  ← 进程级别
│  - 规则内容去重                         │
│  - 跨客户端复用                         │
└─────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────┐
│  连接级策略缓存 (PolicyEnforcer)       │  ← 连接级别
│  - SubjectStore: Subject 查找          │
│  - PolicyMap: 权限映射                  │
└─────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────┐
│  去重缓存 (RuntimeSession)             │  ← 进程级别
│  - 断联事件去重                         │
│  - 时间窗口清理                         │
└─────────────────────────────────────────┘
```

### 3.2 缓存生命周期管理

#### 3.2.1 全局规则缓存
- **创建时机**: `DynamicAclIntegrator::new()`
- **销毁时机**: 程序退出
- **内存管理**: 随进程生命周期，规则累积不清理

#### 3.2.2 连接级策略缓存
- **创建时机**: 每个设备连接时创建独立的 `PolicyEnforcer`
- **销毁时机**: 设备断联时通过 RAII 自动清理
- **内存管理**: 每个连接独立，断联即释放

#### 3.2.3 去重缓存
- **创建时机**: 首次访问时通过 `Lazy` 初始化
- **清理时机**: 定时清理（10秒间隔）
- **内存管理**: 进程级别，防止断联事件重复处理

## 4. 缓存优化策略

### 4.1 规则去重优化

**问题**: 相同权限规则在多个客户端间重复存储

**解决方案**:
```rust
// 1. 计算规则内容哈希
let rule_content = RuleContent::from_rule(rule);

// 2. 检查全局注册表
let assigned_id = if let Some(existing_id) = self.global_rule_registry.get(&rule_content) {
    // ✅ 复用现有规则ID
    existing_id.clone()
} else {
    // ❌ 创建新规则ID并注册
    let new_id = generate_unique_id();
    self.global_rule_registry.insert(rule_content, new_id.clone());
    new_id
};
```

**效果**:
- 减少内存占用
- 提高规则处理效率
- 维护规则一致性

### 4.2 连接隔离优化

**问题**: 不同设备的权限策略相互影响

**解决方案**:
- 每个连接创建独立的 `PolicyEnforcer` 实例
- 使用 `Arc<PolicyEnforcer>` 进行引用计数管理
- 断联时自动释放资源

**效果**:
- 设备权限完全隔离
- 资源自动管理
- 无内存泄漏风险

### 4.3 去重时间窗口优化

**问题**: 断联事件可能被多个触发点重复处理

**解决方案**:
```rust
// 时间窗口去重逻辑
let now = Instant::now();
if should_cleanup(&last_cleanup, now) {
    recents.clear();  // 清理过期记录
}

// 检查是否在时间窗口内重复
if recents.contains(&endpoint) {
    return;  // 跳过重复处理
}
```

**效果**:
- 防止重复的断联通知
- 控制内存使用
- 保证事件处理的幂等性

## 5. 性能特征

### 5.1 内存占用

| 缓存类型 | 内存特征 | 清理策略 |
|---------|---------|---------|
| 全局规则缓存 | 随规则数量线性增长 | 不清理（永久缓存） |
| 连接级策略缓存 | 随连接数量线性增长 | 连接断开时自动清理 |
| 去重缓存 | 固定上限（时间窗口限制） | 定时清理（10秒间隔） |

### 5.2 查询性能

| 操作 | 时间复杂度 | 优化空间 |
|-----|-----------|---------|
| 规则去重查找 | O(1) HashMap | 已优化 |
| Subject 查询 | O(n) 线性搜索 | 可索引优化 |
| 权限检查 | O(1) HashMap | 已优化 |
| 去重检查 | O(1) HashSet | 已优化 |

### 5.3 并发安全性

- **全局规则缓存**: 通过 `&mut self` 确保单线程访问
- **连接级策略缓存**: 每个连接独立，无并发问题
- **去重缓存**: 使用 `Mutex` 保护并发访问

## 6. 扩展性考虑

### 6.1 缓存大小限制

当前实现无缓存大小限制，未来可添加：
- 全局规则缓存大小上限
- LRU 淘汰策略
- 内存使用监控

### 6.2 查询性能优化

- Subject 查询可添加索引结构
- 可考虑缓存编译的权限规则
- 支持批量权限检查

### 6.3 分布式缓存

未来可扩展为分布式缓存：
- Redis 存储全局规则
- 分布式锁保证一致性
- 多节点缓存同步

### 3.5 缓存对比总结表

| 对比维度 | 全局规则缓存 | 连接级Subject缓存 | 连接级Policy缓存 | 去重缓存 |
|---------|-------------|------------------|------------------|---------|
| **主要用途** | 规则去重复用 | Subject快速查找 | 权限策略存储 | 事件去重 |
| **数据结构** | HashMap | Vec | HashMap | HashSet |
| **键类型** | RuleContent哈希 | Subject ID (usize) | Subject ID (usize) | endpoint字符串 |
| **值类型** | 规则ID字符串 | SubjectEntry | PolicyForSubject | 集合成员 |
| **作用域** | 进程全局 | 连接独占 | 连接独占 | 进程全局 |
| **并发控制** | 单线程访问 | 无需并发控制 | 无需并发控制 | Mutex保护 |
| **内存管理** | 长期累积 | 连接RAII清理 | 连接RAII清理 | 定时清理 |
| **查询性能** | O(1) | O(n) | O(1) | O(1) |
| **主要优化目标** | 内存节省 | 查询速度 | 权限检查速度 | 事件处理幂等 |
| **故障影响范围** | 影响规则注册 | 影响单个连接 | 影响单个连接 | 影响事件处理 |

---

**文档版本**: 1.0
**最后更新**: 2026-01-19
**维护者**: Zenoh 开发团队
