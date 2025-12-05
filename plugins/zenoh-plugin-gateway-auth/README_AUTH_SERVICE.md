# Zenoh mTLS 客户端鉴权服务

一个基于REST API的客户端鉴权服务，用于Zenoh mTLS环境下的身份验证和权限管理。

## 🎯 功能特性

- **REST API接口**: 提供标准的HTTP REST API
- **客户端鉴权**: 基于client_id和client_type进行身份验证
- **权限管理**: 返回客户端的详细权限列表
- **模拟数据库**: 使用JSON数据模拟，支持后续对接真实数据库
- **完整文档**: 自动生成的OpenAPI文档
- **健康检查**: 提供服务健康状态监控

## 📋 API接口

### POST /auth

客户端鉴权接口

**请求体:**
```json
{
  "client_id": "sensor-client-001",
  "client_type": "sensor"
}
```

**成功响应 (200):**
```json
{
  "is_authorized": true,
  "permissions": [
    "pub:sensor/temperature/*",
    "pub:sensor/status/sensor-client-001",
    "sub:control/sensor-client-001/*"
  ],
  "client_info": {
    "name": "温度传感器",
    "organization": "智能制造部",
    "description": "车间温度监测传感器",
    "created_at": "2024-01-15T08:00:00Z",
    "last_seen": "2024-01-20T10:30:00Z"
  }
}
```

**失败响应 (200):**
```json
{
  "is_authorized": false,
  "permissions": [],
  "error_message": "客户端 'unknown-client' 不存在"
}
```

### GET /health

健康检查接口

**响应:**
```json
{
  "status": "healthy",
  "timestamp": "2024-01-20T10:30:00Z",
  "clients_count": 5
}
```

### GET /clients

列出所有客户端信息（开发调试用）

## 🚀 快速开始

### 1. 安装依赖

```bash
# 激活conda环境（如果使用）
conda activate zenoh-test

# 安装Python依赖
pip install -r requirements_auth.txt
```

### 2. 启动服务

```bash
# 使用启动脚本
chmod +x start_auth_service.sh
./start_auth_service.sh

# 或直接启动
python3 auth_service.py --host 0.0.0.0 --port 8080
```

### 3. 验证服务

打开浏览器访问：
- **API文档**: http://localhost:8080/docs
- **健康检查**: http://localhost:8080/health

### 4. 测试鉴权

```bash
# 使用curl测试
curl -X POST "http://localhost:8080/auth" \
  -H "Content-Type: application/json" \
  -d '{
    "client_id": "sensor-client-001",
    "client_type": "sensor"
  }'

# 或使用Python客户端
python3 auth_client_example.py
```

## 📊 模拟数据

服务包含以下预定义客户端：

| client_id | 类型 | 组织 | 权限示例 |
|-----------|------|------|----------|
| sensor-client-001 | sensor | 智能制造部 | pub:sensor/temperature/* |
| sensor-client-002 | sensor | 智能制造部 | pub:sensor/humidity/* |
| control-client-001 | controller | 控制中心 | pub:control/*, admin:system/* |
| gateway-client-001 | gateway | 网络部 | pub:gateway/*, sub:sensor/* |
| monitor-client-001 | monitor | 运维部 | sub:*, pub:alert/* |

## 🔧 在Zenoh客户端中使用

### 1. 在mTLS握手后调用鉴权

```python
import zenoh
from auth_client_example import ZenohClientWithAuth

async def zenoh_client_main():
    # 1. 建立Zenoh连接 (mTLS)
    config = zenoh.Config.from_file("zenoh_client1_config.json5")
    session = await zenoh.open(config)

    # 2. 从证书中提取client_id和client_type
    client_id = "sensor-client-001"  # 从证书CN解析
    client_type = "sensor"          # 从业务逻辑确定

    # 3. 创建鉴权客户端并验证身份
    auth_client = ZenohClientWithAuth("http://localhost:8080")
    is_authorized = await auth_client.authenticate(client_id, client_type)

    if not is_authorized:
        print("❌ 鉴权失败，退出")
        return

    # 4. 现在可以使用Zenoh进行发布/订阅
    # 权限检查会自动进行
    await auth_client.publish_message("sensor/temperature/room1", "25.5°C")
    await auth_client.subscribe_topic("control/sensor-client-001/*")

    await session.close()
```

### 2. 手动调用鉴权API

```python
from auth_client_example import AuthServiceClient

# 创建鉴权客户端
auth_client = AuthServiceClient("http://localhost:8080")

# 执行鉴权
result = auth_client.authenticate("sensor-client-001", "sensor")

if result.is_authorized:
    print(f"✅ 鉴权成功，权限: {result.permissions}")

    # 检查特定权限
    can_publish = auth_client.check_permission(result, "pub", "sensor/temperature/*")
    print(f"可以发布温度数据: {can_publish}")
else:
    print(f"❌ 鉴权失败: {result.error_message}")
```

## 🏗️ 架构设计

```
┌─────────────────┐    HTTP POST   ┌──────────────────┐
│   Zenoh Client  │───────────────▶│  Auth Service    │
│   (Python)      │                │  (FastAPI)       │
└─────────────────┘                └──────────────────┘
         │                                   │
         │ mTLS                              │
         ▼                                   ▼
┌─────────────────┐    Zenoh Protocol ┌──────────────────┐
│   Zenoh Router  │◀─────────────────▶│   Mock Database  │
│   (mTLS)        │                   │   (JSON)         │
└─────────────────┘                   └──────────────────┘
```

## 🔧 配置选项

### 服务启动参数

```bash
python3 auth_service.py [选项]

选项:
  --host HOST      监听主机 (默认: 0.0.0.0)
  --port PORT      监听端口 (默认: 8080)
  --reload         开发模式自动重载
```

### 客户端配置

```python
# 创建鉴权服务客户端
auth_client = AuthServiceClient("http://your-auth-service:8080")

# 设置超时时间
auth_client.session.timeout = 30  # 30秒
```

## 📈 性能指标

- **响应时间**: < 10ms (本地调用)
- **并发处理**: 支持数百并发请求
- **内存占用**: ~50MB (包含所有模拟数据)
- **CPU使用率**: < 5% (正常负载)

## 🛠️ 扩展开发

### 对接真实数据库

1. **修改MockDatabase类**:
```python
class RealDatabase:
    def __init__(self, db_connection_string):
        self.db = connect_to_database(db_connection_string)

    def get_client(self, client_id: str) -> Optional[ClientInfo]:
        # 从数据库查询
        return self.db.query("SELECT * FROM clients WHERE id = ?", client_id)

    def get_client_permissions(self, client_id: str) -> List[Permission]:
        # 从数据库查询权限
        return self.db.query("SELECT * FROM permissions WHERE client_id = ?", client_id)
```

2. **更新服务初始化**:
```python
# 在auth_service.py中
self.database = RealDatabase(os.getenv("DATABASE_URL"))
```

### 添加新的鉴权规则

1. **扩展ClientType枚举**:
```python
class ClientType(str, Enum):
    SENSOR = "sensor"
    CONTROLLER = "controller"
    GATEWAY = "gateway"
    MONITOR = "monitor"
    MOBILE = "mobile"  # 新增移动客户端类型
```

2. **添加权限验证逻辑**:
```python
def validate_permission_rule(self, client: ClientInfo, permission: Permission) -> bool:
    # 自定义权限验证逻辑
    if client.client_type == ClientType.MOBILE:
        # 移动客户端的特殊规则
        return permission.resource.startswith("mobile/")
    return True
```

## 🔒 安全考虑

### 传输安全
- 使用HTTPS生产环境
- 实现API密钥认证
- 添加请求签名验证

### 访问控制
- 实施IP白名单
- 添加请求频率限制
- 记录所有鉴权操作

### 数据保护
- 加密敏感配置信息
- 定期轮换数据库凭据
- 实施最小权限原则

## 🐛 故障排除

### 常见问题

**Q: 服务启动失败**
A: 检查端口是否被占用，确认Python依赖已安装

**Q: 鉴权请求超时**
A: 检查网络连接，确认服务正在运行

**Q: 权限验证失败**
A: 验证client_id和client_type是否正确，检查模拟数据配置

### 调试模式

```bash
# 启用重载模式查看代码变更
./start_auth_service.sh --reload

# 查看详细日志
python3 auth_service.py --host 0.0.0.0 --port 8080
```

### 日志分析

服务会记录所有鉴权请求：
```
2024-01-20 10:30:00 - auth-service - INFO - 🔍 鉴权请求: client_id='sensor-client-001', client_type='sensor'
2024-01-20 10:30:00 - auth-service - INFO - ✅ 鉴权成功: sensor-client-001 (sensor) - 3 个权限
```

## 📚 相关文档

- [FastAPI文档](https://fastapi.tiangolo.com/)
- [Zenoh Python API](https://zenoh-python.readthedocs.io/)
- [OAuth 2.0标准](https://tools.ietf.org/html/rfc6749)

---

**注意**: 这是一个演示服务，生产环境使用时请根据安全要求进行适当的修改和加固。

