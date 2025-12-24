//! Dynamic ACL provider interface

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::any::Any;
use zenoh_config::{AclConfigRule, AclConfigSubjects, AclConfigPolicyEntry, InterceptorLink, ZenohId};
use zenoh_link_commons::LinkAuthId;
use zenoh_result::ZResult;

#[cfg(feature = "dynamic_acl")]
pub use crate::net::dynamic_acl::client::AuthResponse;

/// Certificate information extracted from TLS connection
#[cfg(feature = "dynamic_acl")]
#[derive(Debug, Clone)]
pub struct CertificateInfo {
    pub common_name: String,                    // 从LinkAuthId::Tls中提取
    pub organizational_unit: Option<String>,    // 复用现有OU验证逻辑
    pub organization: Option<String>,           // 可扩展字段
    pub interface: Option<String>,              // 从Link地址信息提取
    pub link_protocol: InterceptorLink,         // 固定为Tls
    pub zid: ZenohId,                          // 从Transport中获取
}

#[cfg(feature = "dynamic_acl")]
impl CertificateInfo {
    /// Extract certificate information from TLS link
    pub fn from_link(link: &zenoh_link_commons::Link) -> Option<Self> {
        // Extract common name and OU from LinkAuthId
        // LinkAuthId::Tls may contain either:
        // - "CN" (if only CN is available)
        // - "CN:OU" (if both CN and OU are available)
        let (common_name, organizational_unit) = match &link.auth_identifier {
            LinkAuthId::Tls(Some(auth_value)) => {
                // Try to parse "CN:OU" format
                if let Some((cn, ou)) = auth_value.split_once(':') {
                    (cn.to_string(), Some(ou.to_string()))
                } else {
                    // Only CN is available
                    (auth_value.clone(), None)
                }
            }
            _ => return None, // Only support TLS with client certificate
        };

        // Extract interface from link address
        let interface = link.interfaces.first().cloned();

        Some(CertificateInfo {
            common_name,
            organizational_unit,
            organization: None,       // TODO: extract from certificate
            interface,
            link_protocol: InterceptorLink::Tls,
            zid: ZenohId::default(), // TODO: get from transport context
        })
    }
}

/// Authentication service response format
#[cfg(feature = "dynamic_acl")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicAclResponse {
    pub is_authorized: bool,           // 认证服务返回的授权状态
    pub rules: Option<Vec<AclConfigRule>>, // 直接返回ACL规则列表
    pub subject_config: Option<SubjectConfig>, // 可选的subject配置信息
    pub client_info: Option<serde_json::Value>, // 客户端详细信息
    pub error_message: Option<String>, // 错误信息
}

/// Authentication service provided subject configuration information
#[cfg(feature = "dynamic_acl")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectConfig {
    pub interfaces: Option<Vec<String>>,        // 允许的网络接口
    pub usernames: Option<Vec<String>>,         // 允许的用户名
    pub link_protocols: Option<Vec<String>>,    // 允许的链路协议
    pub zids: Option<Vec<String>>,             // 允许的Zenoh实例ID
}

/// Dynamic ACL provider trait - abstracts authentication service interface
#[cfg(feature = "dynamic_acl")]
#[async_trait]
pub trait DynamicAclProvider: Send + Sync {
    /// Get ACL configuration based on certificate information
    async fn get_acl_config(&self, cert_info: &CertificateInfo) -> ZResult<DynamicAclResponse>;

    /// Validate if cached configuration is still valid
    async fn validate_cache(&self, subject_id: usize) -> ZResult<bool> {
        // Default implementation - cache is always valid
        Ok(true)
    }
}
