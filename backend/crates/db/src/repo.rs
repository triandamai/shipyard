//! Repository traits and PostgreSQL implementations for all Shipyard domain entities.
//!
//! Uses sqlx non-macro form (`sqlx::query_as::<_, T>(sql).bind(...)`) so the file
//! compiles without a live database (no `sqlx::query!` macro offline checks needed).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{
    Container, Deployment, DeploymentLog, DeploymentStep, DockerEvent, Domain, Invitation,
    Network, OrgMember, Organization, Project, Service, ServiceEnv, SystemConfig, TopologyEdge,
    User, Volume,
};

// ════════════════════════════════════════════════════════════════
// Input types
// ════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct CreateUserInput {
    pub email: String,
    pub password_hash: String,
}

#[derive(Debug, Clone)]
pub struct UpdateUserPasswordInput {
    pub id: Uuid,
    pub password_hash: String,
}

#[derive(Debug, Clone)]
pub struct CreateOrgInput {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone)]
pub struct UpdateOrgInput {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone)]
pub struct UpsertOrgMemberInput {
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
}

#[derive(Debug, Clone)]
pub struct CreateInvitationInput {
    pub org_id: Uuid,
    pub email: String,
    pub role: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateProjectInput {
    pub org_id: Uuid,
    pub name: String,
    pub slug: String,
    pub directory_path: String,
}

#[derive(Debug, Clone)]
pub struct UpdateProjectInput {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub directory_path: String,
}

#[derive(Debug, Clone)]
pub struct CreateServiceInput {
    pub project_id: Uuid,
    pub name: String,
    pub slug: String,
    pub service_type: String,
    pub directory_path: String,
}

#[derive(Debug, Clone)]
pub struct UpdateServiceInput {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub service_type: String,
    pub directory_path: String,
}

#[derive(Debug, Clone)]
pub struct UpsertServiceEnvInput {
    pub service_id: Uuid,
    pub key: String,
    pub value_encrypted: String,
    pub is_secret: bool,
}

#[derive(Debug, Clone)]
pub struct CreateVolumeInput {
    pub service_id: Uuid,
    pub name: String,
    pub mount_path: String,
    pub driver: String,
    pub size_mb: i32,
}

#[derive(Debug, Clone)]
pub struct CreateNetworkInput {
    pub project_id: Uuid,
    pub name: String,
    pub driver: String,
    pub subnet: String,
}

#[derive(Debug, Clone)]
pub struct CreateDomainInput {
    pub service_id: Uuid,
    pub hostname: String,
    pub tls_enabled: bool,
    pub traefik_router_name: String,
}

#[derive(Debug, Clone)]
pub struct UpsertContainerInput {
    pub service_id: Uuid,
    pub docker_container_id: String,
    pub docker_task_id: Option<String>,
    pub node_id: Option<String>,
    pub replica_index: Option<i32>,
    pub status: String,
    pub status_message: Option<String>,
    pub image: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct InsertDockerEventInput {
    pub event_type: String,
    pub action: String,
    pub actor_id: String,
    pub actor_attributes: Option<serde_json::Value>,
    pub scope: Option<String>,
    pub raw: serde_json::Value,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DockerEventFilter {
    pub event_type: Option<String>,
    pub action: Option<String>,
    pub actor_id: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct CreateDeploymentInput {
    pub service_id: Uuid,
    pub triggered_by: String,
    pub source_ref: String,
}

#[derive(Debug, Clone)]
pub struct CreateDeploymentStepInput {
    pub deployment_id: Uuid,
    pub name: String,
    pub order_index: i32,
}

#[derive(Debug, Clone)]
pub struct InsertDeploymentLogInput {
    pub deployment_id: Uuid,
    pub step_id: Option<Uuid>,
    pub level: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UpsertTopologyEdgeInput {
    pub project_id: Uuid,
    pub source_node_id: String,
    pub target_node_id: String,
    pub edge_type: String,
}

// ════════════════════════════════════════════════════════════════
// Repository Traits
// ════════════════════════════════════════════════════════════════

#[async_trait]
pub trait UserRepo: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, sqlx::Error>;
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error>;
    async fn create(&self, input: CreateUserInput) -> Result<User, sqlx::Error>;
    async fn update_password(&self, input: UpdateUserPasswordInput) -> Result<User, sqlx::Error>;
    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait OrgRepo: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Organization>, sqlx::Error>;
    async fn find_by_slug(&self, slug: &str) -> Result<Option<Organization>, sqlx::Error>;
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<Organization>, sqlx::Error>;
    async fn create(&self, input: CreateOrgInput) -> Result<Organization, sqlx::Error>;
    async fn update(&self, input: UpdateOrgInput) -> Result<Organization, sqlx::Error>;
    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait OrgMemberRepo: Send + Sync {
    async fn find_by_org_and_user(
        &self,
        org_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<OrgMember>, sqlx::Error>;
    async fn list_by_org(&self, org_id: Uuid) -> Result<Vec<OrgMember>, sqlx::Error>;
    async fn list_by_user(&self, user_id: Uuid) -> Result<Vec<OrgMember>, sqlx::Error>;
    async fn upsert(&self, input: UpsertOrgMemberInput) -> Result<OrgMember, sqlx::Error>;
    async fn delete(&self, org_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait InvitationRepo: Send + Sync {
    async fn find_by_token(&self, token: &str) -> Result<Option<Invitation>, sqlx::Error>;
    async fn create(&self, input: CreateInvitationInput) -> Result<Invitation, sqlx::Error>;
    async fn accept(&self, id: Uuid, accepted_at: DateTime<Utc>) -> Result<Invitation, sqlx::Error>;
    async fn list_by_org(&self, org_id: Uuid) -> Result<Vec<Invitation>, sqlx::Error>;
}

#[async_trait]
pub trait ProjectRepo: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Project>, sqlx::Error>;
    async fn find_by_org_and_slug(
        &self,
        org_id: Uuid,
        slug: &str,
    ) -> Result<Option<Project>, sqlx::Error>;
    async fn list_by_org(&self, org_id: Uuid) -> Result<Vec<Project>, sqlx::Error>;
    async fn create(&self, input: CreateProjectInput) -> Result<Project, sqlx::Error>;
    async fn update(&self, input: UpdateProjectInput) -> Result<Project, sqlx::Error>;
    async fn update_node_positions(
        &self,
        id: Uuid,
        positions: serde_json::Value,
    ) -> Result<Project, sqlx::Error>;
    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait ServiceRepo: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Service>, sqlx::Error>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Service>, sqlx::Error>;
    async fn create(&self, input: CreateServiceInput) -> Result<Service, sqlx::Error>;
    async fn update_status(&self, id: Uuid, status: &str) -> Result<Service, sqlx::Error>;
    async fn update_replicas(&self, id: Uuid, replicas: i32) -> Result<Service, sqlx::Error>;
    async fn update(&self, input: UpdateServiceInput) -> Result<Service, sqlx::Error>;
    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait ServiceEnvRepo: Send + Sync {
    async fn list_by_service(&self, service_id: Uuid) -> Result<Vec<ServiceEnv>, sqlx::Error>;
    async fn upsert(&self, input: UpsertServiceEnvInput) -> Result<ServiceEnv, sqlx::Error>;
    async fn delete_by_key(&self, service_id: Uuid, key: &str) -> Result<(), sqlx::Error>;
    async fn delete_all_for_service(&self, service_id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait VolumeRepo: Send + Sync {
    async fn list_by_service(&self, service_id: Uuid) -> Result<Vec<Volume>, sqlx::Error>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Volume>, sqlx::Error>;
    async fn create(&self, input: CreateVolumeInput) -> Result<Volume, sqlx::Error>;
    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait NetworkRepo: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Network>, sqlx::Error>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Network>, sqlx::Error>;
    async fn create(&self, input: CreateNetworkInput) -> Result<Network, sqlx::Error>;
    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait ServiceNetworkRepo: Send + Sync {
    async fn attach(&self, service_id: Uuid, network_id: Uuid) -> Result<(), sqlx::Error>;
    async fn detach(&self, service_id: Uuid, network_id: Uuid) -> Result<(), sqlx::Error>;
    async fn list_by_service(&self, service_id: Uuid) -> Result<Vec<Network>, sqlx::Error>;
    async fn list_by_network(&self, network_id: Uuid) -> Result<Vec<Service>, sqlx::Error>;
}

#[async_trait]
pub trait DomainRepo: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Domain>, sqlx::Error>;
    async fn find_by_hostname(&self, hostname: &str) -> Result<Option<Domain>, sqlx::Error>;
    async fn list_by_service(&self, service_id: Uuid) -> Result<Vec<Domain>, sqlx::Error>;
    async fn create(&self, input: CreateDomainInput) -> Result<Domain, sqlx::Error>;
    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait ContainerRepo: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Container>, sqlx::Error>;
    async fn find_by_docker_id(
        &self,
        docker_container_id: &str,
    ) -> Result<Option<Container>, sqlx::Error>;
    async fn list_by_service(
        &self,
        service_id: Uuid,
        status_filter: Option<&str>,
    ) -> Result<Vec<Container>, sqlx::Error>;
    async fn upsert_by_docker_id(
        &self,
        input: UpsertContainerInput,
    ) -> Result<Container, sqlx::Error>;
    async fn update_status(
        &self,
        id: Uuid,
        status: &str,
        status_message: Option<&str>,
        finished_at: Option<DateTime<Utc>>,
        exit_code: Option<i32>,
    ) -> Result<Container, sqlx::Error>;
    async fn count_running_for_service(&self, service_id: Uuid) -> Result<i64, sqlx::Error>;
    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait DockerEventRepo: Send + Sync {
    async fn insert(&self, input: InsertDockerEventInput) -> Result<DockerEvent, sqlx::Error>;
    async fn list_paginated(
        &self,
        filter: DockerEventFilter,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DockerEvent>, sqlx::Error>;
}

#[async_trait]
pub trait DeploymentRepo: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Deployment>, sqlx::Error>;
    async fn list_by_service(
        &self,
        service_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Deployment>, sqlx::Error>;
    async fn create(&self, input: CreateDeploymentInput) -> Result<Deployment, sqlx::Error>;
    async fn update_status(
        &self,
        id: Uuid,
        status: &str,
        finished_at: Option<DateTime<Utc>>,
    ) -> Result<Deployment, sqlx::Error>;
    async fn latest_success_for_service(
        &self,
        service_id: Uuid,
    ) -> Result<Option<Deployment>, sqlx::Error>;
}

#[async_trait]
pub trait DeploymentStepRepo: Send + Sync {
    async fn list_by_deployment(
        &self,
        deployment_id: Uuid,
    ) -> Result<Vec<DeploymentStep>, sqlx::Error>;
    async fn create(&self, input: CreateDeploymentStepInput) -> Result<DeploymentStep, sqlx::Error>;
    async fn update_status(
        &self,
        id: Uuid,
        status: &str,
        started_at: Option<DateTime<Utc>>,
        finished_at: Option<DateTime<Utc>>,
    ) -> Result<DeploymentStep, sqlx::Error>;
}

#[async_trait]
pub trait DeploymentLogRepo: Send + Sync {
    async fn list_by_deployment(
        &self,
        deployment_id: Uuid,
        step_id: Option<Uuid>,
        level: Option<&str>,
    ) -> Result<Vec<DeploymentLog>, sqlx::Error>;
    async fn insert_batch(
        &self,
        logs: Vec<InsertDeploymentLogInput>,
    ) -> Result<Vec<DeploymentLog>, sqlx::Error>;
}

#[async_trait]
pub trait TopologyEdgeRepo: Send + Sync {
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<TopologyEdge>, sqlx::Error>;
    async fn upsert_edge(
        &self,
        input: UpsertTopologyEdgeInput,
    ) -> Result<TopologyEdge, sqlx::Error>;
    async fn delete_edge(
        &self,
        project_id: Uuid,
        source_node_id: &str,
        target_node_id: &str,
    ) -> Result<(), sqlx::Error>;
    async fn delete_all_for_project(&self, project_id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait SystemConfigRepo: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<SystemConfig>, sqlx::Error>;
    async fn set(&self, key: &str, value: serde_json::Value) -> Result<SystemConfig, sqlx::Error>;
}

// ════════════════════════════════════════════════════════════════
// PostgreSQL Implementations
// ════════════════════════════════════════════════════════════════

// ─── UserRepo ────────────────────────────────────────────────────

pub struct PgUserRepo {
    pub pool: PgPool,
}

impl PgUserRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepo for PgUserRepo {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, email, password_hash, is_superadmin, staff_permissions, created_at, updated_at FROM users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, email, password_hash, is_superadmin, staff_permissions, created_at, updated_at FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
    }

    async fn create(&self, input: CreateUserInput) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (email, password_hash)
            VALUES ($1, $2)
            RETURNING id, email, password_hash, is_superadmin, staff_permissions, created_at, updated_at
            "#,
        )
        .bind(&input.email)
        .bind(&input.password_hash)
        .fetch_one(&self.pool)
        .await
    }

    async fn update_password(&self, input: UpdateUserPasswordInput) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"
            UPDATE users
            SET password_hash = $2
            WHERE id = $1
            RETURNING id, email, password_hash, is_superadmin, staff_permissions, created_at, updated_at
            "#,
        )
        .bind(input.id)
        .bind(&input.password_hash)
        .fetch_one(&self.pool)
        .await
    }

    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ─── OrgRepo ─────────────────────────────────────────────────────

pub struct PgOrgRepo {
    pub pool: PgPool,
}

impl PgOrgRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OrgRepo for PgOrgRepo {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Organization>, sqlx::Error> {
        sqlx::query_as::<_, Organization>(
            "SELECT id, name, slug, created_at FROM organizations WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Organization>, sqlx::Error> {
        sqlx::query_as::<_, Organization>(
            "SELECT id, name, slug, created_at FROM organizations WHERE slug = $1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<Organization>, sqlx::Error> {
        sqlx::query_as::<_, Organization>(
            r#"
            SELECT o.id, o.name, o.slug, o.created_at
            FROM organizations o
            INNER JOIN org_members m ON m.org_id = o.id
            WHERE m.user_id = $1
            ORDER BY o.name ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn create(&self, input: CreateOrgInput) -> Result<Organization, sqlx::Error> {
        sqlx::query_as::<_, Organization>(
            r#"
            INSERT INTO organizations (name, slug)
            VALUES ($1, $2)
            RETURNING id, name, slug, created_at
            "#,
        )
        .bind(&input.name)
        .bind(&input.slug)
        .fetch_one(&self.pool)
        .await
    }

    async fn update(&self, input: UpdateOrgInput) -> Result<Organization, sqlx::Error> {
        sqlx::query_as::<_, Organization>(
            r#"
            UPDATE organizations
            SET name = $2, slug = $3
            WHERE id = $1
            RETURNING id, name, slug, created_at
            "#,
        )
        .bind(input.id)
        .bind(&input.name)
        .bind(&input.slug)
        .fetch_one(&self.pool)
        .await
    }

    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM organizations WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ─── OrgMemberRepo ───────────────────────────────────────────────

pub struct PgOrgMemberRepo {
    pub pool: PgPool,
}

impl PgOrgMemberRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OrgMemberRepo for PgOrgMemberRepo {
    async fn find_by_org_and_user(
        &self,
        org_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<OrgMember>, sqlx::Error> {
        sqlx::query_as::<_, OrgMember>(
            r#"
            SELECT id, org_id, user_id, role::text AS role, created_at
            FROM org_members
            WHERE org_id = $1 AND user_id = $2
            "#,
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_by_org(&self, org_id: Uuid) -> Result<Vec<OrgMember>, sqlx::Error> {
        sqlx::query_as::<_, OrgMember>(
            r#"
            SELECT id, org_id, user_id, role::text AS role, created_at
            FROM org_members
            WHERE org_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn list_by_user(&self, user_id: Uuid) -> Result<Vec<OrgMember>, sqlx::Error> {
        sqlx::query_as::<_, OrgMember>(
            r#"
            SELECT id, org_id, user_id, role::text AS role, created_at
            FROM org_members
            WHERE user_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn upsert(&self, input: UpsertOrgMemberInput) -> Result<OrgMember, sqlx::Error> {
        sqlx::query_as::<_, OrgMember>(
            r#"
            INSERT INTO org_members (org_id, user_id, role)
            VALUES ($1, $2, $3::member_role)
            ON CONFLICT (org_id, user_id) DO UPDATE
                SET role = EXCLUDED.role
            RETURNING id, org_id, user_id, role::text AS role, created_at
            "#,
        )
        .bind(input.org_id)
        .bind(input.user_id)
        .bind(&input.role)
        .fetch_one(&self.pool)
        .await
    }

    async fn delete(&self, org_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM org_members WHERE org_id = $1 AND user_id = $2")
            .bind(org_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ─── InvitationRepo ──────────────────────────────────────────────

pub struct PgInvitationRepo {
    pub pool: PgPool,
}

impl PgInvitationRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InvitationRepo for PgInvitationRepo {
    async fn find_by_token(&self, token: &str) -> Result<Option<Invitation>, sqlx::Error> {
        sqlx::query_as::<_, Invitation>(
            r#"
            SELECT id, org_id, email, role::text AS role, token, expires_at, accepted_at, created_at
            FROM invitations
            WHERE token = $1
            "#,
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
    }

    async fn create(&self, input: CreateInvitationInput) -> Result<Invitation, sqlx::Error> {
        sqlx::query_as::<_, Invitation>(
            r#"
            INSERT INTO invitations (org_id, email, role, token, expires_at)
            VALUES ($1, $2, $3::member_role, $4, $5)
            RETURNING id, org_id, email, role::text AS role, token, expires_at, accepted_at, created_at
            "#,
        )
        .bind(input.org_id)
        .bind(&input.email)
        .bind(&input.role)
        .bind(&input.token)
        .bind(input.expires_at)
        .fetch_one(&self.pool)
        .await
    }

    async fn accept(
        &self,
        id: Uuid,
        accepted_at: DateTime<Utc>,
    ) -> Result<Invitation, sqlx::Error> {
        sqlx::query_as::<_, Invitation>(
            r#"
            UPDATE invitations
            SET accepted_at = $2
            WHERE id = $1
            RETURNING id, org_id, email, role::text AS role, token, expires_at, accepted_at, created_at
            "#,
        )
        .bind(id)
        .bind(accepted_at)
        .fetch_one(&self.pool)
        .await
    }

    async fn list_by_org(&self, org_id: Uuid) -> Result<Vec<Invitation>, sqlx::Error> {
        sqlx::query_as::<_, Invitation>(
            r#"
            SELECT id, org_id, email, role::text AS role, token, expires_at, accepted_at, created_at
            FROM invitations
            WHERE org_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await
    }
}

// ─── ProjectRepo ─────────────────────────────────────────────────

pub struct PgProjectRepo {
    pub pool: PgPool,
}

impl PgProjectRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProjectRepo for PgProjectRepo {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Project>, sqlx::Error> {
        sqlx::query_as::<_, Project>(
            r#"
            SELECT id, org_id, name, slug, directory_path, node_positions, created_at, updated_at
            FROM projects
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn find_by_org_and_slug(
        &self,
        org_id: Uuid,
        slug: &str,
    ) -> Result<Option<Project>, sqlx::Error> {
        sqlx::query_as::<_, Project>(
            r#"
            SELECT id, org_id, name, slug, directory_path, node_positions, created_at, updated_at
            FROM projects
            WHERE org_id = $1 AND slug = $2
            "#,
        )
        .bind(org_id)
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_by_org(&self, org_id: Uuid) -> Result<Vec<Project>, sqlx::Error> {
        sqlx::query_as::<_, Project>(
            r#"
            SELECT id, org_id, name, slug, directory_path, node_positions, created_at, updated_at
            FROM projects
            WHERE org_id = $1
            ORDER BY name ASC
            "#,
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn create(&self, input: CreateProjectInput) -> Result<Project, sqlx::Error> {
        sqlx::query_as::<_, Project>(
            r#"
            INSERT INTO projects (org_id, name, slug, directory_path)
            VALUES ($1, $2, $3, $4)
            RETURNING id, org_id, name, slug, directory_path, node_positions, created_at, updated_at
            "#,
        )
        .bind(input.org_id)
        .bind(&input.name)
        .bind(&input.slug)
        .bind(&input.directory_path)
        .fetch_one(&self.pool)
        .await
    }

    async fn update(&self, input: UpdateProjectInput) -> Result<Project, sqlx::Error> {
        sqlx::query_as::<_, Project>(
            r#"
            UPDATE projects
            SET name = $2, slug = $3, directory_path = $4
            WHERE id = $1
            RETURNING id, org_id, name, slug, directory_path, node_positions, created_at, updated_at
            "#,
        )
        .bind(input.id)
        .bind(&input.name)
        .bind(&input.slug)
        .bind(&input.directory_path)
        .fetch_one(&self.pool)
        .await
    }

    async fn update_node_positions(
        &self,
        id: Uuid,
        positions: serde_json::Value,
    ) -> Result<Project, sqlx::Error> {
        sqlx::query_as::<_, Project>(
            r#"
            UPDATE projects
            SET node_positions = $2
            WHERE id = $1
            RETURNING id, org_id, name, slug, directory_path, node_positions, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(positions)
        .fetch_one(&self.pool)
        .await
    }

    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ─── ServiceRepo ─────────────────────────────────────────────────

pub struct PgServiceRepo {
    pub pool: PgPool,
}

impl PgServiceRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ServiceRepo for PgServiceRepo {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Service>, sqlx::Error> {
        sqlx::query_as::<_, Service>(
            r#"
            SELECT id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, git_deploy_strategy, git_deploy_branch, git_deploy_tag_pattern, git_provider_id, icon, created_at, updated_at
            FROM services
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Service>, sqlx::Error> {
        sqlx::query_as::<_, Service>(
            r#"
            SELECT id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, git_deploy_strategy, git_deploy_branch, git_deploy_tag_pattern, git_provider_id, icon, created_at, updated_at
            FROM services
            WHERE project_id = $1
            ORDER BY name ASC
            "#,
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn create(&self, input: CreateServiceInput) -> Result<Service, sqlx::Error> {
        sqlx::query_as::<_, Service>(
            r#"
            INSERT INTO services (project_id, name, slug, type, directory_path, git_repo_url, git_branch, auto_deploy, image, ports, status, replicas, git_deploy_strategy, git_provider_id, icon)
            VALUES ($1, $2, $3, $4::service_type, $5, NULL, 'main', true, '', '[]'::jsonb, 'stopped', 1, 'push', NULL, NULL)
            RETURNING id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, git_deploy_strategy, git_deploy_branch, git_deploy_tag_pattern, git_provider_id, icon, created_at, updated_at
            "#,
        )
        .bind(input.project_id)
        .bind(&input.name)
        .bind(&input.slug)
        .bind(&input.service_type)
        .bind(&input.directory_path)
        .fetch_one(&self.pool)
        .await
    }

    async fn update_status(&self, id: Uuid, status: &str) -> Result<Service, sqlx::Error> {
        sqlx::query_as::<_, Service>(
            r#"
            UPDATE services
            SET status = $2
            WHERE id = $1
            RETURNING id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, git_deploy_strategy, git_deploy_branch, git_deploy_tag_pattern, git_provider_id, icon, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(status)
        .fetch_one(&self.pool)
        .await
    }

    async fn update_replicas(&self, id: Uuid, replicas: i32) -> Result<Service, sqlx::Error> {
        sqlx::query_as::<_, Service>(
            r#"
            UPDATE services
            SET replicas = $2
            WHERE id = $1
            RETURNING id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, git_deploy_strategy, git_deploy_branch, git_deploy_tag_pattern, git_provider_id, icon, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(replicas)
        .fetch_one(&self.pool)
        .await
    }

    async fn update(&self, input: UpdateServiceInput) -> Result<Service, sqlx::Error> {
        sqlx::query_as::<_, Service>(
            r#"
            UPDATE services
            SET name = $2, slug = $3, type = $4::service_type, directory_path = $5
            WHERE id = $1
            RETURNING id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, git_deploy_strategy, git_deploy_branch, git_deploy_tag_pattern, git_provider_id, icon, created_at, updated_at
            "#,
        )
        .bind(input.id)
        .bind(&input.name)
        .bind(&input.slug)
        .bind(&input.service_type)
        .bind(&input.directory_path)
        .fetch_one(&self.pool)
        .await
    }

    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM services WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ─── ServiceEnvRepo ──────────────────────────────────────────────

pub struct PgServiceEnvRepo {
    pub pool: PgPool,
}

impl PgServiceEnvRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ServiceEnvRepo for PgServiceEnvRepo {
    async fn list_by_service(&self, service_id: Uuid) -> Result<Vec<ServiceEnv>, sqlx::Error> {
        sqlx::query_as::<_, ServiceEnv>(
            r#"
            SELECT id, service_id, key, value_encrypted, is_secret, created_at
            FROM service_envs
            WHERE service_id = $1
            ORDER BY key ASC
            "#,
        )
        .bind(service_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn upsert(&self, input: UpsertServiceEnvInput) -> Result<ServiceEnv, sqlx::Error> {
        sqlx::query_as::<_, ServiceEnv>(
            r#"
            INSERT INTO service_envs (service_id, key, value_encrypted, is_secret)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (service_id, key) DO UPDATE
                SET value_encrypted = EXCLUDED.value_encrypted,
                    is_secret = EXCLUDED.is_secret
            RETURNING id, service_id, key, value_encrypted, is_secret, created_at
            "#,
        )
        .bind(input.service_id)
        .bind(&input.key)
        .bind(&input.value_encrypted)
        .bind(input.is_secret)
        .fetch_one(&self.pool)
        .await
    }

    async fn delete_by_key(&self, service_id: Uuid, key: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM service_envs WHERE service_id = $1 AND key = $2")
            .bind(service_id)
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_all_for_service(&self, service_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM service_envs WHERE service_id = $1")
            .bind(service_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ─── VolumeRepo ──────────────────────────────────────────────────

pub struct PgVolumeRepo {
    pub pool: PgPool,
}

impl PgVolumeRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VolumeRepo for PgVolumeRepo {
    async fn list_by_service(&self, service_id: Uuid) -> Result<Vec<Volume>, sqlx::Error> {
        sqlx::query_as::<_, Volume>(
            r#"
            SELECT id, service_id, name, mount_path, driver, size_mb, created_at
            FROM volumes
            WHERE service_id = $1
            ORDER BY name ASC
            "#,
        )
        .bind(service_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Volume>, sqlx::Error> {
        sqlx::query_as::<_, Volume>(
            "SELECT id, service_id, name, mount_path, driver, size_mb, created_at FROM volumes WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn create(&self, input: CreateVolumeInput) -> Result<Volume, sqlx::Error> {
        sqlx::query_as::<_, Volume>(
            r#"
            INSERT INTO volumes (service_id, name, mount_path, driver, size_mb)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, service_id, name, mount_path, driver, size_mb, created_at
            "#,
        )
        .bind(input.service_id)
        .bind(&input.name)
        .bind(&input.mount_path)
        .bind(&input.driver)
        .bind(input.size_mb)
        .fetch_one(&self.pool)
        .await
    }

    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM volumes WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ─── NetworkRepo ─────────────────────────────────────────────────

pub struct PgNetworkRepo {
    pub pool: PgPool,
}

impl PgNetworkRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NetworkRepo for PgNetworkRepo {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Network>, sqlx::Error> {
        sqlx::query_as::<_, Network>(
            "SELECT id, project_id, name, driver, subnet, docker_network_id, created_at FROM networks WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Network>, sqlx::Error> {
        sqlx::query_as::<_, Network>(
            r#"
            SELECT id, project_id, name, driver, subnet, docker_network_id, created_at
            FROM networks
            WHERE project_id = $1
            ORDER BY name ASC
            "#,
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn create(&self, input: CreateNetworkInput) -> Result<Network, sqlx::Error> {
        sqlx::query_as::<_, Network>(
            r#"
            INSERT INTO networks (project_id, name, driver, subnet)
            VALUES ($1, $2, $3, $4)
            RETURNING id, project_id, name, driver, subnet, docker_network_id, created_at
            "#,
        )
        .bind(input.project_id)
        .bind(&input.name)
        .bind(&input.driver)
        .bind(&input.subnet)
        .fetch_one(&self.pool)
        .await
    }

    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM networks WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ─── ServiceNetworkRepo ──────────────────────────────────────────

pub struct PgServiceNetworkRepo {
    pub pool: PgPool,
}

impl PgServiceNetworkRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ServiceNetworkRepo for PgServiceNetworkRepo {
    async fn attach(&self, service_id: Uuid, network_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO service_networks (service_id, network_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(service_id)
        .bind(network_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn detach(&self, service_id: Uuid, network_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "DELETE FROM service_networks WHERE service_id = $1 AND network_id = $2",
        )
        .bind(service_id)
        .bind(network_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_by_service(&self, service_id: Uuid) -> Result<Vec<Network>, sqlx::Error> {
        sqlx::query_as::<_, Network>(
            r#"
            SELECT n.id, n.project_id, n.name, n.driver, n.subnet, n.docker_network_id, n.created_at
            FROM networks n
            INNER JOIN service_networks sn ON sn.network_id = n.id
            WHERE sn.service_id = $1
            ORDER BY n.name ASC
            "#,
        )
        .bind(service_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn list_by_network(&self, network_id: Uuid) -> Result<Vec<Service>, sqlx::Error> {
        sqlx::query_as::<_, Service>(
            r#"
            SELECT s.id, s.project_id, s.name, s.slug, s.type::text AS type,
                   s.directory_path, s.status, s.replicas, s.created_at, s.updated_at
            FROM services s
            INNER JOIN service_networks sn ON sn.service_id = s.id
            WHERE sn.network_id = $1
            ORDER BY s.name ASC
            "#,
        )
        .bind(network_id)
        .fetch_all(&self.pool)
        .await
    }
}

// ─── DomainRepo ──────────────────────────────────────────────────

pub struct PgDomainRepo {
    pub pool: PgPool,
}

impl PgDomainRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DomainRepo for PgDomainRepo {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Domain>, sqlx::Error> {
        sqlx::query_as::<_, Domain>(
            r#"
            SELECT id, service_id, hostname, tls_enabled, traefik_router_name, created_at
            FROM domains
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn find_by_hostname(&self, hostname: &str) -> Result<Option<Domain>, sqlx::Error> {
        sqlx::query_as::<_, Domain>(
            r#"
            SELECT id, service_id, hostname, tls_enabled, traefik_router_name, created_at
            FROM domains
            WHERE hostname = $1
            "#,
        )
        .bind(hostname)
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_by_service(&self, service_id: Uuid) -> Result<Vec<Domain>, sqlx::Error> {
        sqlx::query_as::<_, Domain>(
            r#"
            SELECT id, service_id, hostname, tls_enabled, traefik_router_name, created_at
            FROM domains
            WHERE service_id = $1
            ORDER BY hostname ASC
            "#,
        )
        .bind(service_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn create(&self, input: CreateDomainInput) -> Result<Domain, sqlx::Error> {
        sqlx::query_as::<_, Domain>(
            r#"
            INSERT INTO domains (service_id, hostname, tls_enabled, traefik_router_name)
            VALUES ($1, $2, $3, $4)
            RETURNING id, service_id, hostname, tls_enabled, traefik_router_name, created_at
            "#,
        )
        .bind(input.service_id)
        .bind(&input.hostname)
        .bind(input.tls_enabled)
        .bind(&input.traefik_router_name)
        .fetch_one(&self.pool)
        .await
    }

    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM domains WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ─── ContainerRepo ───────────────────────────────────────────────

pub struct PgContainerRepo {
    pub pool: PgPool,
}

impl PgContainerRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ContainerRepo for PgContainerRepo {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Container>, sqlx::Error> {
        sqlx::query_as::<_, Container>(
            r#"
            SELECT id, service_id, docker_container_id, docker_task_id, node_id,
                   replica_index, status::text AS status, status_message, image,
                   started_at, finished_at, exit_code, created_at, updated_at
            FROM containers
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn find_by_docker_id(
        &self,
        docker_container_id: &str,
    ) -> Result<Option<Container>, sqlx::Error> {
        sqlx::query_as::<_, Container>(
            r#"
            SELECT id, service_id, docker_container_id, docker_task_id, node_id,
                   replica_index, status::text AS status, status_message, image,
                   started_at, finished_at, exit_code, created_at, updated_at
            FROM containers
            WHERE docker_container_id = $1
            "#,
        )
        .bind(docker_container_id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_by_service(
        &self,
        service_id: Uuid,
        status_filter: Option<&str>,
    ) -> Result<Vec<Container>, sqlx::Error> {
        match status_filter {
            Some(status) => {
                sqlx::query_as::<_, Container>(
                    r#"
                    SELECT id, service_id, docker_container_id, docker_task_id, node_id,
                           replica_index, status::text AS status, status_message, image,
                           started_at, finished_at, exit_code, created_at, updated_at
                    FROM containers
                    WHERE service_id = $1 AND status::text = $2
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(service_id)
                .bind(status)
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, Container>(
                    r#"
                    SELECT id, service_id, docker_container_id, docker_task_id, node_id,
                           replica_index, status::text AS status, status_message, image,
                           started_at, finished_at, exit_code, created_at, updated_at
                    FROM containers
                    WHERE service_id = $1
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(service_id)
                .fetch_all(&self.pool)
                .await
            }
        }
    }

    async fn upsert_by_docker_id(
        &self,
        input: UpsertContainerInput,
    ) -> Result<Container, sqlx::Error> {
        sqlx::query_as::<_, Container>(
            r#"
            INSERT INTO containers (
                service_id, docker_container_id, docker_task_id, node_id,
                replica_index, status, status_message, image,
                started_at, finished_at, exit_code
            ) VALUES ($1, $2, $3, $4, $5, $6::container_status, $7, $8, $9, $10, $11)
            ON CONFLICT (docker_container_id) DO UPDATE SET
                service_id      = EXCLUDED.service_id,
                docker_task_id  = EXCLUDED.docker_task_id,
                node_id         = EXCLUDED.node_id,
                replica_index   = EXCLUDED.replica_index,
                status          = EXCLUDED.status,
                status_message  = EXCLUDED.status_message,
                image           = EXCLUDED.image,
                started_at      = EXCLUDED.started_at,
                finished_at     = EXCLUDED.finished_at,
                exit_code       = EXCLUDED.exit_code,
                updated_at      = NOW()
            RETURNING id, service_id, docker_container_id, docker_task_id, node_id,
                      replica_index, status::text AS status, status_message, image,
                      started_at, finished_at, exit_code, created_at, updated_at
            "#,
        )
        .bind(input.service_id)
        .bind(&input.docker_container_id)
        .bind(input.docker_task_id.as_deref())
        .bind(input.node_id.as_deref())
        .bind(input.replica_index)
        .bind(&input.status)
        .bind(input.status_message.as_deref())
        .bind(&input.image)
        .bind(input.started_at)
        .bind(input.finished_at)
        .bind(input.exit_code)
        .fetch_one(&self.pool)
        .await
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: &str,
        status_message: Option<&str>,
        finished_at: Option<DateTime<Utc>>,
        exit_code: Option<i32>,
    ) -> Result<Container, sqlx::Error> {
        sqlx::query_as::<_, Container>(
            r#"
            UPDATE containers
            SET status = $2::container_status,
                status_message = $3,
                finished_at = $4,
                exit_code = $5,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, service_id, docker_container_id, docker_task_id, node_id,
                      replica_index, status::text AS status, status_message, image,
                      started_at, finished_at, exit_code, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(status)
        .bind(status_message)
        .bind(finished_at)
        .bind(exit_code)
        .fetch_one(&self.pool)
        .await
    }

    async fn count_running_for_service(&self, service_id: Uuid) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM containers WHERE service_id = $1 AND status::text = 'running'",
        )
        .bind(service_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM containers WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ─── DockerEventRepo ─────────────────────────────────────────────

pub struct PgDockerEventRepo {
    pub pool: PgPool,
}

impl PgDockerEventRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DockerEventRepo for PgDockerEventRepo {
    async fn insert(&self, input: InsertDockerEventInput) -> Result<DockerEvent, sqlx::Error> {
        sqlx::query_as::<_, DockerEvent>(
            r#"
            INSERT INTO docker_events (event_type, action, actor_id, actor_attributes, scope, raw, received_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, event_type, action, actor_id, actor_attributes, scope, raw, received_at
            "#,
        )
        .bind(&input.event_type)
        .bind(&input.action)
        .bind(&input.actor_id)
        .bind(input.actor_attributes)
        .bind(input.scope.as_deref())
        .bind(input.raw)
        .bind(input.received_at)
        .fetch_one(&self.pool)
        .await
    }

    async fn list_paginated(
        &self,
        filter: DockerEventFilter,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DockerEvent>, sqlx::Error> {
        // Build dynamic filter — we use a parameterized approach with manual predicate building.
        // To keep this compile-time-safe without the macro, we build the SQL string and bind
        // parameters positionally. The number of optional params is small (5 max), so we enumerate
        // all branches to keep the code straightforward.
        //
        // Slot layout:
        //   $1 = limit, $2 = offset, then optional filters $3..
        //
        // Rather than dynamic SQL string mutation we use a pattern that covers the matrix of
        // present/absent filters via a WHERE clause with IS NOT DISTINCT FROM for optionals.
        // This keeps a single query but sqlx needs concrete binds, so we use the fully
        // parameterized form and pass NULL when filter is absent.

        sqlx::query_as::<_, DockerEvent>(
            r#"
            SELECT id, event_type, action, actor_id, actor_attributes, scope, raw, received_at
            FROM docker_events
            WHERE ($3::text IS NULL OR event_type = $3)
              AND ($4::text IS NULL OR action = $4)
              AND ($5::text IS NULL OR actor_id = $5)
              AND ($6::timestamptz IS NULL OR received_at >= $6)
              AND ($7::timestamptz IS NULL OR received_at <= $7)
            ORDER BY received_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .bind(filter.event_type.as_deref())
        .bind(filter.action.as_deref())
        .bind(filter.actor_id.as_deref())
        .bind(filter.from)
        .bind(filter.to)
        .fetch_all(&self.pool)
        .await
    }
}

// ─── DeploymentRepo ──────────────────────────────────────────────

pub struct PgDeploymentRepo {
    pub pool: PgPool,
}

impl PgDeploymentRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeploymentRepo for PgDeploymentRepo {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Deployment>, sqlx::Error> {
        sqlx::query_as::<_, Deployment>(
            r#"
            SELECT id, service_id, triggered_by, source_ref, status::text AS status,
                   created_at, finished_at
            FROM deployments
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_by_service(
        &self,
        service_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Deployment>, sqlx::Error> {
        sqlx::query_as::<_, Deployment>(
            r#"
            SELECT id, service_id, triggered_by, source_ref, status::text AS status,
                   created_at, finished_at
            FROM deployments
            WHERE service_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(service_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    async fn create(&self, input: CreateDeploymentInput) -> Result<Deployment, sqlx::Error> {
        sqlx::query_as::<_, Deployment>(
            r#"
            INSERT INTO deployments (service_id, triggered_by, source_ref)
            VALUES ($1, $2, $3)
            RETURNING id, service_id, triggered_by, source_ref, status::text AS status,
                      created_at, finished_at
            "#,
        )
        .bind(input.service_id)
        .bind(&input.triggered_by)
        .bind(&input.source_ref)
        .fetch_one(&self.pool)
        .await
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: &str,
        finished_at: Option<DateTime<Utc>>,
    ) -> Result<Deployment, sqlx::Error> {
        sqlx::query_as::<_, Deployment>(
            r#"
            UPDATE deployments
            SET status = $2::deployment_status, finished_at = $3
            WHERE id = $1
            RETURNING id, service_id, triggered_by, source_ref, status::text AS status,
                      created_at, finished_at
            "#,
        )
        .bind(id)
        .bind(status)
        .bind(finished_at)
        .fetch_one(&self.pool)
        .await
    }

    async fn latest_success_for_service(
        &self,
        service_id: Uuid,
    ) -> Result<Option<Deployment>, sqlx::Error> {
        sqlx::query_as::<_, Deployment>(
            r#"
            SELECT id, service_id, triggered_by, source_ref, status::text AS status,
                   created_at, finished_at
            FROM deployments
            WHERE service_id = $1 AND status::text = 'success'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(service_id)
        .fetch_optional(&self.pool)
        .await
    }
}

// ─── DeploymentStepRepo ──────────────────────────────────────────

pub struct PgDeploymentStepRepo {
    pub pool: PgPool,
}

impl PgDeploymentStepRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeploymentStepRepo for PgDeploymentStepRepo {
    async fn list_by_deployment(
        &self,
        deployment_id: Uuid,
    ) -> Result<Vec<DeploymentStep>, sqlx::Error> {
        sqlx::query_as::<_, DeploymentStep>(
            r#"
            SELECT id, deployment_id, name, status::text AS status, order_index,
                   started_at, finished_at
            FROM deployment_steps
            WHERE deployment_id = $1
            ORDER BY order_index ASC
            "#,
        )
        .bind(deployment_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn create(
        &self,
        input: CreateDeploymentStepInput,
    ) -> Result<DeploymentStep, sqlx::Error> {
        sqlx::query_as::<_, DeploymentStep>(
            r#"
            INSERT INTO deployment_steps (deployment_id, name, order_index)
            VALUES ($1, $2, $3)
            RETURNING id, deployment_id, name, status::text AS status, order_index,
                      started_at, finished_at
            "#,
        )
        .bind(input.deployment_id)
        .bind(&input.name)
        .bind(input.order_index)
        .fetch_one(&self.pool)
        .await
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: &str,
        started_at: Option<DateTime<Utc>>,
        finished_at: Option<DateTime<Utc>>,
    ) -> Result<DeploymentStep, sqlx::Error> {
        sqlx::query_as::<_, DeploymentStep>(
            r#"
            UPDATE deployment_steps
            SET status = $2::step_status, started_at = $3, finished_at = $4
            WHERE id = $1
            RETURNING id, deployment_id, name, status::text AS status, order_index,
                      started_at, finished_at
            "#,
        )
        .bind(id)
        .bind(status)
        .bind(started_at)
        .bind(finished_at)
        .fetch_one(&self.pool)
        .await
    }
}

// ─── DeploymentLogRepo ───────────────────────────────────────────

pub struct PgDeploymentLogRepo {
    pub pool: PgPool,
}

impl PgDeploymentLogRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeploymentLogRepo for PgDeploymentLogRepo {
    async fn list_by_deployment(
        &self,
        deployment_id: Uuid,
        step_id: Option<Uuid>,
        level: Option<&str>,
    ) -> Result<Vec<DeploymentLog>, sqlx::Error> {
        sqlx::query_as::<_, DeploymentLog>(
            r#"
            SELECT id, deployment_id, step_id, level::text AS level, message, timestamp
            FROM deployment_logs
            WHERE deployment_id = $1
              AND ($2::uuid IS NULL OR step_id = $2)
              AND ($3::text IS NULL OR level::text = $3)
            ORDER BY timestamp ASC
            "#,
        )
        .bind(deployment_id)
        .bind(step_id)
        .bind(level)
        .fetch_all(&self.pool)
        .await
    }

    async fn insert_batch(
        &self,
        logs: Vec<InsertDeploymentLogInput>,
    ) -> Result<Vec<DeploymentLog>, sqlx::Error> {
        let mut results = Vec::with_capacity(logs.len());
        for log in logs {
            let record = sqlx::query_as::<_, DeploymentLog>(
                r#"
                INSERT INTO deployment_logs (deployment_id, step_id, level, message, timestamp)
                VALUES ($1, $2, $3::log_level, $4, $5)
                RETURNING id, deployment_id, step_id, level::text AS level, message, timestamp
                "#,
            )
            .bind(log.deployment_id)
            .bind(log.step_id)
            .bind(&log.level)
            .bind(&log.message)
            .bind(log.timestamp)
            .fetch_one(&self.pool)
            .await?;
            results.push(record);
        }
        Ok(results)
    }
}

// ─── TopologyEdgeRepo ────────────────────────────────────────────

pub struct PgTopologyEdgeRepo {
    pub pool: PgPool,
}

impl PgTopologyEdgeRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TopologyEdgeRepo for PgTopologyEdgeRepo {
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<TopologyEdge>, sqlx::Error> {
        sqlx::query_as::<_, TopologyEdge>(
            r#"
            SELECT id, project_id, source_node_id, target_node_id, edge_type, created_at
            FROM topology_edges
            WHERE project_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
    }

    async fn upsert_edge(
        &self,
        input: UpsertTopologyEdgeInput,
    ) -> Result<TopologyEdge, sqlx::Error> {
        // topology_edges has no unique constraint on (project_id, source_node_id, target_node_id),
        // so we update any existing matching row first; if none exists, we insert a new one.
        let existing = sqlx::query_as::<_, TopologyEdge>(
            r#"
            UPDATE topology_edges
            SET edge_type = $4
            WHERE project_id = $1 AND source_node_id = $2 AND target_node_id = $3
            RETURNING id, project_id, source_node_id, target_node_id, edge_type, created_at
            "#,
        )
        .bind(input.project_id)
        .bind(&input.source_node_id)
        .bind(&input.target_node_id)
        .bind(&input.edge_type)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(edge) = existing {
            return Ok(edge);
        }

        sqlx::query_as::<_, TopologyEdge>(
            r#"
            INSERT INTO topology_edges (project_id, source_node_id, target_node_id, edge_type)
            VALUES ($1, $2, $3, $4)
            RETURNING id, project_id, source_node_id, target_node_id, edge_type, created_at
            "#,
        )
        .bind(input.project_id)
        .bind(&input.source_node_id)
        .bind(&input.target_node_id)
        .bind(&input.edge_type)
        .fetch_one(&self.pool)
        .await
    }

    async fn delete_edge(
        &self,
        project_id: Uuid,
        source_node_id: &str,
        target_node_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM topology_edges
            WHERE project_id = $1 AND source_node_id = $2 AND target_node_id = $3
            "#,
        )
        .bind(project_id)
        .bind(source_node_id)
        .bind(target_node_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_all_for_project(&self, project_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM topology_edges WHERE project_id = $1")
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ─── SystemConfigRepo ────────────────────────────────────────────

pub struct PgSystemConfigRepo {
    pub pool: PgPool,
}

impl PgSystemConfigRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SystemConfigRepo for PgSystemConfigRepo {
    async fn get(&self, key: &str) -> Result<Option<SystemConfig>, sqlx::Error> {
        sqlx::query_as::<_, SystemConfig>(
            "SELECT key, value, updated_at FROM system_config WHERE key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
    }

    async fn set(&self, key: &str, value: serde_json::Value) -> Result<SystemConfig, sqlx::Error> {
        sqlx::query_as::<_, SystemConfig>(
            r#"
            INSERT INTO system_config (key, value)
            VALUES ($1, $2)
            ON CONFLICT (key) DO UPDATE
                SET value = EXCLUDED.value,
                    updated_at = NOW()
            RETURNING key, value, updated_at
            "#,
        )
        .bind(key)
        .bind(value)
        .fetch_one(&self.pool)
        .await
    }
}
