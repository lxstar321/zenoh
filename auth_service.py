#!/usr/bin/env python3
"""
Zenoh Dynamic ACL Authentication Service

Mock service for testing dynamic ACL functionality.
Provides ACL rules, subjects, and policies based on client certificates.
"""

import logging
from typing import Dict, List, Optional, Any
from dataclasses import dataclass, asdict

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field
import uvicorn

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger('dynamic-acl-service')

# ============================================================================
# Data Models
# ============================================================================

@dataclass
class AclConfigRule:
    """ACL configuration rule matching Zenoh format"""
    id: str
    key_exprs: List[str]
    messages: List[str]
    permission: str
    flows: Optional[List[str]] = None

@dataclass
class SubjectConfig:
    """Subject configuration for client"""
    subject_id: Optional[str] = None
    interfaces: Optional[List[str]] = None
    cert_common_names: Optional[List[str]] = None
    usernames: Optional[List[str]] = None
    link_protocols: Optional[List[str]] = None
    zids: Optional[List[str]] = None

@dataclass
class DynamicAclResponse:
    """Dynamic ACL response"""
    is_authorized: bool
    rules: Optional[List[AclConfigRule]] = None
    subject_config: Optional[SubjectConfig] = None
    client_info: Optional[Dict[str, Any]] = None
    error_message: Optional[str] = None

@dataclass
class DeviceConnectRequest:
    """Device connection notification request"""
    client_id: str
    client_type: str
    timestamp: Optional[int] = None
    interface: Optional[str] = None
    link_protocol: Optional[str] = None

@dataclass
class DeviceDisconnectRequest:
    """Device disconnection notification request"""
    client_id: str
    client_type: str
    timestamp: Optional[int] = None
    reason: Optional[str] = None

@dataclass
class NotificationResponse:
    """Notification response"""
    success: bool
    message: Optional[str] = None
    error_message: Optional[str] = None

# Pydantic models for API
class AuthRequest(BaseModel):
    """Authentication request"""
    client_id: str = Field(..., description="Client ID (CN from certificate)")
    client_type: str = Field(..., description="Client type (OU from certificate)")

    class Config:
        json_schema_extra = {
            "example": {
                "client_id": "sensor-001",
                "client_type": "production"
            }
        }

class DeviceConnectRequestModel(BaseModel):
    """Device connection notification request"""
    client_id: str
    client_type: str
    timestamp: Optional[int] = None
    interface: Optional[str] = None
    link_protocol: Optional[str] = None

class DeviceDisconnectRequestModel(BaseModel):
    """Device disconnection notification request"""
    client_id: str
    client_type: str
    timestamp: Optional[int] = None
    reason: Optional[str] = None

class NotificationResponseModel(BaseModel):
    """Notification response"""
    success: bool
    message: Optional[str] = None
    error_message: Optional[str] = None

class DynamicAclResponseModel(BaseModel):
    """Dynamic ACL response model"""
    is_authorized: bool
    rules: Optional[List[Dict[str, Any]]] = None
    subject: Optional[Dict[str, Any]] = None
    client_info: Optional[Dict[str, Any]] = None
    error_message: Optional[str] = None

# ============================================================================
# Mock Database
# ============================================================================

class MockDatabase:
    """Mock database for testing dynamic ACL"""

    def __init__(self):
        self.client_rules: Dict[str, List[AclConfigRule]] = {}
        self._load_mock_data()

    def _load_mock_data(self):
        """Load mock ACL rules for different clients"""

        # Sensor clients - can publish sensor data and subscribe to control commands
        self.client_rules["sensor-001"] = [
            AclConfigRule(
                id="sensor-temp-pub",
                key_exprs=["sensor/temperature/*"],
                messages=["put"],
                permission="allow"
            ),
            AclConfigRule(
                id="sensor-status-pub",
                key_exprs=["sensor/status/sensor-001"],
                messages=["put"],
                permission="allow"
            ),
            AclConfigRule(
                id="sensor-control-sub",
                key_exprs=["control/sensor-001/*"],
                messages=["declare_subscriber"],
                permission="allow"
            )
        ]

        self.client_rules["sensor-002"] = [
            AclConfigRule(
                id="sensor-humidity-pub",
                key_exprs=["sensor/humidity/*"],
                messages=["put"],
                permission="allow"
            ),
            AclConfigRule(
                id="sensor-status-pub",
                key_exprs=["sensor/status/sensor-002"],
                messages=["put"],
                permission="allow"
            )
        ]

        # Test client - for integration testing
        self.client_rules["zenoh-client"] = [
            AclConfigRule(
                id="test-sensor-access",
                key_exprs=["sensor/temperature/*", "sensor/status/*"],
                messages=["put", "declare_subscriber"],
                flows=["ingress", "egress"],
                permission="allow"
            ),
            AclConfigRule(
                id="test-control-access",
                key_exprs=["control/*"],
                messages=["declare_subscriber"],
                flows=["ingress", "egress"],
                permission="allow"
            )
        ]

        # Controller client - can control everything and monitor sensors
        self.client_rules["controller-001"] = [
            AclConfigRule(
                id="controller-pub",
                key_exprs=["control/*"],
                messages=["put"],
                permission="allow"
            ),
            AclConfigRule(
                id="sensor-sub",
                key_exprs=["sensor/*"],
                messages=["declare_subscriber"],
                permission="allow"
            ),
            AclConfigRule(
                id="system-status-sub",
                key_exprs=["system/status/*"],
                messages=["declare_subscriber"],
                permission="allow"
            )
        ]

        # Gateway client - bridges sensor and control networks
        self.client_rules["gateway-001"] = [
            AclConfigRule(
                id="gateway-status-pub",
                key_exprs=["gateway/status"],
                messages=["put"],
                permission="allow"
            ),
            AclConfigRule(
                id="sensor-bridge-sub",
                key_exprs=["sensor/*"],
                messages=["declare_subscriber"],
                permission="allow"
            ),
            AclConfigRule(
                id="sensor-bridge-pub",
                key_exprs=["sensor/*/gateway"],
                messages=["put"],
                permission="allow"
            ),
            AclConfigRule(
                id="control-bridge-sub",
                key_exprs=["control/*"],
                messages=["declare_subscriber"],
                permission="allow"
            ),
            AclConfigRule(
                id="control-bridge-pub",
                key_exprs=["control/*/gateway"],
                messages=["put"],
                permission="allow"
            )
        ]

        logger.info(f"Loaded ACL rules for {len(self.client_rules)} clients")

    def get_client_acl_rules(self, client_id: str) -> Optional[List[AclConfigRule]]:
        """Get ACL rules for a client"""
        return self.client_rules.get(client_id)

    def authenticate_client(self, client_id: str, client_type: str) -> DynamicAclResponse:
        """Authenticate client and return ACL configuration"""
        try:
            logger.info(f"Authenticating client {client_id} with type: {client_type}")

            rules = self.get_client_acl_rules(client_id)

            if rules is None:
                return DynamicAclResponse(
                    is_authorized=False,
                    error_message=f"Client '{client_id}' not found"
                )

            # Apply client_type-based access control
            if client_type == "restricted":
                # Restricted type clients get limited permissions - no status publishing
                logger.info(f"Client {client_id} of type 'restricted' - limiting permissions")
                restricted_rules = [
                    rule for rule in rules
                    if not any("sensor/status" in str(key_expr) for key_expr in rule.key_exprs)
                ]
                rules = restricted_rules
            elif client_type == "admin":
                # Admin type clients get full access plus admin permissions
                logger.info(f"Client {client_id} of type 'admin' - granting full access")
                # Keep all rules for admin
            elif client_type == "production":
                # Production type clients get standard production permissions
                logger.info(f"Client {client_id} of type 'production' - standard production access")
                # Keep all rules for production
            elif client_type == "testing":
                # Testing type clients get testing permissions plus some additional access
                logger.info(f"Client {client_id} of type 'testing' - testing access with extras")
                # Keep all rules for testing
            else:
                # Default type handling
                logger.info(f"Client {client_id} of type '{client_type}' - applying standard rules")

            # Create subject config for the client
            subject_config = SubjectConfig(
                subject_id=f"subject-{client_id}",
                cert_common_names=[client_id],  # Use client_id as cert common name
                link_protocols=["tls"]
            )

            return DynamicAclResponse(
                is_authorized=True,
                rules=rules,
                subject_config=subject_config,
                client_info={
                    "client_id": client_id,
                    "client_type": client_type,
                    "authenticated_at": "2024-01-01T00:00:00Z"
                }
            )

        except Exception as e:
            logger.error(f"Authentication error for client {client_id}: {e}")
            return DynamicAclResponse(
                is_authorized=False,
                error_message=f"Authentication service error: {str(e)}"
            )

# ============================================================================
# Authentication Service
# ============================================================================

class AuthService:
    """Dynamic ACL authentication service"""

    def __init__(self):
        self.database = MockDatabase()
        logger.info("Dynamic ACL authentication service initialized")

    def authenticate(self, client_id: str, client_type: Optional[str] = None) -> DynamicAclResponse:
        """Authenticate client and return dynamic ACL configuration"""
        logger.info(f"Authentication request for client: {client_id}, type: {client_type}")

        result = self.database.authenticate_client(client_id)

        if result.is_authorized:
            rule_count = len(result.rules) if result.rules else 0
            logger.info(f"Authentication successful for {client_id}: {rule_count} rules")
        else:
            logger.warning(f"Authentication failed for {client_id}: {result.error_message}")

        return result

    def authenticate_client(self, client_id: str, client_type: str) -> DynamicAclResponse:
        """Authenticate client with client_type and return ACL configuration"""
        return self.database.authenticate_client(client_id, client_type)

# ============================================================================
# FastAPI Application
# ============================================================================

app = FastAPI(
    title="Zenoh Dynamic ACL Authentication Service",
    description="Mock service providing dynamic ACL configurations for testing",
    version="1.0.0",
    docs_url="/docs"
)

# Add CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Create authentication service instance
auth_service = AuthService()

@app.get("/")
async def root():
    """Service root endpoint"""
    return {
        "service": "Zenoh Dynamic ACL Authentication Service",
        "version": "1.0.0",
        "status": "running",
        "description": "Provides dynamic ACL configurations based on client certificate information",
        "endpoints": {
            "/auth": "Dynamic ACL authentication (requires certificate info)",
            "/health": "Health check",
            "/clients": "List available test clients"
        }
    }

@app.get("/health")
async def health_check():
    """Health check endpoint"""
    return {
        "status": "healthy",
        "clients_count": len(auth_service.database.client_rules)
    }

@app.post("/auth", response_model=DynamicAclResponseModel)
async def authenticate_client(request: AuthRequest):
    """
    Dynamic ACL authentication endpoint

    Receives client ID and returns ACL rules, subject config, and client info
    """
    try:
        result = auth_service.authenticate_client(request.client_id, request.client_type)

        # Convert to response model
        response = DynamicAclResponseModel(
            is_authorized=result.is_authorized,
            rules=[asdict(rule) for rule in result.rules] if result.rules else None,
            subject=asdict(result.subject_config) if result.subject_config else None,
            client_info=result.client_info,
            error_message=result.error_message
        )

        return response

    except Exception as e:
        logger.error(f"API error: {e}")
        raise HTTPException(status_code=500, detail=f"Internal server error: {str(e)}")

@app.post("/device/connect", response_model=NotificationResponseModel)
async def device_connect(request: DeviceConnectRequestModel):
    """
    Device connection notification endpoint

    Receives device connection notification and logs it
    """
    try:
        logger.info(f"Device connected: {request.client_id} ({request.client_type}) - "
                    f"interface: {request.interface}, protocol: {request.link_protocol}")

        return NotificationResponseModel(
            success=True,
            message=f"Device {request.client_id} connection acknowledged"
        )

    except Exception as e:
        logger.error(f"Device connect notification error: {e}")
        return NotificationResponseModel(
            success=False,
            error_message=f"Failed to process device connection: {str(e)}"
        )

@app.post("/device/disconnect", response_model=NotificationResponseModel)
async def device_disconnect(request: DeviceDisconnectRequestModel):
    """
    Device disconnection notification endpoint

    Receives device disconnection notification and logs it
    """
    try:
        logger.info(f"Device disconnected: {request.client_id} ({request.client_type}) - "
                    f"reason: {request.reason}")

        return NotificationResponseModel(
            success=True,
            message=f"Device {request.client_id} disconnection acknowledged"
        )

    except Exception as e:
        logger.error(f"Device disconnect notification error: {e}")
        return NotificationResponseModel(
            success=False,
            error_message=f"Failed to process device disconnection: {str(e)}"
        )

@app.get("/clients")
async def list_clients():
    """List all available test clients"""
    clients = []
    for client_id, rules in auth_service.database.client_rules.items():
        clients.append({
            "client_id": client_id,
            "rules_count": len(rules),
            "rules": [rule.id for rule in rules]
        })

    return {
        "total_clients": len(clients),
        "clients": clients
    }

# ============================================================================
# Main Program
# ============================================================================

if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Zenoh Dynamic ACL Authentication Service")
    parser.add_argument("--host", default="0.0.0.0", help="Host to bind to (default: 0.0.0.0)")
    parser.add_argument("--port", type=int, default=8080, help="Port to bind to (default: 8080)")

    args = parser.parse_args()

    logger.info("Starting Zenoh Dynamic ACL Authentication Service")
    logger.info(f"Listening on: http://{args.host}:{args.port}")
    logger.info(f"API docs: http://{args.host}:{args.port}/docs")
    logger.info(f"Health check: http://{args.host}:{args.port}/health")

    uvicorn.run(
        "auth_service:app",
        host=args.host,
        port=args.port,
        log_level="info"
    )

