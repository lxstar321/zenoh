//! Dynamic ACL integrator - combines all components

use nonempty_collections::NEVec;
use std::collections::HashMap;
use zenoh_config::{
    AclConfigPolicyEntry, AclConfigRule, AclConfigSubjects, CertCommonName, Interface, Permission,
    Username, ZenohId,
};
use zenoh_result::{bail, ZResult};

#[cfg(feature = "dynamic_acl")]
use crate::net::dynamic_acl::{
    provider::{CertificateInfo, DynamicAclProvider, DynamicAclResponse, SubjectConfig},
    rules::RuleProcessor,
};

/// Rule content for deduplication (ignores ID)
#[cfg(feature = "dynamic_acl")]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RuleContent {
    pub key_exprs: Vec<String>,     // key表达式列表（按顺序比较）
    pub messages: Vec<String>,      // 消息类型列表（按顺序比较）
    pub flows: Option<Vec<String>>, // 流量方向（可选，按顺序比较）
    pub permission: String,         // 权限类型
}

#[cfg(feature = "dynamic_acl")]
impl RuleContent {
    fn from_rule(rule: &AclConfigRule) -> Self {
        Self {
            key_exprs: rule.key_exprs.iter().map(|ke| ke.to_string()).collect(),
            messages: rule.messages.iter().map(|m| format!("{:?}", m)).collect(),
            flows: rule
                .flows
                .as_ref()
                .map(|f| f.iter().map(|flow| format!("{:?}", flow)).collect()),
            permission: format!("{:?}", rule.permission),
        }
    }
}

/// Dynamic ACL integrator - coordinates all components
#[cfg(feature = "dynamic_acl")]
pub struct DynamicAclIntegrator {
    rule_processor: RuleProcessor,
    global_rule_registry: HashMap<RuleContent, String>,
}

#[cfg(feature = "dynamic_acl")]
impl DynamicAclIntegrator {
    /// Create a new integrator
    pub fn new() -> Self {
        Self {
            rule_processor: RuleProcessor::new(),
            global_rule_registry: HashMap::new(),
        }
    }

    /// Integrate ACL configuration from authentication response
    pub fn integrate_acl_config(
        &mut self,
        client_id: &str,
        response: &DynamicAclResponse,
    ) -> ZResult<(
        Vec<AclConfigRule>,
        Vec<AclConfigSubjects>,
        Vec<AclConfigPolicyEntry>,
    )> {
        if !response.is_authorized {
            bail!(
                "Client {} is not authorized: {}",
                client_id,
                response.error_message.as_deref().unwrap_or("Unknown error")
            );
        }

        match &response.rules {
            Some(rules) => {
                // 1. 智能去重：相同规则内容复用，不同规则分配唯一ID
                let processed_rules = self.deduplicate_and_assign_rule_ids(client_id, rules)?;

                // 2. 自动生成对应的subject
                let subject =
                    self.generate_subject_for_client(client_id, &response.subject_config)?;

                // 3. 为客户端生成一个policy - 统一管理所有权限规则
                let policy =
                    self.generate_policy_for_client(client_id, &processed_rules, &subject)?;

                Ok((processed_rules, vec![subject], vec![policy]))
            }
            None => {
                // 如果没有提供rules，返回空配置（客户端无权限）
                Ok((vec![], vec![], vec![]))
            }
        }
    }

    /// Smart deduplication and ID assignment
    fn deduplicate_and_assign_rule_ids(
        &mut self,
        client_id: &str,
        rules: &[AclConfigRule],
    ) -> ZResult<Vec<AclConfigRule>> {
        let mut result = Vec::new();
        let mut client_rule_ids = std::collections::HashSet::new();

        for rule in rules {
            let rule_content = RuleContent::from_rule(rule);

            // 1. Check global rule registry: does identical content already exist?
            let assigned_id =
                if let Some(existing_id) = self.global_rule_registry.get(&rule_content) {
                    // Rule content identical, reuse existing ID
                    tracing::debug!(
                        "Rule content identical to existing rule '{}', reusing ID",
                        existing_id
                    );
                    existing_id.clone()
                } else {
                    // Rule content different, need new ID
                    let mut candidate_id = rule.id.clone();

                    // Ensure ID is unique within client scope
                    if !client_rule_ids.insert(candidate_id.clone()) {
                        candidate_id = format!("{}-{}", client_id, rule.id);
                        tracing::warn!(
                            "Rule ID '{}' already used by client '{}', renamed to '{}'",
                            rule.id,
                            client_id,
                            candidate_id
                        );
                    }

                    // Register in global rule table
                    self.global_rule_registry
                        .insert(rule_content, candidate_id.clone());
                    candidate_id
                };

            // Create rule with assigned ID but same content
            let new_rule = AclConfigRule {
                id: assigned_id,
                key_exprs: rule.key_exprs.clone(),
                messages: rule.messages.clone(),
                flows: rule.flows.clone(),
                permission: rule.permission,
            };

            result.push(new_rule);
        }

        Ok(result)
    }

    /// Generate subject for client
    pub fn generate_subject_for_client(
        &self,
        client_id: &str,
        subject_config: &Option<SubjectConfig>,
    ) -> ZResult<AclConfigSubjects> {
        let mut subject = AclConfigSubjects {
            id: format!("subject-{}", client_id),
            interfaces: None,
            cert_common_names: Some(NEVec::new(CertCommonName(client_id.to_string()))), // Default to client_id as CN
            usernames: None,
            link_protocols: Some(NEVec::new(zenoh_config::InterceptorLink::Tls)), // Default to TLS
            zids: None,
        };

        // Override with subject_config if provided
        if let Some(config) = subject_config {
            if let Some(interfaces) = &config.interfaces {
                let interface_list: Vec<Interface> =
                    interfaces.iter().map(|s| Interface(s.clone())).collect();
                if !interface_list.is_empty() {
                    let mut nevec = NEVec::new(interface_list[0].clone());
                    for item in &interface_list[1..] {
                        nevec.push(item.clone());
                    }
                    subject.interfaces = Some(nevec);
                }
            }
            if let Some(usernames) = &config.usernames {
                let username_list: Vec<Username> =
                    usernames.iter().map(|s| Username(s.clone())).collect();
                if !username_list.is_empty() {
                    let mut nevec = NEVec::new(username_list[0].clone());
                    for item in &username_list[1..] {
                        nevec.push(item.clone());
                    }
                    subject.usernames = Some(nevec);
                }
            }
            if let Some(link_protocols) = &config.link_protocols {
                let link_protocol_list: Vec<zenoh_config::InterceptorLink> = link_protocols
                    .iter()
                    .filter_map(|s| match s.as_str() {
                        "tls" => Some(zenoh_config::InterceptorLink::Tls),
                        "tcp" => Some(zenoh_config::InterceptorLink::Tcp),
                        _ => None,
                    })
                    .collect();
                if !link_protocol_list.is_empty() {
                    let mut nevec = NEVec::new(link_protocol_list[0].clone());
                    for item in &link_protocol_list[1..] {
                        nevec.push(item.clone());
                    }
                    subject.link_protocols = Some(nevec);
                }
            }
            if let Some(zids) = &config.zids {
                let zid_list: Vec<ZenohId> = zids.iter().filter_map(|s| s.parse().ok()).collect();
                if !zid_list.is_empty() {
                    let mut nevec = NEVec::new(zid_list[0].clone());
                    for item in &zid_list[1..] {
                        nevec.push(item.clone());
                    }
                    subject.zids = Some(nevec);
                }
            }
        }

        Ok(subject)
    }

    /// Generate policy for client - unified management of all rules
    pub fn generate_policy_for_client(
        &self,
        client_id: &str,
        rules: &[AclConfigRule],
        subject: &AclConfigSubjects,
    ) -> ZResult<AclConfigPolicyEntry> {
        let rule_ids: Vec<String> = rules.iter().map(|r| r.id.clone()).collect();

        Ok(AclConfigPolicyEntry {
            id: Some(format!("policy-{}", client_id)),
            rules: rule_ids,
            subjects: vec![subject.id.clone()],
        })
    }
}
