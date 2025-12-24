//! Authentication service client
//!
//! This module provides HTTP client functionality to communicate with
//! external authentication services for dynamic ACL authorization.

use std::time::Duration;
use serde::{Deserialize, Serialize};
#[cfg(feature = "dynamic_acl")]
use zenoh_config::DynamicAclConfig;
#[cfg(feature = "dynamic_acl")]
use zenoh_result::{bail, ZResult};

// ============================================================================
// Request/Response Structures
// ============================================================================

/// Authentication request sent to the external service
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthRequest {
    pub client_id: String,
    pub client_type: String,  // Required field, not Optional
}

/// Device connection notification request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceConnectRequest {
    pub client_id: String,
    pub client_type: String,
    pub timestamp: Option<u64>,  // Unix timestamp in seconds
    pub interface: Option<String>,
    pub link_protocol: Option<String>,  // e.g., "tls", "quic"
}

/// Device disconnection notification request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceDisconnectRequest {
    pub client_id: String,
    pub client_type: String,
    pub timestamp: Option<u64>,  // Unix timestamp in seconds
    pub reason: Option<String>,  // Disconnection reason
}

/// Generic notification response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotificationResponse {
    pub success: bool,
    pub message: Option<String>,
    pub error_message: Option<String>,
}

/// Authentication response from the external service
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub is_authorized: bool,
    pub rules: Option<Vec<DynamicAclRule>>,
    pub subject: Option<SubjectConfig>,
    pub client_info: Option<ClientInfo>,
    pub error_message: Option<String>,
}

/// Subject configuration from auth service
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SubjectConfig {
    pub subject_id: Option<String>,
    pub interfaces: Option<Vec<String>>,
    pub cert_common_names: Option<Vec<String>>,
    pub usernames: Option<Vec<String>>,
    pub link_protocols: Option<Vec<String>>,
    pub zids: Option<Vec<String>>,
}

/// Client information returned by auth service
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientInfo {
    pub client_id: String,
    pub client_type: Option<String>,
    pub expires_at: Option<u64>,
}

/// Dynamic ACL rule structure matching Zenoh's AclConfigRule
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DynamicAclRule {
    pub id: String,
    pub key_exprs: Vec<String>,
    pub messages: Vec<String>,
    pub flows: Option<Vec<String>>,
    pub permission: String,
}

// ============================================================================
// AuthClient Implementation
// ============================================================================
#[cfg(feature = "dynamic_acl")]
pub struct AuthClient {
    config: DynamicAclConfig,
    client: reqwest::Client,
}

#[cfg(feature = "dynamic_acl")]
impl AuthClient {
    /// Create a new authentication client
    pub fn new(config: DynamicAclConfig) -> ZResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| zenoh_result::zerror!("Failed to create HTTP client: {}", e))?;

        Ok(AuthClient { config, client })
    }

    // ========================================================================
    // Authentication API
    // ========================================================================

    /// Authenticate a client with the external service
    pub async fn authenticate(
        &self,
        client_id: &str,
        _cert_common_name: Option<&str>,
        organizational_unit: Option<&str>,
        _interface: Option<&str>,
    ) -> ZResult<AuthResponse> {
        let request = AuthRequest {
            client_id: client_id.to_string(),
            client_type: organizational_unit.unwrap_or("default").to_string(),
        };

        let response_text = self.request_with_retry("POST", "/auth", &request).await?;
        let auth_response: AuthResponse = serde_json::from_str(&response_text)
            .map_err(|e| zenoh_result::zerror!("Failed to parse authentication response: {}", e))?;

        tracing::debug!(
            "Authentication response: {}",
            serde_json::to_string(&auth_response).unwrap_or_else(|_| "Failed to serialize".to_string())
        );

        Ok(auth_response)
    }

    // ========================================================================
    // Device Lifecycle APIs
    // ========================================================================

    /// Notify authentication service when a device connects
    pub async fn notify_device_connect(
        &self,
        client_id: &str,
        client_type: &str,
        interface: Option<&str>,
        link_protocol: Option<&str>,
    ) -> ZResult<NotificationResponse> {
        let request = DeviceConnectRequest {
            client_id: client_id.to_string(),
            client_type: client_type.to_string(),
            timestamp: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
            interface: interface.map(|s| s.to_string()),
            link_protocol: link_protocol.map(|s| s.to_string()),
        };

        let response_text = self.request_with_retry("POST", "/device/connect", &request).await?;
        let notification_response: NotificationResponse = serde_json::from_str(&response_text)
            .map_err(|e| zenoh_result::zerror!("Failed to parse notification response: {}", e))?;

        tracing::debug!(
            "Device connect notification response: {}",
            serde_json::to_string(&notification_response).unwrap_or_else(|_| "Failed to serialize".to_string())
        );

        Ok(notification_response)
    }

    /// Notify authentication service when a device disconnects
    pub async fn notify_device_disconnect(
        &self,
        client_id: &str,
        client_type: &str,
        reason: Option<&str>,
    ) -> ZResult<NotificationResponse> {
        let request = DeviceDisconnectRequest {
            client_id: client_id.to_string(),
            client_type: client_type.to_string(),
            timestamp: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
            reason: reason.map(|s| s.to_string()),
        };

        let response_text = self.request_with_retry("POST", "/device/disconnect", &request).await?;
        let notification_response: NotificationResponse = serde_json::from_str(&response_text)
            .map_err(|e| zenoh_result::zerror!("Failed to parse notification response: {}", e))?;

        tracing::debug!(
            "Device disconnect notification response: {}",
            serde_json::to_string(&notification_response).unwrap_or_else(|_| "Failed to serialize".to_string())
        );

        Ok(notification_response)
    }

    // ========================================================================
    // Internal Helper Methods
    // ========================================================================

    /// Generic HTTP request method with retry logic
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
                    tracing::debug!(
                        "Request succeeded: {} {} (attempt {})",
                        method,
                        endpoint,
                        attempts
                    );
                    return Ok(response);
                }
                Err(e) => {
                    if attempts >= max_attempts {
                        tracing::error!(
                            "Request failed after {} attempts: {} {} - {}",
                            max_attempts,
                            method,
                            endpoint,
                            e
                        );
                        return Err(e);
                    }
                    tracing::warn!(
                        "Request failed (attempt {}/{}): {} {} - {}",
                        attempts,
                        max_attempts,
                        method,
                        endpoint,
                        e
                    );
                    tokio::time::sleep(Duration::from_millis(
                        self.config.retry_delay_seconds * 1000,
                    ))
                        .await;
                }
            }
        }
    }

    /// Make a single HTTP request without retry
    /// Returns the response body text for parsing
    async fn make_request_once<T>(
        &self,
        method: &str,
        endpoint: &str,
        request_body: &T,
    ) -> ZResult<String>
    where
        T: Serialize + std::fmt::Debug,
    {
        let url = format!("{}{}", self.config.endpoint, endpoint);

        tracing::debug!("Making {} request to {} with body: {:?}", method, url, request_body);

        let request_builder = match method {
            "POST" => self.client.post(&url),
            "PUT" => self.client.put(&url),
            "PATCH" => self.client.patch(&url),
            "GET" => self.client.get(&url),
            "DELETE" => self.client.delete(&url),
            _ => bail!("Unsupported HTTP method: {}", method),
        };

        let response = request_builder
            .json(request_body)
            .send()
            .await
            .map_err(|e| zenoh_result::zerror!("HTTP request failed: {}", e))?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read response body".to_string());

        if !status.is_success() {
            bail!(
                "Authentication service returned error status {}: {}",
                status,
                response_text
            );
        }

        tracing::trace!(
            "Request completed successfully: {} {} - Response: {}",
            method,
            endpoint,
            response_text
        );

        Ok(response_text)
    }
}
