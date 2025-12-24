//! Dynamic ACL (Access Control List) module
//!
//! This module provides dynamic authentication and authorization capabilities
//! for Zenoh by integrating with external authentication services.

pub mod client;
pub mod rules;
pub mod provider;
pub mod integrator;

#[cfg(feature = "dynamic_acl")]
pub use client::AuthClient;
#[cfg(feature = "dynamic_acl")]
pub use rules::{RuleProcessor, AuthResponse, DynamicAclRule};
#[cfg(feature = "dynamic_acl")]
pub use provider::{DynamicAclProvider, CertificateInfo, DynamicAclResponse, SubjectConfig};
#[cfg(feature = "dynamic_acl")]
pub use integrator::DynamicAclIntegrator;
