//! Dynamic ACL rule processing

use std::collections::HashMap;
use nonempty_collections::NEVec;
use zenoh_config::{AclConfigRule, AclMessage, InterceptorFlow, Permission};
use zenoh_keyexpr::OwnedKeyExpr;
use zenoh_result::{bail, ZResult};

#[cfg(feature = "dynamic_acl")]
pub use crate::net::dynamic_acl::client::{AuthResponse, DynamicAclRule};

/// Rule processing errors
#[derive(Debug)]
pub enum RuleProcessingError {
    InvalidKeyExpr(String),
    UnknownMessageType(String),
    UnknownFlowType(String),
    UnknownPermission(String),
    EmptyKeyExpressions,
    EmptyMessages,
}

/// Rule processor for dynamic ACL
#[cfg(feature = "dynamic_acl")]
pub struct RuleProcessor {
    // Cache for converted key expressions to avoid repeated parsing
    keyexpr_cache: HashMap<String, OwnedKeyExpr>,
}

#[cfg(feature = "dynamic_acl")]
impl RuleProcessor {
    /// Create a new rule processor
    pub fn new() -> Self {
        Self {
            keyexpr_cache: HashMap::new(),
        }
    }

    /// Process authentication response and convert to Zenoh ACL rules
    pub fn process_auth_response(&mut self, response: AuthResponse) -> ZResult<Vec<AclConfigRule>> {
        if !response.is_authorized {
            bail!("Client is not authorized: {}", response.error_message.unwrap_or_default());
        }

        let mut rules = Vec::new();
        if let Some(dynamic_rules) = &response.rules {
            for dynamic_rule in dynamic_rules {
                let rule = self.convert_dynamic_rule(dynamic_rule.clone())?;
                rules.push(rule);
            }
        }

        Ok(rules)
    }

    /// Convert dynamic rule to Zenoh ACL rule
    fn convert_dynamic_rule(&mut self, dynamic_rule: DynamicAclRule) -> ZResult<AclConfigRule> {
        // Validate and convert key expressions
        if dynamic_rule.key_exprs.is_empty() {
            bail!("Empty key expressions list");
        }

        let mut key_exprs = Vec::new();
        for key_expr_str in dynamic_rule.key_exprs {
            let key_expr = self.parse_key_expr(&key_expr_str)?;
            key_exprs.push(key_expr);
        }

        // Validate and convert messages
        if dynamic_rule.messages.is_empty() {
            bail!("Empty messages list");
        }

        let mut messages = Vec::new();
        for msg_str in dynamic_rule.messages {
            let message = self.parse_message_type(&msg_str)?;
            messages.push(message);
        }

        // Convert flows (optional)
        let flows = if let Some(flow_list) = dynamic_rule.flows {
            if flow_list.is_empty() {
                None
            } else {
                let mut converted_flows = Vec::new();
                for flow_str in flow_list {
                    let flow = self.parse_flow_type(&flow_str)?;
                    converted_flows.push(flow);
                }
                // Convert to NEVec: take first element and rest
                let (first, rest) = converted_flows.split_first()
                    .expect("Flow list should not be empty after check");
                Some(NEVec::from((first.clone(), rest.to_vec())))
            }
        } else {
            None
        };

        // Convert permission
        let permission = self.parse_permission(&dynamic_rule.permission)?;

        // Convert to NEVec: take first element and rest
        let (first_keyexpr, rest_keyexprs) = key_exprs.split_first()
            .expect("Key expressions list should not be empty after check");
        let (first_message, rest_messages) = messages.split_first()
            .expect("Messages list should not be empty after check");

        Ok(AclConfigRule {
            id: dynamic_rule.id,
            key_exprs: NEVec::from((first_keyexpr.clone(), rest_keyexprs.to_vec())),
            messages: NEVec::from((first_message.clone(), rest_messages.to_vec())),
            flows,
            permission,
        })
    }

    /// Parse key expression string to OwnedKeyExpr
    fn parse_key_expr(&mut self, key_expr_str: &str) -> ZResult<OwnedKeyExpr> {
        // Check cache first
        if let Some(cached) = self.keyexpr_cache.get(key_expr_str) {
            return Ok(cached.clone());
        }

        // Parse and validate key expression
        match OwnedKeyExpr::try_from(key_expr_str) {
            Ok(key_expr) => {
                // Cache the result
                self.keyexpr_cache.insert(key_expr_str.to_string(), key_expr.clone());
                Ok(key_expr)
            }
            Err(e) => Err(zenoh_result::zerror!(
                "Invalid key expression '{}': {}",
                key_expr_str, e
            ).into()),
        }
    }

    /// Parse message type string to AclMessage
    fn parse_message_type(&self, msg_str: &str) -> ZResult<AclMessage> {
        match msg_str.to_lowercase().as_str() {
            "put" => Ok(AclMessage::Put),
            "delete" => Ok(AclMessage::Delete),
            "query" => Ok(AclMessage::Query),
            "declare_subscriber" => Ok(AclMessage::DeclareSubscriber),
            "declare_queryable" => Ok(AclMessage::DeclareQueryable),
            "reply" => Ok(AclMessage::Reply),
            "liveliness_token" => Ok(AclMessage::LivelinessToken),
            "declare_liveliness_subscriber" => Ok(AclMessage::DeclareLivelinessSubscriber),
            "liveliness_query" => Ok(AclMessage::LivelinessQuery),
            _ => Err(zenoh_result::zerror!("Unknown message type: {}", msg_str).into()),
        }
    }

    /// Parse flow type string to InterceptorFlow
    fn parse_flow_type(&self, flow_str: &str) -> ZResult<InterceptorFlow> {
        match flow_str.to_lowercase().as_str() {
            "ingress" => Ok(InterceptorFlow::Ingress),
            "egress" => Ok(InterceptorFlow::Egress),
            _ => Err(zenoh_result::zerror!("Unknown flow type: {}", flow_str).into()),
        }
    }

    /// Parse permission string to Permission
    fn parse_permission(&self, perm_str: &str) -> ZResult<Permission> {
        match perm_str.to_lowercase().as_str() {
            "allow" => Ok(Permission::Allow),
            "deny" => Ok(Permission::Deny),
            _ => Err(zenoh_result::zerror!("Unknown permission type: {}", perm_str).into()),
        }
    }

    /// Validate a rule for common issues
    pub fn validate_rule(&self, _rule: &AclConfigRule) -> ZResult<()> {
        // NEVec is guaranteed to be non-empty, so basic validation is already done
        // Additional validation can be added here
        Ok(())
    }
}
