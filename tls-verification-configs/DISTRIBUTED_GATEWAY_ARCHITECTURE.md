# Zenoh 分布式网关架构设计

## 1. 需求分析

基于您的需求分析，设计一个支持负载均衡的分布式 Zenoh 网关架构：

- **网关集群**: 多节点 Router 构成高可用网关集群
- **ELB 集成**: 外部负载均衡器分发连接
- **设备接入**: 外部设备通过网关转发消息
- **后端服务**: Agent 和数据收集系统处理转发消息
- **分布式架构**: 水平扩展，支持故障转移

## 2. 架构设计

```
┌─────────────────────────────────────────────────────────────────┐
│                    外部网络 (Internet)                           │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    AWS ELB / Nginx LB                        │ │
│  │  Load Balancer: tcp/7447 (TLS termination)                 │ │
│  └─────────────────────┬───────────────────────────────────────┘ │
└───────────────────────┼───────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────────┐
│                Zenoh 网关集群层 (DMZ)                          │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │            Router 集群 (3-5节点)                           │ │
│  │  • Router-01 (Primary)                                     │ │
│  │  • Router-02 (Secondary)                                   │ │
│  │  • Router-03 (Secondary)                                   │ │
│  │  • Router-04 (Backup)                                      │ │
│  │  • Router-05 (Backup)                                      │ │
│  └─────────────────────┬───────────────────────────────────────┘ │
└───────────────────────┼───────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────────┐
│                内部服务层 (Internal Network)                   │
│  ┌─────────────────────┬─────────────────────┬───────────────┐ │
│  │    Agent 服务       │   数据收集服务      │  监控服务     │ │
│  │  • Agent-01         │ • Collector-01      │ • Monitor-01  │ │
│  │  • Agent-02         │ • Collector-02      │ • Monitor-02  │ │
│  │  • Agent-03         │ • Collector-03      │ • Monitor-03  │ │
│  └─────────────────────┼─────────────────────┼───────────────┘ │
└────────────────────────┼─────────────────────┼──────────────────┘
                         │                     │
                         ▼                     ▼
┌────────────────────────┴─────────────────────┴──────────────────┐
│                外部设备层 (Device Network)                      │
│  ┌─────────────────────┬─────────────────────┬───────────────┐ │
│  │   IoT 设备          │   移动应用         │   Web 客户端   │ │
│  │ • Sensor-01         │ • Mobile-App-01    │ • Web-Client-01│ │
│  │ • Sensor-02         │ • Mobile-App-02    │ • Web-Client-02│ │
│  │ • Gateway-01        │ • Mobile-App-03    │ • Web-Client-03│ │
│  └─────────────────────┼─────────────────────┼───────────────┘ │
└────────────────────────┴─────────────────────┴──────────────────┘
```

## 2.2 单播 vs 多播选择决策

### 2.2.1 决策树分析

```
开始：选择通信协议
│
├── 🎯 通信模式?
│   ├── 一对一 (点对点数据传输) → 单播 ✅
│   ├── 一对多 (广播通知) → 多播 ✅
│   └── 多对多 (群组通信) → 根据具体场景选择
│
├── 🌐 网络环境?
│   ├── 生产环境 (固定拓扑) → 单播 ✅
│   │  ├── 理由：稳定、可控、易监控
│   ├── 云环境 (动态伸缩) → 多播 ✅
│   │  ├── 理由：自动发现新节点
│   └── 开发/ → 多播 ✅
│       └── 理由：简化配置，快速搭建
│
├── 📨 消息特点?
│   ├── 高可靠性要求 → 单播 ✅
│   │  ├── 理由：TC测试环境P保证交付，连接可监控
│   ├── 实时性要求 → 单播 ✅
│   │  ├── 理由：低延迟，QoS保证
│   └── 发现/通知类 → 多播 ✅
│       └── 理由：高效广播，无需维护连接表
│
├── 📈 扩展性需求?
│   ├── 静态集群 (<10节点) → 单播 ✅
│   │  ├── 理由：配置简单，性能可控
│   ├── 动态集群 (10-100节点) → 单播 + 多播混合
│   │  ├── 理由：单播处理数据，多播处理发现
│   └── 海量设备接入 (>1000节点) → 单播 + 负载均衡
│       └── 理由：ELB处理接入，多播处理集群内部
│
└── 🏗️ 架构决策结果
    ├── Router集群内部通信 → 单播 (稳定数据传输)
    ├── 服务发现和节点加入 → 多播 (动态节点发现)
    ├── 设备到网关连接 → 单播 (ELB负载均衡)
    ├── Agent/Collector通信 → 单播 (业务数据流)
    └── 集群状态同步 → 多播 (Gossip协议)
```

### 2.2.2 具体场景选择指南

| 场景 | 单播 | 多播 | 推荐理由 |
|-----|------|------|---------|
| **网关集群内部通信** | ✅ | ❌ | Router间需要稳定的点对点连接，单播提供可靠性保证 |
| **新节点加入集群** | ❌ | ✅ | 多播允许新节点快速发现现有集群，无需配置 |
| **故障检测和传播** | ❌ | ✅ | Gossip协议使用多播高效传播状态变化 |
| **设备连接网关** | ✅ | ❌ | ELB进行负载均衡，设备使用单播稳定连接 |
| **Agent处理业务数据** | ✅ | ❌ | 业务数据需要可靠传输，单播保证消息不丢失 |
| **数据收集和存储** | ✅ | ❌ | 分析数据要求准确性，单播提供传输保证 |
| **监控指标收集** | ✅ | ❌ | 监控数据不能丢失，单播确保可靠性 |

### 2.2.3 性能对比分析

| 维度 | 单播 (Unicast) | 多播 (Multicast) |
|-----|----------------|------------------|
| **网络开销** | 每个接收者一份数据 | 一份数据多个接收者 |
| **可靠性** | TCP保证交付 | UDP尽力交付 |
| **延迟** | 略高（建立连接） | 低（无需握手） |
| **可扩展性** | 随接收者线性增长 | 独立于接收者数量 |
| **配置复杂度** | 需要维护连接表 | 简单配置地址和端口 |
| **故障检测** | 连接状态可监控 | 需要额外心跳机制 |

### 2.2.4 混合使用策略

**生产环境推荐配置**:
```json5
{
  // 主要数据传输：单播
  transport: {
    unicast: {
      enabled: true,
      max_sessions: 1000  // 支持大量并发连接
    }
  },

  // 集群管理：多播
  scouting: {
    multicast: {
      enabled: true,
      address: "224.0.0.224:7446",
      autoconnect: ["router"],  // 只在Router间自动发现
      listen: {
        router: true,
        peer: false,
        client: false
      }
    },

    // Gossip用于大规模集群
    gossip: {
      enabled: true,
      multihop: true,  // 支持多跳传播
      target: ["router"]
    }
  },

  // 路由：智能单播
  routing: {
    mode: "linkstate"  // 基于单播连接的智能路由
  }
}
```

**混合使用的优势**:
1. **单播处理数据**: 保证可靠性、低延迟、有序交付
2. **多播处理控制**: 高效发现、管理、状态同步
3. **最佳性能**: 数据传输高效，集群管理灵活
4. **可扩展性**: 单播支持海量连接，多播支持动态伸缩

## 3. 单播 vs 多播选择决策详解

### 3.1 核心决策因素

| 决策维度 | 单播选择条件 | 多播选择条件 |
|---------|-------------|--------------|
| **通信模式** | 一对一数据传输 | 一对多发现/通知 |
| **网络环境** | 生产固定拓扑 | 云环境动态伸缩 |
| **消息特点** | 高可靠性/实时性 | 低延迟广播通知 |
| **扩展性需求** | 海量并发连接 | 动态节点发现 |
| **运维复杂度** | 偏好可控配置 | 偏好自动化管理 |

### 3.2 架构层级选择

#### 3.2.1 网关集群内部通信

**选择**: 单播 ✅

**理由**:
- Router 间需要稳定的点对点数据传输
- 单播提供 TCP 可靠性保证和流量控制
- Link State 路由算法基于单播连接工作
- 便于监控和故障排查

**配置示例**:
```json5
{
  // Router 间通过单播建立稳定连接
  connect: {
    endpoints: [
      "tcp/gateway-router-02:7447",
      "tcp/gateway-router-03:7447"
    ]
  },

  // 多播仅用于发现和管理
  scouting: {
    multicast: {
      enabled: true,           // ✅ 启用发现
      autoconnect: ["router"]  // ✅ 只连接 Router
    },
    gossip: {
      enabled: true,           // ✅ 启用状态同步
      target: ["router"]       // ✅ 只在 Router 间 gossip
    }
  }
}
```

#### 3.2.2 设备接入层

**选择**: 单播 ✅

**理由**:
- 设备需要可靠地发送数据到网关
- ELB 进行负载均衡，无需设备端多播
- 单播提供连接状态监控和 QoS 保证
- 便于实现设备认证和访问控制

**配置示例**:
```json5
{
  mode: "client",  // 或 "peer"
  connect: {
    endpoints: [
      "tls/gateway-elb:7447"  // ✅ 通过 ELB 单播连接
    ]
  },
  scouting: {
    multicast: { enabled: false },  // ❌ 不需要多播发现
    gossip: { enabled: false }
  }
}
```

#### 3.2.3 Agent/Collector 通信

**选择**: 单播 ✅

**理由**:
- 业务数据需要可靠传输
- Agent 处理结果需要准确送达
- 单播提供事务保证和错误重试
- 便于实现业务级别的确认机制

**配置示例**:
```json5
{
  mode: "peer",
  connect: {
    endpoints: [
      "tcp/gateway-router-01:7447",  // ✅ 直接单播连接
      "tcp/gateway-router-02:7447"   // ✅ 多路径冗余
    ]
  },
  scouting: {
    multicast: { enabled: false },  // ❌ 固定拓扑无需多播
    gossip: { enabled: false }
  }
}
```

### 3.3 性能对比分析

| 性能指标 | 单播 | 多播 | 混合使用 |
|---------|------|------|---------|
| **可靠性** | 高 (TCP) | 中 (UDP) | 高 (单播传输) |
| **延迟** | 中等 | 低 | 低 (多播发现) |
| **网络开销** | 随连接数线性增长 | 固定开销 | 优化 (按需使用) |
| **可扩展性** | 良好 (ELB扩展) | 优秀 (无连接表) | 最佳 (各取所长) |
| **配置复杂度** | 中等 | 低 | 中等 |
| **监控难度** | 易 (连接状态) | 中 (需要心跳) | 中等 |

### 3.4 混合使用最佳实践

#### 3.4.1 生产环境推荐配置

```json5
{
  // 1. 主要数据传输：单播保证可靠性
  transport: {
    unicast: {
      enabled: true,
      max_sessions: 10000,      // 支持海量并发
      qos: { enabled: true },   // 启用 QoS
      compression: { enabled: true }  // 启用压缩
    }
  },

  // 2. 集群管理：多播实现自动化
  scouting: {
    multicast: {
      enabled: true,             // ✅ 启用节点发现
      autoconnect: ["router"],   // ✅ 限制范围
      listen: {
        router: true,            // ✅ Router 监听
        peer: false,             // ❌ Peer 不监听
        client: false            // ❌ Client 不监听
      }
    },
    gossip: {
      enabled: true,             // ✅ 状态同步
      multihop: false,           // ✅ 单跳平衡性能
      target: ["router"]         // ✅ 限制目标
    }
  },

  // 3. 路由决策：基于单播连接
  routing: {
    mode: "linkstate"            // ✅ 智能路由
  }
}
```

#### 3.4.2 扩展性考虑

**小规模部署 (< 10节点)**:
```json5
{
  scouting: {
    multicast: { enabled: false },  // ❌ 关闭多播，使用手动配置
    gossip: { enabled: false }
  },
  connect: { endpoints: ["tcp:router-01:7447"] }  // ✅ 显式单播连接
}
```

**大规模部署 (> 100节点)**:
```json5
{
  scouting: {
    multicast: { enabled: true },   // ✅ 多播发现新节点
    gossip: {
      enabled: true,
      multihop: true                // ✅ 多跳扩展集群
    }
  },
  connect: { /* 动态生成 */ }       // ✅ 运行时发现连接
}
```

### 3.5 故障场景处理

#### 单播连接失败
- **检测**: TCP 连接状态监控
- **恢复**: 自动重连其他 Router
- **预防**: 多路径冗余连接

#### 多播网络分区
- **检测**: Gossip 协议检测隔离
- **恢复**: 网络恢复后自动重新加入
- **预防**: 配置种子节点确保连通性

## 4. 组件角色和 Zenoh 模式配置

### 3.1 网关 Router 集群

**角色**: 消息路由和转发核心

**Zenoh 模式**: `router`

**配置特点**:
```json5
{
  mode: "router",
  id: "gateway-router-01",

  // 外部监听 (ELB 连接)
  listen: {
    endpoints: [
      "tls/0.0.0.0:7447",  // 内部集群通信
      "tcp/0.0.0.0:8447"   // 管理接口
    ]
  },

  // 集群内部连接
  connect: {
    endpoints: [
      "tcp/gateway-router-02:7447",
      "tcp/gateway-router-03:7447",
      "tcp/gateway-router-04:7447",
      "tcp/gateway-router-05:7447"
    ]
  },

  // 集群发现和负载均衡
  scouting: {
    multicast: {
      enabled: false  // 生产环境禁用 UDP 多播，使用手动配置
    },
    gossip: {
      enabled: true,
      multihop: true,
      target: ["router"]
    }
  },

  // 链路状态路由
  routing: {
    mode: "linkstate",
    router: {
      linkstate: {
        // 基于网络延迟和带宽的权重
        weights: {
          "tcp/gateway-router-02:7447": 100,
          "tcp/gateway-router-03:7447": 100
        }
      }
    }
  },

  // 动态 ACL (安全控制)
  access_control: {
    enabled: true,
    query_strategy: "dynamic",
    dynamic_config: {
      endpoint: "http://auth-service:8080",
      timeout_seconds: 5
    }
  }
}
```

### 3.2 外部设备客户端

**角色**: 数据产生者和消费者

**Zenoh 模式**: `client` (推荐) 或 `peer`

**Client 模式配置** (推荐):
```json5
{
  mode: "client",
  id: "iot-sensor-01",

  // 只连接网关
  connect: {
    endpoints: [
      "tls/gateway-elb:7447"  // 通过 ELB 连接网关集群
    ]
  },

  // 不参与集群发现
  scouting: {
    multicast: { enabled: false },
    gossip: { enabled: false }
  },

  // 认证配置
  transport: {
    auth: {
      usrpwd: {
        user: "sensor-device",
        password: "device-token"
      }
    }
  }
}
```

**Peer 模式配置** (高级设备):
```json5
{
  mode: "peer",
  id: "smart-gateway-01",

  listen: {
    endpoints: ["tcp/0.0.0.0:7447"]
  },

  connect: {
    endpoints: [
      "tls/gateway-elb:7447",  // 主连接
      "tls/gateway-router-01:7447",  // 备用连接
      "tls/gateway-router-02:7447"   // 备用连接
    ]
  },

  scouting: {
    multicast: { enabled: false },
    gossip: { enabled: false }
  },

  routing: {
    peer: {
      mode: "linkstate"
    }
  }
}
```

### 3.3 数据处理 Agent

**角色**: 业务逻辑处理服务

**Zenoh 模式**: `peer`

**配置特点**:
```json5
{
  mode: "peer",
  id: "data-agent-01",

  listen: {
    endpoints: ["tcp/0.0.0.0:7447"]
  },

  // 连接网关集群
  connect: {
    endpoints: [
      "tcp/gateway-router-01:7447",
      "tcp/gateway-router-02:7447",
      "tcp/gateway-router-03:7447"
    ]
  },

  scouting: {
    multicast: {
      enabled: false
    },
    gossip: {
      enabled: false  // Agent 不参与 gossip
    }
  },

  routing: {
    peer: {
      mode: "linkstate",
      linkstate: {
        // 偏好最近的网关
        weights: {
          "tcp/gateway-router-01:7447": 100,
          "tcp/gateway-router-02:7447": 80
        }
      }
    }
  },

  // Agent 通常有写权限
  access_control: {
    enabled: true,
    default_permission: "allow"  // Agent 需要处理数据
  }
}
```

### 3.4 数据收集服务

**角色**: 数据存储和分析服务

**Zenoh 模式**: `peer`

**配置特点**:
```json5
{
  mode: "peer",
  id: "data-collector-01",

  listen: {
    endpoints: ["tcp/0.0.0.0:7447"]
  },

  connect: {
    endpoints: [
      "tcp/gateway-router-01:7447",
      "tcp/gateway-router-02:7447"
    ]
  },

  scouting: {
    multicast: { enabled: false },
    gossip: { enabled: false }
  },

  routing: {
    peer: {
      mode: "linkstate"
    }
  },

  // 收集服务通常只读
  access_control: {
    enabled: true,
    default_permission: "allow",
    rules: [
      {
        id: "collector-readonly",
        key_exprs: ["data/**"],
        messages: ["PUT"],
        permission: "ALLOW"
      }
    ]
  }
}
```

## 4. ELB 负载均衡配置

### 4.1 AWS ELB 配置

**监听器配置**:
```
Frontend: TCP/7447 (TLS termination)
Backend: TCP/7447 (转发到网关 Router)

健康检查:
- Protocol: TCP
- Port: 7447
- Healthy threshold: 2
- Unhealthy threshold: 2
- Timeout: 5 seconds
- Interval: 30 seconds
```

**目标组配置**:
```
Target type: IP
Protocol: TCP
Port: 7447

Targets:
- gateway-router-01:7447
- gateway-router-02:7447
- gateway-router-03:7447
- gateway-router-04:7447
- gateway-router-05:7447
```

### 4.2 Nginx 负载均衡配置

```nginx
upstream zenoh_gateway {
    # 网关集群
    server gateway-router-01:7447 weight=10;
    server gateway-router-02:7447 weight=10;
    server gateway-router-03:7447 weight=5;   # 备用节点权重较低
    server gateway-router-04:7447 weight=5;
    server gateway-router-05:7447 weight=5;

    # 负载均衡策略
    least_conn;  # 最少连接数策略
}

server {
    listen 7447 ssl;
    server_name gateway.example.com;

    # SSL 配置
    ssl_certificate /etc/ssl/certs/gateway.crt;
    ssl_certificate_key /etc/ssl/private/gateway.key;

    location / {
        # 代理到 Zenoh 网关集群
        proxy_pass tcp://zenoh_gateway;
        proxy_connect_timeout 5s;
        proxy_timeout 24h;

        # 健康检查
        health_check interval=30s;
    }
}
```

## 5. 分布式架构工作流程

### 5.1 消息流向示例

```
1. 设备连接 ELB
   IoT Sensor → AWS ELB → Gateway Router (负载均衡选择)

2. 消息路由转发
   Gateway Router → 查找订阅者 → 转发到 Agent/Collector

3. Agent 处理消息
   Agent 接收消息 → 业务处理 → 可选回复

4. Collector 收集数据
   Collector 订阅数据 → 存储分析 → 生成报告
```

### 5.2 故障转移流程

```
1. Router 节点故障
   AWS ELB 检测到不健康 → 从目标组移除

2. 集群内部重新路由
   其他 Router 通过 gossip 发现节点故障
   Link State 协议重新计算路由

3. 设备自动重连
   设备检测连接断开 → 通过 ELB 重连到健康节点

4. Agent/Collector 切换
   Peer 节点检测路由变化 → 自动切换到可用路径
```

## 6. 扩展性和性能优化

### 6.1 水平扩展策略

**Router 集群扩展**:
```bash
# 添加新 Router 节点
docker run -d zenoh-router \
  --connect gateway-router-01:7447 \
  --scouting-gossip-enabled true

# ELB 自动发现新节点
aws elbv2 register-targets \
  --target-group-arn $TARGET_GROUP_ARN \
  --targets Id=new-router:7447
```

**Agent/Collector 扩展**:
```bash
# 启动新 Agent 实例
docker run -d data-agent \
  --connect gateway-router-01:7447 \
  --connect gateway-router-02:7447

# 自动加入集群并开始处理消息
```

### 6.2 性能优化配置

> **配置项修正说明**: 本节已根据 Zenoh 的实际配置文件结构进行了修正。移除了不存在的配置项（如 `route_cache_size`、`heartbeat_interval`、`connection_timeout` 等），并使用 DEFAULT_CONFIG.json5 中真实存在的配置项进行优化。

> **注意**: 以下配置基于 Zenoh 默认配置文件中的实际可用选项。所有配置项都可以在 DEFAULT_CONFIG.json5 中找到对应定义。

**Router 性能调优**:
```json5
{
  // 传输层优化
  transport: {
    unicast: {
      // 最大并发会话数
      max_sessions: 1000,

      // 最大链路数
      max_links: 1,

      // 启用 QoS
      qos: {
        enabled: true
      },

      // 启用压缩
      compression: {
        enabled: true
      }
    },

    link: {
      tx: {
        // 批处理大小
        batch_size: 65535,

        // 队列大小配置
        queue: {
          size: {
            control: 2,
            real_time: 2,
            interactive_high: 2,
            interactive_low: 2,
            data_high: 16,
            data: 8,
            data_low: 2,
            background: 1
          }
        }
      },

      rx: {
        // 接收缓冲区大小
        buffer_size: 1048576,

        // 最大消息大小
        max_message_size: 1073741824
      }
    }
  },

  // 路由层优化
  routing: {
    router: {
      // 启用 Peer 故障转移代理
      peers_failover_brokering: true,

      // 链路状态路由权重
      linkstate: {
        // 基于网络条件的权重配置
        // transport_weights: [
        //   { dst_zid: "router-02-zid", weight: 100 },
        //   { dst_zid: "router-03-zid", weight: 80 }
        // ]
      }
    },

    // 兴趣声明超时
    interests: {
      timeout: 5000  // 5秒超时，降低延迟
    }
  }
}
```

**网络优化**:
```json5
{
  transport: {
    unicast: {
      // 打开链接超时
      open_timeout: 5000,

      // 接受链接超时
      accept_timeout: 5000,

      // 待处理链接队列大小
      accept_pending: 100,

      // 启用低延迟传输
      lowlatency: false,

      // QoS 配置
      qos: {
        enabled: true
      },

      // 压缩配置
      compression: {
        enabled: true
      }
    },

    link: {
      tx: {
        // 链路租约时间
        lease: 10000,

        // 保活消息数量
        keep_alive: 4,

        // 批处理大小
        batch_size: 65535,

        // 队列配置优化
        queue: {
          size: {
            // 根据业务优先级调整队列大小
            data_high: 32,  // 高优先级数据
            data: 16,       // 普通数据
            data_low: 4     // 低优先级数据
          }
        }
      },

      rx: {
        // 接收缓冲区大小
        buffer_size: 1048576,

        // 最大消息大小
        max_message_size: 1073741824
      }
    }
  }
}
```

## 7. 监控和运维

### 7.1 监控指标

**网关集群监控**:
- 连接数统计
- 消息吞吐量
- 路由延迟
- 节点健康状态

**ELB 监控**:
- 请求数
- 错误率
- 响应时间
- 目标健康状态

### 7.2 日志聚合

```json5
{
  logging: {
    level: "info",
    format: "json",

    // 发送到集中日志系统
    outputs: [
      {
        type: "tcp",
        address: "log-collector:514"
      }
    ]
  }
}
```

## 8. 部署架构总结

| 组件 | Zenoh 模式 | 部署位置 | 扩展方式 | 主要职责 |
|-----|-----------|---------|---------|---------|
| **网关 Router** | router | DMZ | 水平扩展 | 消息路由转发 |
| **外部设备** | client/peer | 设备网络 | 动态连接 | 数据产生消费 |
| **数据 Agent** | peer | 内部网络 | 水平扩展 | 业务逻辑处理 |
| **数据收集器** | peer | 内部网络 | 水平扩展 | 数据存储分析 |

**ELB 负载均衡器** 位于最前端，负责将外部连接均匀分发到网关 Router 集群。网关内部通过 Zenoh 的链路状态路由和 gossip 协议实现集群间的智能负载均衡和故障转移。

这种架构提供了**高可用**、**高扩展性**和**高性能**的分布式消息系统，支持海量设备连接和复杂的数据处理需求。
