/// Shipyard platform-level permission strings.
///
/// Format: `shipyard:{scope}:{resource}:{action}`
///
/// Admin staff permissions control access to /admin/* pages.
/// Superadmin bypasses all permission checks.
/// Regular org users have org-scoped permissions (shipyard:<org_id>:*).

// ── Legacy settings permissions (kept for backwards compatibility) ──────────
pub const PERM_SETTINGS_INFRA_VIEW:    &str = "shipyard:settings:infra:view";
pub const PERM_SETTINGS_INFRA_EDIT:    &str = "shipyard:settings:infra:edit";
pub const PERM_SETTINGS_SMTP_VIEW:     &str = "shipyard:settings:smtp:view";
pub const PERM_SETTINGS_SMTP_EDIT:     &str = "shipyard:settings:smtp:edit";
pub const PERM_SETTINGS_TRAEFIK_VIEW:  &str = "shipyard:settings:traefik:view";
pub const PERM_SETTINGS_TRAEFIK_EDIT:  &str = "shipyard:settings:traefik:edit";
pub const PERM_SETTINGS_DOCKER_VIEW:   &str = "shipyard:settings:docker:view";
pub const PERM_SETTINGS_DOCKER_EDIT:   &str = "shipyard:settings:docker:edit";
pub const PERM_SETTINGS_MQTT_VIEW:     &str = "shipyard:settings:mqtt:view";
pub const PERM_SETTINGS_MQTT_EDIT:     &str = "shipyard:settings:mqtt:edit";
pub const PERM_SETTINGS_PROVIDERS_VIEW:&str = "shipyard:settings:providers:view";
pub const PERM_SETTINGS_PROVIDERS_EDIT:&str = "shipyard:settings:providers:edit";
pub const PERM_ORGS_VIEW:              &str = "shipyard:orgs:view";
pub const PERM_ORGS_EDIT:              &str = "shipyard:orgs:edit";
pub const PERM_ORGS_SUSPEND:           &str = "shipyard:orgs:suspend";
pub const PERM_ORGS_DELETE:            &str = "shipyard:orgs:delete";
pub const PERM_USERS_VIEW:             &str = "shipyard:users:view";
pub const PERM_USERS_EDIT:             &str = "shipyard:users:edit";
pub const PERM_USERS_PROMOTE:          &str = "shipyard:users:promote";
pub const PERM_NODES_VIEW:             &str = "shipyard:nodes:view";
pub const PERM_NODES_MANAGE:           &str = "shipyard:nodes:manage";
pub const PERM_NODES_DELETE:           &str = "shipyard:nodes:delete";
pub const PERM_BILLING_VIEW:           &str = "shipyard:billing:view";
pub const PERM_BILLING_MANAGE:         &str = "shipyard:billing:manage";
pub const PERM_CONFIG_VIEW:            &str = "shipyard:config:view";
pub const PERM_CONFIG_EDIT:            &str = "shipyard:config:edit";

// ── Admin staff permissions (/admin/* pages) ─────────────────────────────────
pub const PERM_ADMIN_ORG_VIEW:         &str = "shipyard:admin:organization:view";
pub const PERM_ADMIN_ORG_MANAGE:       &str = "shipyard:admin:organization:manage";
pub const PERM_ADMIN_USERS_VIEW:       &str = "shipyard:admin:users:view";
pub const PERM_ADMIN_USERS_MANAGE:     &str = "shipyard:admin:users:manage";
pub const PERM_ADMIN_STAFF_VIEW:       &str = "shipyard:admin:staff:view";
pub const PERM_ADMIN_STAFF_MANAGE:     &str = "shipyard:admin:staff:manage";
pub const PERM_ADMIN_PROJECTS_VIEW:    &str = "shipyard:admin:projects:view";
pub const PERM_ADMIN_PROJECTS_MANAGE:  &str = "shipyard:admin:projects:manage";
pub const PERM_ADMIN_DEPLOY_VIEW:      &str = "shipyard:deployments:projects:view";
pub const PERM_ADMIN_DEPLOY_MANAGE:    &str = "shipyard:deployments:projects:manage";
pub const PERM_ADMIN_PROV_VIEW:        &str = "shipyard:deployments:orgs:view";
pub const PERM_ADMIN_PROV_MANAGE:      &str = "shipyard:deployments:orgs:manage";
pub const PERM_ADMIN_NODES_VIEW:       &str = "shipyard:admin:nodes:view";
pub const PERM_ADMIN_NODES_MANAGE:     &str = "shipyard:admin:nodes:manage";
pub const PERM_ADMIN_INFRA_VIEW:       &str = "shipyard:admin:infra:view";
pub const PERM_ADMIN_INFRA_MANAGE:     &str = "shipyard:admin:infra:manage";
pub const PERM_ADMIN_DOCKER_VIEW:      &str = "shipyard:admin:infra:docker:view";
pub const PERM_ADMIN_DOCKER_MANAGE:    &str = "shipyard:admin:infra:docker:manage";
pub const PERM_ADMIN_TRAEFIK_VIEW:     &str = "shipyard:admin:infra:traefik:view";
pub const PERM_ADMIN_TRAEFIK_MANAGE:   &str = "shipyard:admin:infra:traefik:manage";
pub const PERM_ADMIN_MQTT_VIEW:        &str = "shipyard:admin:infra:mqtt:view";
pub const PERM_ADMIN_MQTT_MANAGE:      &str = "shipyard:admin:infra:mqtt:manage";
pub const PERM_ADMIN_STATIC_VIEW:      &str = "shipyard:admin:infra:static:view";
pub const PERM_ADMIN_STATIC_MANAGE:    &str = "shipyard:admin:infra:static:manage";
pub const PERM_ADMIN_SMTP_VIEW:        &str = "shipyard:admin:smtp:view";
pub const PERM_ADMIN_SMTP_MANAGE:      &str = "shipyard:admin:smtp:manage";
pub const PERM_ADMIN_DB_POSTGRES_VIEW: &str = "shipyard:admin:infra:postgres:view";
pub const PERM_ADMIN_DB_POSTGRES_MANAGE:&str = "shipyard:admin:infra:postgres:manage";
pub const PERM_ADMIN_DB_REDIS_VIEW:    &str = "shipyard:admin:infra:redis:view";
pub const PERM_ADMIN_DB_REDIS_MANAGE:  &str = "shipyard:admin:infra:redis:manage";
pub const PERM_ADMIN_AUDIT_VIEW:       &str = "shipyard:admin:audit:view";
pub const PERM_ADMIN_AUDIT_MANAGE:     &str = "shipyard:admin:audit:manage";
pub const PERM_ADMIN_PLAN_VIEW:        &str = "shipyard:admin:plan:view";
pub const PERM_ADMIN_PLAN_MANAGE:      &str = "shipyard:admin:plan:manage";
pub const PERM_ADMIN_UPDATES_VIEW:     &str = "shipyard:admin:system:update:view";
pub const PERM_ADMIN_UPDATES_MANAGE:   &str = "shipyard:admin:system:update:manage";
pub const PERM_ADMIN_CONFIG_VIEW:      &str = "shipyard:admin:system:config:view";
pub const PERM_ADMIN_CONFIG_MANAGE:    &str = "shipyard:admin:system:config:manage";

/// All permissions granted to a super-admin user. Embedded in their JWT.
pub const SUPERADMIN_PERMISSIONS: &[&str] = &[
    PERM_SETTINGS_INFRA_VIEW,
    PERM_SETTINGS_INFRA_EDIT,
    PERM_SETTINGS_SMTP_VIEW,
    PERM_SETTINGS_SMTP_EDIT,
    PERM_SETTINGS_TRAEFIK_VIEW,
    PERM_SETTINGS_TRAEFIK_EDIT,
    PERM_SETTINGS_DOCKER_VIEW,
    PERM_SETTINGS_DOCKER_EDIT,
    PERM_SETTINGS_MQTT_VIEW,
    PERM_SETTINGS_MQTT_EDIT,
    PERM_SETTINGS_PROVIDERS_VIEW,
    PERM_SETTINGS_PROVIDERS_EDIT,
    PERM_ORGS_VIEW,
    PERM_ORGS_EDIT,
    PERM_ORGS_SUSPEND,
    PERM_ORGS_DELETE,
    PERM_USERS_VIEW,
    PERM_USERS_EDIT,
    PERM_USERS_PROMOTE,
    PERM_NODES_VIEW,
    PERM_NODES_MANAGE,
    PERM_NODES_DELETE,
    PERM_BILLING_VIEW,
    PERM_BILLING_MANAGE,
    PERM_CONFIG_VIEW,
    PERM_CONFIG_EDIT,
];

/// Returns the full superadmin permission list as owned Strings.
pub fn superadmin_permissions() -> Vec<String> {
    SUPERADMIN_PERMISSIONS.iter().map(|s| s.to_string()).collect()
}

/// Check whether a permission list grants a specific permission.
/// Supports suffix wildcards: `"shipyard:settings:*"` matches `"shipyard:settings:infra:view"`.
pub fn has_permission(permissions: &[String], required: &str) -> bool {
    permissions.iter().any(|p| {
        if p == required {
            return true;
        }
        // wildcard: "shipyard:*" matches anything starting with "shipyard:"
        if let Some(prefix) = p.strip_suffix('*') {
            return required.starts_with(prefix);
        }
        false
    })
}
