//
// Copyright (c) 2024 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)

use std::{any::Any, collections::HashSet, iter, sync::Arc};

use itertools::Itertools;
use serde_json;
use zenoh_config::{
    AclConfig, AclConfigPolicyEntry, AclConfigRule, AclConfigSubjects, AclMessage, CertCommonName,
    InterceptorFlow, Interface, Permission, QueryStrategy, Username, ZenohId,
};
use zenoh_keyexpr::keyexpr;
use zenoh_keyexpr::OwnedKeyExpr;
use zenoh_link::LinkAuthId;
use zenoh_protocol::{
    core::ZenohIdProto,
    network::{
        interest::InterestMode, Declare, DeclareBody, Interest, NetworkBodyMut, NetworkMessageMut,
        Push, Request, Response,
    },
    zenoh::{PushBody, RequestBody},
};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast, unicast::TransportUnicast, TransportPeerEventHandler,
};

use super::{
    authorization::PolicyEnforcer, EgressInterceptor, IngressInterceptor, InterceptorFactory,
    InterceptorFactoryTrait, InterceptorLinkWrapper, InterceptorTrait,
};
use crate::{
    key_expr::KeyExpr,
    net::routing::interceptor::{authorization::SubjectQuery, InterceptorContext},
};

#[cfg(feature = "dynamic_acl")]
use crate::net::dynamic_acl::{
    client::{AuthClient, AuthResponse},
    integrator::DynamicAclIntegrator,
    provider::{CertificateInfo, DynamicAclResponse},
};

pub struct AclEnforcer {
    enforcer: Arc<PolicyEnforcer>,
    #[cfg(feature = "dynamic_acl")]
    dynamic_acl_config: Option<zenoh_config::DynamicAclConfig>,
    #[cfg(feature = "dynamic_acl")]
    auth_client: Option<AuthClient>,
    #[cfg(feature = "dynamic_acl")]
    integrator: Option<DynamicAclIntegrator>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthSubject {
    id: usize,
    name: String,
}

struct EgressAclEnforcer {
    policy_enforcer: Arc<PolicyEnforcer>,
    subject: Vec<AuthSubject>,
    zid: ZenohIdProto,
}

impl EgressAclEnforcer {
    #[inline]
    fn cached_result_or_action(
        &self,
        cached_permission: Option<Permission>,
        action: AclMessage,
        log_msg: &str,
        key_expr: KeyExpr,
    ) -> Permission {
        match cached_permission {
            Some(p) => {
                match p {
                    Permission::Allow => tracing::trace!(
                        "Using cached result: {} is authorized to {} on {}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                    Permission::Deny => tracing::trace!(
                        "Using cached result: {} is unauthorized to {} on {}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                }
                p
            }
            None => self.action(action, log_msg, &key_expr),
        }
    }
}

struct IngressAclEnforcer {
    policy_enforcer: Arc<PolicyEnforcer>,
    subject: Vec<AuthSubject>,
    zid: ZenohIdProto,
}

impl IngressAclEnforcer {
    #[inline]
    fn cached_result_or_action(
        &self,
        cached_permission: Option<Permission>,
        action: AclMessage,
        log_msg: &str,
        key_expr: KeyExpr,
    ) -> Permission {
        match cached_permission {
            Some(p) => {
                match p {
                    Permission::Allow => tracing::trace!(
                        "Using cached result: {} is authorized to {} on {}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                    Permission::Deny => tracing::trace!(
                        "Using cached result: {} is unauthorized to {} on {}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                }
                p
            }
            None => self.action(action, log_msg, &key_expr),
        }
    }

    #[inline]
    fn cached_result_or_action_undecl(
        &self,
        cached_permission: Option<Permission>,
        action: AclMessage,
        log_msg: &str,
        key_expr: Option<KeyExpr>,
    ) -> Permission {
        match cached_permission {
            Some(p) => {
                match p {
                    Permission::Allow => tracing::trace!(
                        "Using cached result: {} is authorized to {} on {:?}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                    Permission::Deny => tracing::trace!(
                        "Using cached result: {} is unauthorized to {} on {:?}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                }
                p
            }
            None => {
                // Undeclarations in ingress are only filtered if the ext_wire_expr is set.
                // If it's not set, we let the undeclaration pass, it will be rejected by the routing logic
                // if its associated declaration was denied.
                match key_expr {
                    Some(ke) => self.action(action, log_msg, &ke),
                    None => Permission::Allow,
                }
            }
        }
    }
}

pub(crate) fn acl_interceptor_factories(
    acl_config: &AclConfig,
) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];

    if acl_config.enabled {
        let mut policy_enforcer = PolicyEnforcer::new();
        match policy_enforcer.init(acl_config) {
            Ok(_) => {
                tracing::debug!("Access control is enabled");
                let mut acl_enforcer = AclEnforcer {
                    enforcer: Arc::new(policy_enforcer),
                    #[cfg(feature = "dynamic_acl")]
                    dynamic_acl_config: acl_config.dynamic_config.clone(),
                    #[cfg(feature = "dynamic_acl")]
                    auth_client: None,
                    #[cfg(feature = "dynamic_acl")]
                    integrator: None,
                };

                // Store dynamic ACL config for lazy initialization
                #[cfg(feature = "dynamic_acl")]
                if let Some(query_strategy) = &acl_config.query_strategy {
                    if *query_strategy == QueryStrategy::Dynamic {
                        if let Some(dynamic_config) = &acl_config.dynamic_config {
                            tracing::debug!("Query strategy is dynamic - storing config for lazy initialization");
                            // Don't create AuthClient here - defer to connection time
                            acl_enforcer.dynamic_acl_config = Some(dynamic_config.clone());
                            // Create integrator for dynamic ACL rule processing
                            acl_enforcer.integrator = Some(DynamicAclIntegrator::new());
                            tracing::info!(
                                "Dynamic ACL config stored - will initialize on first connection"
                            );
                        } else {
                            tracing::warn!(
                                "Query strategy is dynamic but no dynamic_config provided"
                            );
                        }
                    }
                }

                res.push(Box::new(acl_enforcer));
            }
            Err(e) => bail!("Access control not enabled due to: {}", e),
        }
    } else {
        tracing::debug!("Access control is disabled");
    }

    Ok(res)
}

#[cfg(feature = "dynamic_acl")]
fn initialize_dynamic_acl_components(
    config: &zenoh_config::DynamicAclConfig,
) -> ZResult<(AuthClient, DynamicAclIntegrator)> {
    use crate::net::dynamic_acl::{client::AuthClient, integrator::DynamicAclIntegrator};

    let client = AuthClient::new(config.clone())
        .map_err(|e| zenoh_result::zerror!("Failed to create auth client: {}", e))?;

    let integrator = DynamicAclIntegrator::new();

    Ok((client, integrator))
}

impl AclEnforcer {
    /// Create interceptors for static ACL configuration
    fn new_static_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        let auth_ids = match transport.get_auth_ids() {
            Ok(auth_ids) => auth_ids,
            Err(err) => {
                tracing::error!("Couldn't get Transport Auth IDs: {}", err);
                return (None, None);
            }
        };

        let mut cert_common_names = Vec::new();
        let mut link_protocols = Vec::new();
        let username = auth_ids.username().cloned().map(Username);
        let zid: ZenohId = (*auth_ids.zid()).into();

        for auth_id in auth_ids.link_auth_ids() {
            match auth_id {
                LinkAuthId::Tls(value) => {
                    cert_common_names.push(value.as_ref().map(|v| CertCommonName(v.clone())));
                }
                LinkAuthId::Quic(value) => {
                    cert_common_names.push(value.as_ref().map(|v| CertCommonName(v.clone())));
                }
                _ => {}
            }
            link_protocols.push(Some(InterceptorLinkWrapper::from(auth_id).0));
        }
        if cert_common_names.is_empty() {
            cert_common_names.push(None);
        }

        let links = match transport.get_links() {
            Ok(links) => links,
            Err(err) => {
                tracing::error!("Couldn't get Transport links: {}", err);
                return (None, None);
            }
        };
        let mut interfaces = links
            .into_iter()
            .flat_map(|link| {
                link.interfaces
                    .into_iter()
                    .map(|interface| Some(Interface(interface)))
            })
            .collect::<Vec<_>>();
        if interfaces.is_empty() {
            interfaces.push(None);
        } else if interfaces.len() > 1 {
            tracing::warn!("Transport returned multiple network interfaces, current ACL logic might incorrectly apply filters in this case!");
        }

        let mut auth_subjects = HashSet::new();

        for ((((username, interface), cert_common_name), link_protocol), zid) in
            iter::once(username)
                .cartesian_product(interfaces.into_iter())
                .cartesian_product(cert_common_names.into_iter())
                .cartesian_product(link_protocols.into_iter())
                .cartesian_product(iter::once(Some(zid)))
        {
            let query = SubjectQuery {
                interface,
                cert_common_name,
                username,
                link_protocol,
                zid,
            };

            for entry in self.enforcer.subject_store.query(&query) {
                auth_subjects.insert(AuthSubject {
                    id: entry.id,
                    name: format!("{query}"),
                });
            }
        }

        let zid = match transport.get_zid() {
            Ok(zid) => zid,
            Err(err) => {
                tracing::error!("Couldn't get Transport zid: {}", err);
                return (None, None);
            }
        };
        // FIXME: Investigate if `AuthSubject` can have duplicates above and try to avoid this conversion
        let auth_subjects = auth_subjects.into_iter().collect::<Vec<AuthSubject>>();
        if auth_subjects.is_empty() {
            tracing::info!(
                "{zid} did not match any configured ACL subject. Default permission `{:?}` will be applied on all messages",
                self.enforcer.default_permission
            );
        }
        let ingress_interceptor = Box::new(IngressAclEnforcer {
            policy_enforcer: self.enforcer.clone(),
            zid,
            subject: auth_subjects.clone(),
        });
        let egress_interceptor = Box::new(EgressAclEnforcer {
            policy_enforcer: self.enforcer.clone(),
            zid,
            subject: auth_subjects,
        });
        (
            self.enforcer
                .interface_enabled
                .ingress
                .then_some(ingress_interceptor),
            self.enforcer
                .interface_enabled
                .egress
                .then_some(egress_interceptor),
        )
    }

    #[cfg(feature = "dynamic_acl")]
    /// Create interceptors for dynamic ACL configuration
    fn new_dynamic_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        // Extract certificate information from transport links
        let links = match transport.get_links() {
            Ok(links) => links,
            Err(err) => {
                tracing::error!("Couldn't get Transport links: {}", err);
                return self.new_static_transport_unicast(transport);
            }
        };

        let cert_info = match links
            .into_iter()
            .find_map(|link| CertificateInfo::from_link(&link))
        {
            Some(info) => info,
            None => {
                tracing::warn!("Could not extract certificate information from transport links, falling back to static ACL");
                return self.new_static_transport_unicast(transport);
            }
        };

        // Authenticate client and get dynamic ACL rules
        tracing::info!(
            "Starting dynamic ACL authentication for client: {} (type: {:?})",
            cert_info.common_name,
            cert_info.organizational_unit
        );
        let dynamic_acl_result = match self.authenticate_client(&cert_info) {
            Ok(result) => {
                tracing::info!(
                    "Dynamic ACL authentication response received for client: {}",
                    cert_info.common_name
                );
                tracing::info!("  Authorized: {}", result.is_authorized);
                if let Some(ref error) = result.error_message {
                    tracing::info!("  Error message: {}", error);
                }
                if let Some(ref rules) = result.rules {
                    tracing::info!("  Number of rules: {}", rules.len());
                    for rule in rules {
                        tracing::info!(
                            "    Rule '{}': key_exprs={:?}, messages={:?}, permission={:?}",
                            rule.id,
                            rule.key_exprs,
                            rule.messages,
                            rule.permission
                        );
                    }
                } else {
                    tracing::info!("  No rules provided");
                }
                result
            }
            Err(e) => {
                tracing::error!("Client authentication failed: {}", e);
                return self.new_static_transport_unicast(transport);
            }
        };

        if !dynamic_acl_result.is_authorized {
            tracing::warn!(
                "Client not authorized: {:?}",
                dynamic_acl_result.error_message
            );
            return self.new_static_transport_unicast(transport);
        }

        let rules = match dynamic_acl_result.rules {
            Some(rules) => rules,
            None => {
                tracing::warn!(
                    "No rules provided for authorized client, falling back to static ACL"
                );
                return self.new_static_transport_unicast(transport);
            }
        };

        // Create dynamic subject for this client
        use nonempty_collections::NEVec;
        let dynamic_subject = AclConfigSubjects {
            id: format!("dynamic-subject-{}", cert_info.common_name),
            cert_common_names: Some(NEVec::new(CertCommonName(cert_info.common_name.clone()))),
            interfaces: cert_info
                .interface
                .as_ref()
                .map(|i| NEVec::new(Interface(i.clone()))),
            usernames: None,
            link_protocols: Some(NEVec::new(cert_info.link_protocol.clone())),
            zids: Some(NEVec::new(cert_info.zid)),
        };

        // Create policy for this client
        let dynamic_policy = AclConfigPolicyEntry {
            id: Some(format!("dynamic-policy-{}", cert_info.common_name)),
            rules: rules.iter().map(|r| r.id.clone()).collect(),
            subjects: vec![dynamic_subject.id.clone()],
        };

        tracing::info!("Created dynamic ACL policy for cert_info: {:?}", cert_info);
        tracing::info!("Policy ID: {:?}", dynamic_policy.id);
        tracing::info!("Policy rules: {:?}", dynamic_policy.rules);
        tracing::info!("Policy subjects: {:?}", dynamic_policy.subjects);

        let zid = match transport.get_zid() {
            Ok(zid) => zid,
            Err(err) => {
                tracing::error!("Couldn't get Transport zid: {}", err);
                return (None, None);
            }
        };

        // Create separate PolicyEnforcer instances for ingress and egress
        // Initialize both with the same dynamic ACL configuration
        let mut ingress_policy_enforcer = PolicyEnforcer::new();
        let mut egress_policy_enforcer = PolicyEnforcer::new();

        // Initialize ingress policy enforcer first to get subject ID
        let acl_config = AclConfig {
            enabled: true,
            default_permission: self.enforcer.default_permission,
            rules: Some(rules.clone()),
            subjects: Some(vec![dynamic_subject.clone()]),
            policies: Some(vec![dynamic_policy.clone()]),
            query_strategy: None,
            dynamic_config: None,
        };

        if let Err(e) = ingress_policy_enforcer.init(&acl_config) {
            tracing::error!("Failed to initialize ingress policy enforcer: {}", e);
            return self.new_static_transport_unicast(transport);
        }

        // Create dynamic subject for the interceptor
        // Find the subject ID from the initialized policy enforcer
        let subject_query = SubjectQuery {
            interface: None,
            cert_common_name: Some(CertCommonName(cert_info.common_name.clone())),
            username: None,
            link_protocol: Some(cert_info.link_protocol),
            zid: Some(cert_info.zid),
        };

        let auth_subjects = match ingress_policy_enforcer
            .subject_store
            .query(&subject_query)
            .next()
        {
            Some(entry) => {
                tracing::info!(
                    "Found subject ID {} for dynamic client: {}",
                    entry.id,
                    cert_info.common_name
                );
                vec![AuthSubject {
                    id: entry.id,
                    name: format!("dynamic-client-{}", cert_info.common_name),
                }]
            }
            None => {
                tracing::error!("Failed to find subject ID for dynamic client: {}. Query: cert_common_name={:?}, link_protocol={:?}, zid={:?}",
                    cert_info.common_name, subject_query.cert_common_name, subject_query.link_protocol, subject_query.zid);
                tracing::error!("Subject store may not have been initialized correctly or query parameters don't match");
                return self.new_static_transport_unicast(transport);
            }
        };

        let acl_config_egress = AclConfig {
            enabled: true,
            default_permission: self.enforcer.default_permission,
            rules: Some(rules),
            subjects: Some(vec![dynamic_subject]),
            policies: Some(vec![dynamic_policy]),
            query_strategy: None,
            dynamic_config: None,
        };

        if let Err(e) = egress_policy_enforcer.init(&acl_config_egress) {
            tracing::error!("Failed to initialize egress policy enforcer: {}", e);
            return self.new_static_transport_unicast(transport);
        }

        let ingress_policy_enforcer = Arc::new(ingress_policy_enforcer);
        let egress_policy_enforcer = Arc::new(egress_policy_enforcer);

        let ingress_interceptor = Box::new(IngressAclEnforcer {
            policy_enforcer: ingress_policy_enforcer,
            zid,
            subject: auth_subjects.clone(),
        });

        let egress_interceptor = Box::new(EgressAclEnforcer {
            policy_enforcer: egress_policy_enforcer,
            zid,
            subject: auth_subjects,
        });

        (
            self.enforcer
                .interface_enabled
                .ingress
                .then_some(ingress_interceptor),
            self.enforcer
                .interface_enabled
                .egress
                .then_some(egress_interceptor),
        )
    }

    #[cfg(feature = "dynamic_acl")]
    /// Authenticate client with external service
    fn authenticate_client(
        &self,
        cert_info: &CertificateInfo,
    ) -> ZResult<crate::net::dynamic_acl::provider::DynamicAclResponse> {
        let auth_client = self
            .auth_client
            .as_ref()
            .ok_or_else(|| zenoh_result::zerror!("Auth client not initialized"))?;

        // Call authentication service
        let response = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                auth_client
                    .authenticate(
                        &cert_info.common_name,
                        Some(&cert_info.common_name),
                        cert_info.organizational_unit.as_deref(),
                        cert_info.interface.as_deref(),
                    )
                    .await
            })
        })
        .map_err(|e| zenoh_result::zerror!("Authentication service call failed: {}", e))?;

        // Convert AuthResponse to DynamicAclResponse
        // Convert DynamicAclRule to AclConfigRule
        let acl_rules: Vec<AclConfigRule> = if let Some(rules) = &response.rules {
            rules
                .iter()
                .map(|rule| AclConfigRule {
                    id: rule.id.clone(),
                    key_exprs: rule
                        .key_exprs
                        .clone()
                        .into_iter()
                        .map(|s| s.parse().unwrap())
                        .collect::<Vec<_>>()
                        .try_into()
                        .unwrap(),
                    messages: rule
                        .messages
                        .clone()
                        .into_iter()
                        .map(|s| match s.as_str() {
                            "put" => AclMessage::Put,
                            "delete" => AclMessage::Delete,
                            "declare_subscriber" => AclMessage::DeclareSubscriber,
                            "declare_queryable" => AclMessage::DeclareQueryable,
                            "query" => AclMessage::Query,
                            _ => AclMessage::Put, // default
                        })
                        .collect::<Vec<_>>()
                        .try_into()
                        .unwrap(),
                    flows: rule.flows.clone().map(|flows| {
                        flows
                            .into_iter()
                            .map(|s| match s.as_str() {
                                "ingress" => InterceptorFlow::Ingress,
                                "egress" => InterceptorFlow::Egress,
                                _ => InterceptorFlow::Ingress, // default
                            })
                            .collect::<Vec<_>>()
                            .try_into()
                            .unwrap()
                    }),
                    permission: match rule.permission.as_str() {
                        "allow" => Permission::Allow,
                        "deny" => Permission::Deny,
                        _ => Permission::Deny, // default
                    },
                })
                .collect()
        } else {
            vec![]
        };

        // Convert client_info to serde_json::Value
        let client_info_value = response.client_info.map(|info| {
            serde_json::json!({
                "client_id": info.client_id,
                "client_type": info.client_type,
                "expires_at": info.expires_at
            })
        });

        let dynamic_response = DynamicAclResponse {
            is_authorized: response.is_authorized,
            rules: Some(acl_rules),
            subject_config: None, // AuthResponse doesn't have subject_config
            client_info: client_info_value,
            error_message: response.error_message,
        };

        Ok(dynamic_response)
    }

    fn new_transport_multicast(
        &self,
        _transport: &TransportMulticast,
    ) -> Option<EgressInterceptor> {
        tracing::debug!("Transport Multicast is disabled in interceptor");
        None
    }

    fn new_peer_multicast(&self, _transport: &TransportMulticast) -> Option<IngressInterceptor> {
        tracing::debug!("Peer Multicast is disabled in interceptor");
        None
    }
}

impl InterceptorFactoryTrait for AclEnforcer {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        // Check if dynamic ACL is enabled and should be used
        #[cfg(feature = "dynamic_acl")]
        if let Some(dynamic_config) = &self.dynamic_acl_config {
            // Extract certificate information from transport
            let cert_info = match transport.get_links() {
                Ok(links) => {
                    if let Some(link) = links.get(0) {
                        match CertificateInfo::from_link(link) {
                            Some(info) => info,
                            None => {
                                tracing::error!("Failed to extract certificate info from link");
                                return (None, None);
                            }
                        }
                    } else {
                        tracing::error!("No links found in transport");
                        return (None, None);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to get transport links: {}", e);
                    return (None, None);
                }
            };

            tracing::info!(
                "Starting dynamic ACL authentication for client: {}",
                cert_info.common_name
            );

            // Create AuthClient for this connection
            let auth_client = match AuthClient::new(dynamic_config.clone()) {
                Ok(client) => client,
                Err(e) => {
                    tracing::error!("Failed to create AuthClient: {}", e);
                    return (None, None);
                }
            };

            tracing::info!("luoluo Perform authentication");
            // Perform authentication
            let dynamic_acl_result = match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    // We're in an async context, use spawn_blocking to run the async authentication
                    tokio::task::block_in_place(|| {
                        handle.block_on(async {
                            auth_client
                                .authenticate(
                                    &cert_info.common_name,
                                    Some(&cert_info.common_name), // Use common_name as cert_common_name
                                    cert_info.organizational_unit.as_deref(),
                                    cert_info.interface.as_deref(),
                                )
                                .await
                        })
                    })
                }
                Err(_) => {
                    // Not in async context, create a new runtime
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                        auth_client
                            .authenticate(
                                &cert_info.common_name,
                                Some(&cert_info.common_name), // Use common_name as cert_common_name
                                cert_info.organizational_unit.as_deref(),
                                cert_info.interface.as_deref(),
                            )
                            .await
                    })
                }
            };
            let dynamic_acl_result = match dynamic_acl_result {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("Dynamic ACL authentication failed: {}", e);
                    return (None, None);
                }
            };

            if !dynamic_acl_result.is_authorized {
                tracing::warn!(
                    "Client {} is not authorized by dynamic ACL",
                    cert_info.common_name
                );
                return (None, None);
            }

            tracing::info!(
                "Dynamic ACL authentication response received for client: {}",
                cert_info.common_name
            );
            tracing::info!("Authorized: {}", dynamic_acl_result.is_authorized);
            tracing::info!(
                "Number of rules: {}",
                dynamic_acl_result.rules.as_ref().map_or(0, |r| r.len())
            );
            if let Some(rules_vec) = &dynamic_acl_result.rules {
                for rule in rules_vec {
                    tracing::info!(
                        "Rule '{:?}': key_exprs={:?}, messages={:?}, permission={:?}",
                        rule.id,
                        rule.key_exprs,
                        rule.messages,
                        rule.permission
                    );
                }
            }

            // Convert client::SubjectConfig to provider::SubjectConfig
            let provider_subject_config: crate::net::dynamic_acl::provider::SubjectConfig =
                dynamic_acl_result.subject.as_ref().unwrap().clone().into();

            // Create dynamic subject
            let dynamic_subject = match self
                .integrator
                .as_ref()
                .unwrap()
                .generate_subject_for_client(&cert_info.common_name, &Some(provider_subject_config))
            {
                Ok(subject) => subject,
                Err(e) => {
                    tracing::error!("Failed to generate dynamic subject: {}", e);
                    return (None, None);
                }
            };

            // Convert DynamicAclRule to AclConfigRule for policy generation
            let acl_rules: Vec<AclConfigRule> = dynamic_acl_result
                .rules
                .as_ref()
                .unwrap_or(&vec![])
                .iter()
                .map(|rule| {
                    // This conversion should match what we do in the conversion function
                    // For now, create a minimal conversion
                    AclConfigRule {
                        id: rule.id.clone(),
                        key_exprs: rule
                            .key_exprs
                            .iter()
                            .map(|s| OwnedKeyExpr::new(s.as_str()).unwrap())
                            .collect::<Vec<_>>()
                            .try_into()
                            .unwrap(),
                        messages: rule
                            .messages
                            .iter()
                            .map(|s| match s.as_str() {
                                "put" => AclMessage::Put,
                                "query" => AclMessage::Query,
                                "declare_subscriber" => AclMessage::DeclareSubscriber,
                                "declare_queryable" => AclMessage::DeclareQueryable,
                                _ => AclMessage::Put,
                            })
                            .collect::<Vec<_>>()
                            .try_into()
                            .unwrap(),
                        flows: rule.flows.as_ref().map(|flows| {
                            flows
                                .iter()
                                .map(|s| match s.as_str() {
                                    "ingress" => InterceptorFlow::Ingress,
                                    "egress" => InterceptorFlow::Egress,
                                    _ => InterceptorFlow::Ingress,
                                })
                                .collect::<Vec<_>>()
                                .try_into()
                                .unwrap()
                        }),
                        permission: match rule.permission.as_str() {
                            "allow" => Permission::Allow,
                            "deny" => Permission::Deny,
                            _ => Permission::Deny,
                        },
                    }
                })
                .collect();

            // Create dynamic policy
            let dynamic_policy = match self
                .integrator
                .as_ref()
                .unwrap()
                .generate_policy_for_client(&cert_info.common_name, &acl_rules, &dynamic_subject)
            {
                Ok(policy) => policy,
                Err(e) => {
                    tracing::error!("Failed to generate dynamic policy: {}", e);
                    return (None, None);
                }
            };

            tracing::info!(
                "Created dynamic ACL policy for client: {}",
                cert_info.common_name
            );
            tracing::info!("  Policy ID: {:?}", dynamic_policy.id);
            tracing::info!("  Policy rules: {:?}", dynamic_policy.rules);
            tracing::info!("  Policy subjects: {:?}", dynamic_policy.subjects);

            // Create ACL config from dynamic rules
            let dynamic_acl_config = AclConfig {
                enabled: true,
                default_permission: Permission::Deny,
                rules: Some(acl_rules),
                subjects: Some(vec![dynamic_subject.clone()]),
                policies: Some(vec![dynamic_policy.clone()]),
                query_strategy: Some(QueryStrategy::Static), // Use static mode for dynamic rules
                dynamic_config: None,
            };

            // Create new PolicyEnforcer for this connection and initialize it
            let mut dynamic_policy_enforcer = PolicyEnforcer::new();
            if let Err(e) = dynamic_policy_enforcer.init(&dynamic_acl_config) {
                tracing::error!("Failed to initialize dynamic policy enforcer: {}", e);
                return (None, None);
            }

            // Get the subject ID from the dynamic subject configuration
            // Since we created the subject with id "subject-{client_id}", we need to find its numeric ID
            // For now, we'll assume the first subject in the config has ID 1 (as assigned by SubjectMapBuilder)
            let subject_id = 1; // SubjectMapBuilder assigns IDs starting from 1

            // Create interceptors with the dynamic policy enforcer
            let ingress_enforcer = IngressAclEnforcer {
                policy_enforcer: Arc::new(dynamic_policy_enforcer),
                zid: transport
                    .get_zid()
                    .unwrap_or_else(|_| zenoh_protocol::core::ZenohIdProto::rand()),
                subject: vec![AuthSubject {
                    id: subject_id,
                    name: format!("subject-{}", cert_info.common_name),
                }],
            };

            return (Some(Box::new(ingress_enforcer)), None);
        }

        // Use static ACL
        self.new_static_transport_unicast(transport)
    }

    fn new_transport_multicast(
        &self,
        _transport: &TransportMulticast,
    ) -> Option<EgressInterceptor> {
        tracing::debug!("Transport Multicast is disabled in interceptor");
        None
    }

    fn new_peer_multicast(&self, _transport: &TransportMulticast) -> Option<IngressInterceptor> {
        tracing::debug!("Peer Multicast is disabled in interceptor");
        None
    }
}

struct Cache {
    query: Permission,
    reply: Permission,
    put: Permission,
    delete: Permission,
    declare_subscriber: Permission,
    declare_queryable: Permission,
    declare_token: Permission,
    query_token: Permission,
    declare_liveliness_subscriber: Permission,
}

impl InterceptorTrait for IngressAclEnforcer {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        tracing::trace!("ACL (ingress): caching permissions for `{}` ...", key_expr);
        Some(Box::new(Cache {
            query: self.action(AclMessage::Query, "Query (ingress)", key_expr),
            reply: self.action(AclMessage::Reply, "Reply (ingress)", key_expr),
            put: self.action(AclMessage::Put, "Put (ingress)", key_expr),
            delete: self.action(AclMessage::Delete, "Delete (ingress)", key_expr),
            declare_subscriber: self.action(
                AclMessage::DeclareSubscriber,
                "Declare/Undeclare Subscriber (ingress)",
                key_expr,
            ),
            declare_queryable: self.action(
                AclMessage::DeclareQueryable,
                "Declare/Undeclare Queryable (ingress)",
                key_expr,
            ),
            declare_token: self.action(
                AclMessage::LivelinessToken,
                "Declare/Undeclare Liveliness Token (ingress)",
                key_expr,
            ),
            query_token: self.action(
                AclMessage::LivelinessQuery,
                "Liveliness Query (ingress)",
                key_expr,
            ),
            declare_liveliness_subscriber: self.action(
                AclMessage::DeclareLivelinessSubscriber,
                "Declare Liveliness Subscriber (ingress)",
                key_expr,
            ),
        }))
    }

    fn intercept<'a>(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        let cache = ctx
            .get_cache(msg)
            .and_then(|i| match i.downcast_ref::<Cache>() {
                Some(c) => Some(c),
                None => {
                    tracing::debug!("Cache content type is incorrect");
                    None
                }
            });

        match &msg.body {
            NetworkBodyMut::Request(Request {
                payload: RequestBody::Query(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.query),
                    AclMessage::Query,
                    "Query (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Response(Response { .. }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.reply),
                    AclMessage::Reply,
                    "Reply (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Put(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.put),
                    AclMessage::Put,
                    "Put (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Del(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.delete),
                    AclMessage::Delete,
                    "Delete (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareSubscriber(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_subscriber),
                    AclMessage::DeclareSubscriber,
                    "Declare Subscriber (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareSubscriber(_),
                ..
            }) => {
                // Undeclaration filtering diverges between ingress and egress:
                // Undeclarations in ingress are only filtered if the ext_wire_expr is set.
                // If it's not set, we let the undeclaration pass, it will be rejected by the routing logic
                // if its associated declaration was denied.
                if self.cached_result_or_action_undecl(
                    cache.map(|c| c.declare_subscriber),
                    AclMessage::DeclareSubscriber,
                    "Undeclare Subscriber (ingress)",
                    ctx.full_keyexpr(msg),
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareQueryable(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_queryable),
                    AclMessage::DeclareQueryable,
                    "Declare Queryable (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareQueryable(_),
                ..
            }) => {
                // Undeclaration filtering diverges between ingress and egress:
                // Undeclarations in ingress are only filtered if the ext_wire_expr is set.
                // If it's not set, we let the undeclaration pass, it will be rejected by the routing logic
                // if its associated declaration was denied.
                if self.cached_result_or_action_undecl(
                    cache.map(|c| c.declare_queryable),
                    AclMessage::DeclareQueryable,
                    "Undeclare Queryable (ingress)",
                    ctx.full_keyexpr(msg),
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareToken(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_token),
                    AclMessage::LivelinessToken,
                    "Liveliness Token (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }

            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareToken(_),
                ..
            }) => {
                // Undeclaration filtering diverges between ingress and egress:
                // Undeclarations in ingress are only filtered if the ext_wire_expr is set.
                // If it's not set, we let the undeclaration pass, it will be rejected by the routing logic
                // if its associated declaration was denied.
                if self.cached_result_or_action_undecl(
                    cache.map(|c| c.declare_token),
                    AclMessage::LivelinessToken,
                    "Undeclare Liveliness Token (ingress)",
                    ctx.full_keyexpr(msg),
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Current,
                options,
                ..
            }) if options.tokens() => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.query_token),
                    AclMessage::LivelinessQuery,
                    "Liveliness Query (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Future | InterestMode::CurrentFuture,
                options,
                ..
            }) if options.tokens() => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_liveliness_subscriber),
                    AclMessage::DeclareLivelinessSubscriber,
                    "Declare Liveliness Subscriber (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Final,
                ..
            }) => {
                // InterestMode::Final filtering diverges between ingress and egress:
                // InterestMode::Final ingress is always allowed, it will be rejected by routing logic if its associated Interest was denied
            }
            // Unfiltered Declare messages
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareKeyExpr(_),
                ..
            })
            | NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareFinal(_),
                ..
            }) => {}
            // Unfiltered Undeclare messages
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareKeyExpr(_),
                ..
            }) => {}
            // Unfiltered remaining message types
            NetworkBodyMut::Interest(_)
            | NetworkBodyMut::OAM(_)
            | NetworkBodyMut::ResponseFinal(_) => {}
        }
        true
    }
}

impl InterceptorTrait for EgressAclEnforcer {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        tracing::trace!("ACL (egress): caching permissions for `{}` ...", key_expr);
        Some(Box::new(Cache {
            query: self.action(AclMessage::Query, "Query (egress)", key_expr),
            reply: self.action(AclMessage::Reply, "Reply (egress)", key_expr),
            put: self.action(AclMessage::Put, "Put (egress)", key_expr),
            delete: self.action(AclMessage::Delete, "Delete (egress)", key_expr),
            declare_subscriber: self.action(
                AclMessage::DeclareSubscriber,
                "Declare/Undeclare Subscriber (egress)",
                key_expr,
            ),
            declare_queryable: self.action(
                AclMessage::DeclareQueryable,
                "Declare/Undeclare Queryable (egress)",
                key_expr,
            ),
            declare_token: self.action(
                AclMessage::LivelinessToken,
                "Declare/Undeclare Liveliness Token (egress)",
                key_expr,
            ),
            query_token: self.action(
                AclMessage::LivelinessQuery,
                "Liveliness Query (egress)",
                key_expr,
            ),
            declare_liveliness_subscriber: self.action(
                AclMessage::DeclareLivelinessSubscriber,
                "Declare Liveliness Subscriber (egress)",
                key_expr,
            ),
        }))
    }

    fn intercept(
        // String, Arc<str>, Box<str>, etc... & -> &str
        &self,
        msg: &mut NetworkMessageMut,
        ctx: &mut dyn InterceptorContext,
    ) -> bool {
        let cache = ctx
            .get_cache(msg)
            .and_then(|i| match i.downcast_ref::<Cache>() {
                Some(c) => Some(c),
                None => {
                    tracing::debug!("Cache content type is incorrect");
                    None
                }
            });

        match &msg.body {
            NetworkBodyMut::Request(Request {
                payload: RequestBody::Query(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.query),
                    AclMessage::Query,
                    "Query (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Response(Response { .. }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.reply),
                    AclMessage::Reply,
                    "Reply (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Put(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.put),
                    AclMessage::Put,
                    "Put (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Del(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.put),
                    AclMessage::Delete,
                    "Delete (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareSubscriber(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_subscriber),
                    AclMessage::DeclareSubscriber,
                    "Declare Subscriber (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareSubscriber(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                // Undeclaration filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_subscriber),
                    AclMessage::DeclareSubscriber,
                    "Undeclare Subscriber (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareQueryable(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_queryable),
                    AclMessage::DeclareQueryable,
                    "Declare Queryable (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareQueryable(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                // Undeclaration filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_queryable),
                    AclMessage::DeclareQueryable,
                    "Undeclare Queryable (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareToken(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_token),
                    AclMessage::LivelinessToken,
                    "Liveliness Token (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareToken(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                // Undeclaration filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_token),
                    AclMessage::LivelinessToken,
                    "Undeclare Liveliness Token (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Current,
                options,
                ..
            }) if options.tokens() => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.query_token),
                    AclMessage::LivelinessQuery,
                    "Liveliness Query (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Future | InterestMode::CurrentFuture,
                options,
                ..
            }) if options.tokens() => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_liveliness_subscriber),
                    AclMessage::DeclareLivelinessSubscriber,
                    "Declare Liveliness Subscriber (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Final,
                options,
                ..
            }) if options.tokens() => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                // Note: options are set for InterestMode::Final for internal use only by egress interceptors.

                // InterestMode::Final filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_liveliness_subscriber),
                    AclMessage::DeclareLivelinessSubscriber,
                    "Undeclare Liveliness Subscriber (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            // Unfiltered Declare messages
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareKeyExpr(_),
                ..
            })
            | NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareFinal(_),
                ..
            }) => {}
            // Unfiltered Undeclare messages
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareKeyExpr(_),
                ..
            }) => {}
            // Unfiltered remaining message types
            NetworkBodyMut::Interest(_)
            | NetworkBodyMut::OAM(_)
            | NetworkBodyMut::ResponseFinal(_) => {}
        }
        true
    }
}

pub trait AclActionMethods {
    fn policy_enforcer(&self) -> &PolicyEnforcer;
    fn zid(&self) -> &ZenohIdProto;
    fn flow(&self) -> InterceptorFlow;
    fn authn_ids(&self) -> &Vec<AuthSubject>;
    fn action(&self, action: AclMessage, log_msg: &str, key_expr: &keyexpr) -> Permission {
        let policy_enforcer = self.policy_enforcer();
        let authn_ids = self.authn_ids();
        let zid = self.zid();
        let mut decision = policy_enforcer.default_permission;
        for subject in authn_ids {
            match policy_enforcer.policy_decision_point(subject.id, self.flow(), action, key_expr) {
                Ok(Permission::Allow) => {
                    tracing::trace!(
                        "{} on {} is authorized to {} on {}",
                        zid,
                        subject.name,
                        log_msg,
                        key_expr
                    );
                    decision = Permission::Allow;
                    break;
                }
                Ok(Permission::Deny) => {
                    tracing::trace!(
                        "{} on {} is unauthorized to {} on {}",
                        zid,
                        subject.name,
                        log_msg,
                        key_expr
                    );

                    decision = Permission::Deny;
                    continue;
                }
                Err(e) => {
                    tracing::debug!(
                        "{} on {} has an authorization error to {} on {}: {}",
                        zid,
                        subject.name,
                        log_msg,
                        key_expr,
                        e
                    );
                    return Permission::Deny;
                }
            }
        }
        decision
    }
}

impl AclActionMethods for EgressAclEnforcer {
    fn policy_enforcer(&self) -> &PolicyEnforcer {
        &self.policy_enforcer
    }

    fn zid(&self) -> &ZenohIdProto {
        &self.zid
    }

    fn flow(&self) -> InterceptorFlow {
        InterceptorFlow::Egress
    }

    fn authn_ids(&self) -> &Vec<AuthSubject> {
        &self.subject
    }
}

impl AclActionMethods for IngressAclEnforcer {
    fn policy_enforcer(&self) -> &PolicyEnforcer {
        &self.policy_enforcer
    }

    fn zid(&self) -> &ZenohIdProto {
        &self.zid
    }

    fn flow(&self) -> InterceptorFlow {
        InterceptorFlow::Ingress
    }

    fn authn_ids(&self) -> &Vec<AuthSubject> {
        &self.subject
    }
}
