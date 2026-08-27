//! Multi-tenant support with Row-Level Security (RLS) and access isolation.
//!
//! Provides tenant context tracking and isolation mechanisms to ensure
//! data access is restricted to the authenticated tenant.

use std::sync::Arc;
use uuid::Uuid;

/// Represents a tenant in the system
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TenantId(String);

impl TenantId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn default_tenant() -> Self {
        Self("default".to_string())
    }

    pub fn is_default(&self) -> bool {
        self.0 == "default"
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TenantId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TenantId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Request context carrying tenant information
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: TenantId,
    pub user_id: Option<String>,
    pub request_id: String,
}

impl TenantContext {
    pub fn new(tenant_id: TenantId, user_id: Option<String>) -> Self {
        Self {
            tenant_id,
            user_id,
            request_id: Uuid::new_v4().to_string(),
        }
    }

    pub fn from_header(tenant_header: Option<&str>) -> Self {
        let tenant_id = tenant_header
            .map(|s| TenantId::new(s.to_string()))
            .unwrap_or_else(TenantId::default_tenant);

        Self::new(tenant_id, None)
    }

    pub fn with_user(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }
}

/// Tenant provisioning data
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TenantProvisioning {
    pub tenant_id: String,
    pub name: String,
    pub admin_email: Option<String>,
    pub config: Option<serde_json::Value>,
}

impl TenantProvisioning {
    pub fn new(tenant_id: String, name: String) -> Self {
        Self {
            tenant_id,
            name,
            admin_email: None,
            config: None,
        }
    }

    pub fn with_admin_email(mut self, email: String) -> Self {
        self.admin_email = Some(email);
        self
    }

    pub fn with_config(mut self, config: serde_json::Value) -> Self {
        self.config = Some(config);
        self
    }
}

/// Tenant provider trait for custom implementations
pub trait TenantProvider: Send + Sync {
    fn get_tenant(&self, tenant_id: &str) -> Option<TenantProvisioning>;
    fn provision_tenant(&self, provisioning: TenantProvisioning) -> Result<(), String>;
    fn list_tenants(&self) -> Vec<TenantProvisioning>;
}

/// In-memory tenant provider for testing
#[derive(Debug, Default)]
pub struct InMemoryTenantProvider {
    tenants: Arc<std::sync::RwLock<std::collections::HashMap<String, TenantProvisioning>>>,
}

impl InMemoryTenantProvider {
    pub fn new() -> Self {
        let mut provider = Self::default();
        // Initialize with default tenant
        let _ = provider.provision_tenant(TenantProvisioning::new(
            "default".to_string(),
            "Default Tenant".to_string(),
        ));
        provider
    }
}

impl TenantProvider for InMemoryTenantProvider {
    fn get_tenant(&self, tenant_id: &str) -> Option<TenantProvisioning> {
        self.tenants.read().ok()?.get(tenant_id).cloned()
    }

    fn provision_tenant(&self, provisioning: TenantProvisioning) -> Result<(), String> {
        let mut tenants = self
            .tenants
            .write()
            .map_err(|e| format!("failed to acquire lock: {e}"))?;
        tenants.insert(provisioning.tenant_id.clone(), provisioning);
        Ok(())
    }

    fn list_tenants(&self) -> Vec<TenantProvisioning> {
        self.tenants
            .read()
            .ok()
            .map(|t| t.values().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_id_creation() {
        let id = TenantId::new("tenant-1");
        assert_eq!(id.as_str(), "tenant-1");
        assert!(!id.is_default());
    }

    #[test]
    fn default_tenant() {
        let id = TenantId::default_tenant();
        assert_eq!(id.as_str(), "default");
        assert!(id.is_default());
    }

    #[test]
    fn tenant_context_from_header() {
        let ctx = TenantContext::from_header(Some("my-tenant"));
        assert_eq!(ctx.tenant_id.as_str(), "my-tenant");
        assert!(ctx.user_id.is_none());
    }

    #[test]
    fn tenant_context_default_when_header_missing() {
        let ctx = TenantContext::from_header(None);
        assert_eq!(ctx.tenant_id.as_str(), "default");
    }

    #[test]
    fn tenant_provisioning_with_email() {
        let prov = TenantProvisioning::new("tenant-1".to_string(), "Tenant 1".to_string())
            .with_admin_email("admin@example.com".to_string());
        assert_eq!(prov.admin_email, Some("admin@example.com".to_string()));
    }

    #[test]
    fn in_memory_provider_provisioning() {
        let provider = InMemoryTenantProvider::new();
        let prov = TenantProvisioning::new("test-tenant".to_string(), "Test".to_string());
        assert!(provider.provision_tenant(prov).is_ok());

        let retrieved = provider.get_tenant("test-tenant");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test");
    }

    #[test]
    fn in_memory_provider_list_tenants() {
        let provider = InMemoryTenantProvider::new();
        provider
            .provision_tenant(TenantProvisioning::new(
                "t1".to_string(),
                "T1".to_string(),
            ))
            .ok();
        provider
            .provision_tenant(TenantProvisioning::new(
                "t2".to_string(),
                "T2".to_string(),
            ))
            .ok();

        let tenants = provider.list_tenants();
        assert!(tenants.len() >= 3); // default + t1 + t2
    }
}
