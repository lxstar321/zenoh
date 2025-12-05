#!/usr/bin/env python3
"""
Zenoh mTLS 客户端鉴权服务

提供REST API接口，用于验证客户端身份和权限
基于client_id和client_type返回鉴权结果和权限列表

作者: Zenoh Team
"""

import json
import logging
from datetime import datetime
from typing import Dict, List, Optional, Any
from dataclasses import dataclass, asdict
from enum import Enum

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import JSONResponse
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field
import uvicorn

# 配置日志
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger('auth-service')

# ============================================================================
# 数据模型
# ============================================================================

class ClientType(str, Enum):
    """客户端类型枚举"""
    SENSOR = "sensor"
    CONTROLLER = "controller"
    GATEWAY = "gateway"
    MONITOR = "monitor"

@dataclass
class ClientInfo:
    """客户端信息"""
    client_id: str
    name: str
    organization: str
    description: str
    client_type: ClientType
    is_active: bool
    created_at: str
    last_seen: Optional[str] = None

@dataclass
class Permission:
    """权限定义"""
    action: str  # pub, sub, admin
    resource: str  # 消息模式，如 sensor/*, control/*
    description: str

@dataclass
class AuthResponse:
    """鉴权响应"""
    is_authorized: bool
    permissions: List[str]  # 简化的权限字符串列表
    client_info: Optional[Dict[str, Any]] = None
    error_message: Optional[str] = None

# Pydantic模型用于API请求/响应
class AuthRequest(BaseModel):
    """鉴权请求"""
    client_id: str = Field(..., description="客户端ID")
    client_type: str = Field(..., description="客户端类型")

    class Config:
        schema_extra = {
            "example": {
                "client_id": "sensor-client-001",
                "client_type": "sensor"
            }
        }

class AuthResponseModel(BaseModel):
    """鉴权响应模型"""
    is_authorized: bool
    permissions: List[str]
    client_info: Optional[Dict[str, Any]] = None
    error_message: Optional[str] = None

# ============================================================================
# 模拟数据库
# ============================================================================

class MockDatabase:
    """模拟数据库 - 使用JSON数据存储客户端信息"""

    def __init__(self):
        self.clients: Dict[str, ClientInfo] = {}
        self.permissions: Dict[str, List[Permission]] = {}
        self._load_mock_data()

    def _load_mock_data(self):
        """加载模拟数据"""
        # 客户端信息
        self.clients = {
            "sensor-client-001": ClientInfo(
                client_id="sensor-client-001",
                name="温度传感器",
                organization="智能制造部",
                description="车间温度监测传感器",
                client_type=ClientType.SENSOR,
                is_active=True,
                created_at="2024-01-15T08:00:00Z"
            ),
            "sensor-client-002": ClientInfo(
                client_id="sensor-client-002",
                name="湿度传感器",
                organization="智能制造部",
                description="车间湿度监测传感器",
                client_type=ClientType.SENSOR,
                is_active=True,
                created_at="2024-01-16T09:30:00Z"
            ),
            "control-client-001": ClientInfo(
                client_id="control-client-001",
                name="中央控制器",
                organization="控制中心",
                description="生产控制系统中央控制器",
                client_type=ClientType.CONTROLLER,
                is_active=True,
                created_at="2024-01-10T10:00:00Z"
            ),
            "gateway-client-001": ClientInfo(
                client_id="gateway-client-001",
                name="物联网网关",
                organization="网络部",
                description="连接车间设备和控制中心的网关",
                client_type=ClientType.GATEWAY,
                is_active=True,
                created_at="2024-01-12T14:20:00Z"
            ),
            "monitor-client-001": ClientInfo(
                client_id="monitor-client-001",
                name="监控系统",
                organization="运维部",
                description="系统监控和告警平台",
                client_type=ClientType.MONITOR,
                is_active=True,
                created_at="2024-01-14T16:45:00Z"
            )
        }

        # 权限配置
        self.permissions = {
            "sensor-client-001": [
                Permission("pub", "sensor/temperature/*", "发布温度传感器数据"),
                Permission("pub", "sensor/status/sensor-client-001", "发布传感器状态"),
                Permission("sub", "control/sensor-client-001/*", "订阅控制命令"),
            ],
            "sensor-client-002": [
                Permission("pub", "sensor/humidity/*", "发布湿度传感器数据"),
                Permission("pub", "sensor/status/sensor-client-002", "发布传感器状态"),
                Permission("sub", "control/sensor-client-002/*", "订阅控制命令"),
            ],
            "control-client-001": [
                Permission("pub", "control/*", "发布所有控制命令"),
                Permission("sub", "sensor/*", "订阅所有传感器数据"),
                Permission("sub", "system/status/*", "订阅系统状态"),
                Permission("admin", "system/*", "系统管理权限"),
            ],
            "gateway-client-001": [
                Permission("pub", "gateway/status", "发布网关状态"),
                Permission("sub", "sensor/*", "订阅传感器数据"),
                Permission("pub", "sensor/*/gateway", "转发传感器数据"),
                Permission("sub", "control/*", "订阅控制命令"),
                Permission("pub", "control/*/gateway", "转发控制命令"),
            ],
            "monitor-client-001": [
                Permission("sub", "sensor/*", "订阅传感器数据"),
                Permission("sub", "control/*", "订阅控制命令"),
                Permission("sub", "system/*", "订阅系统状态"),
                Permission("sub", "gateway/*", "订阅网关状态"),
                Permission("pub", "alert/*", "发布告警信息"),
                Permission("admin", "monitor/*", "监控系统管理"),
            ]
        }

        logger.info(f"已加载 {len(self.clients)} 个客户端和 {sum(len(p) for p in self.permissions.values())} 个权限规则")

    def get_client(self, client_id: str) -> Optional[ClientInfo]:
        """获取客户端信息"""
        return self.clients.get(client_id)

    def get_client_permissions(self, client_id: str) -> List[Permission]:
        """获取客户端权限"""
        return self.permissions.get(client_id, [])

    def authenticate_client(self, client_id: str, client_type: str) -> AuthResponse:
        """
        客户端鉴权

        Args:
            client_id: 客户端ID
            client_type: 客户端类型

        Returns:
            AuthResponse: 鉴权结果
        """
        try:
            # 查找客户端
            client = self.get_client(client_id)
            if not client:
                return AuthResponse(
                    is_authorized=False,
                    permissions=[],
                    error_message=f"客户端 '{client_id}' 不存在"
                )

            # 检查客户端是否激活
            if not client.is_active:
                return AuthResponse(
                    is_authorized=False,
                    permissions=[],
                    error_message=f"客户端 '{client_id}' 已禁用"
                )

            # 检查客户端类型是否匹配
            if client.client_type.value != client_type:
                return AuthResponse(
                    is_authorized=False,
                    permissions=[],
                    error_message=f"客户端类型不匹配: 期望 '{client.client_type.value}', 收到 '{client_type}'"
                )

            # 获取权限列表
            permissions = self.get_client_permissions(client_id)
            permission_strings = [f"{p.action}:{p.resource}" for p in permissions]

            # 更新最后访问时间
            client.last_seen = datetime.utcnow().isoformat() + "Z"

            # 返回鉴权成功结果
            return AuthResponse(
                is_authorized=True,
                permissions=permission_strings,
                client_info={
                    "name": client.name,
                    "organization": client.organization,
                    "description": client.description,
                    "created_at": client.created_at,
                    "last_seen": client.last_seen
                }
            )

        except Exception as e:
            logger.error(f"鉴权过程中出错: {e}")
            return AuthResponse(
                is_authorized=False,
                permissions=[],
                error_message=f"鉴权服务内部错误: {str(e)}"
            )

# ============================================================================
# 鉴权服务
# ============================================================================

class AuthService:
    """鉴权服务"""

    def __init__(self):
        self.database = MockDatabase()
        logger.info("🔐 鉴权服务初始化完成")

    def authenticate(self, client_id: str, client_type: str) -> AuthResponse:
        """执行客户端鉴权"""
        logger.info(f"🔍 鉴权请求: client_id='{client_id}', client_type='{client_type}'")

        result = self.database.authenticate_client(client_id, client_type)

        if result.is_authorized:
            logger.info(f"✅ 鉴权成功: {client_id} ({client_type}) - {len(result.permissions)} 个权限")
        else:
            logger.warning(f"❌ 鉴权失败: {client_id} ({client_type}) - {result.error_message}")

        return result

# ============================================================================
# FastAPI 应用
# ============================================================================

app = FastAPI(
    title="Zenoh mTLS 客户端鉴权服务",
    description="基于client_id和client_type提供客户端鉴权和权限查询服务",
    version="1.0.0",
    docs_url="/docs",
    redoc_url="/redoc"
)

# 添加CORS中间件
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # 在生产环境中应该限制为特定域名
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# 创建鉴权服务实例
auth_service = AuthService()

@app.get("/")
async def root():
    """服务根路径"""
    return {
        "service": "Zenoh mTLS Client Authorization Service",
        "version": "1.0.0",
        "status": "running",
        "docs": "/docs"
    }

@app.get("/health")
async def health_check():
    """健康检查"""
    return {
        "status": "healthy",
        "timestamp": datetime.utcnow().isoformat() + "Z",
        "clients_count": len(auth_service.database.clients)
    }

@app.post("/auth", response_model=AuthResponseModel)
async def authenticate_client(request: AuthRequest):
    """
    客户端鉴权接口

    接收客户端ID和类型，返回鉴权结果和权限列表
    """
    try:
        # 执行鉴权
        result = auth_service.authenticate(request.client_id, request.client_type)

        # 转换为响应模型
        response = AuthResponseModel(
            is_authorized=result.is_authorized,
            permissions=result.permissions,
            client_info=result.client_info,
            error_message=result.error_message
        )

        return response

    except Exception as e:
        logger.error(f"API错误: {e}")
        raise HTTPException(status_code=500, detail=f"服务内部错误: {str(e)}")

@app.get("/clients")
async def list_clients():
    """列出所有客户端（开发调试用）"""
    clients_info = []
    for client_id, client in auth_service.database.clients.items():
        permissions = auth_service.database.get_client_permissions(client_id)
        clients_info.append({
            "client_id": client_id,
            "name": client.name,
            "type": client.client_type.value,
            "organization": client.organization,
            "is_active": client.is_active,
            "permissions_count": len(permissions),
            "permissions": [f"{p.action}:{p.resource}" for p in permissions]
        })

    return {
        "total_clients": len(clients_info),
        "clients": clients_info
    }

@app.middleware("http")
async def log_requests(request: Request, call_next):
    """请求日志中间件"""
    start_time = datetime.utcnow()

    # 记录请求
    logger.info(f"📨 {request.method} {request.url.path} - {request.client.host if request.client else 'unknown'}")

    # 处理请求
    response = await call_next(request)

    # 记录响应
    duration = (datetime.utcnow() - start_time).total_seconds() * 1000
    logger.info(f"📤 {request.method} {request.url.path} - {response.status_code} - {duration:.1f}ms")
    return response

# ============================================================================
# 主程序
# ============================================================================

if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Zenoh mTLS 客户端鉴权服务")
    parser.add_argument("--host", default="0.0.0.0", help="监听主机 (默认: 0.0.0.0)")
    parser.add_argument("--port", type=int, default=8080, help="监听端口 (默认: 8080)")
    parser.add_argument("--reload", action="store_true", help="开发模式自动重载")

    args = parser.parse_args()

    logger.info("🚀 启动 Zenoh mTLS 客户端鉴权服务")
    logger.info(f"📡 监听地址: http://{args.host}:{args.port}")
    logger.info(f"📚 API文档: http://{args.host}:{args.port}/docs")
    logger.info(f"💚 健康检查: http://{args.host}:{args.port}/health")

    uvicorn.run(
        "auth_service:app",
        host=args.host,
        port=args.port,
        reload=args.reload,
        log_level="info"
    )
