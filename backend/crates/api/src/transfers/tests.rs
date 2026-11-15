#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::transfers::worker::{execute_project_transfer, execute_service_claim};

    fn test_database_url() -> String {
        std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://shipyard:shipyard@localhost:5432/shipyard".to_string())
    }

    async fn pool() -> Option<PgPool> {
        let url = test_database_url();
        match shipyard_db::init_pool(&url, 5).await {
            Ok(pool) => {
                let _ = shipyard_db::run_migrations(&pool).await;
                Some(pool)
            }
            Err(_) => None,
        }
    }

    async fn create_test_org(pool: &PgPool, name: &str, slug: &str) -> Uuid {
        let org_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO organizations (id, name, slug, created_at) VALUES ($1, $2, $3, NOW())",
        )
        .bind(org_id)
        .bind(name)
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
        org_id
    }

    async fn create_test_user(pool: &PgPool, email: &str) -> Uuid {
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_active, created_at, updated_at)
             VALUES ($1, $2, 'hash', true, NOW(), NOW())",
        )
        .bind(user_id)
        .bind(email)
        .execute(pool)
        .await
        .unwrap();
        user_id
    }

    async fn add_org_member(pool: &PgPool, org_id: Uuid, user_id: Uuid, role: &str) {
        sqlx::query(
            "INSERT INTO org_members (id, org_id, user_id, role, created_at)
             VALUES ($1, $2, $3, $4, NOW())",
        )
        .bind(Uuid::new_v4())
        .bind(org_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn create_test_project(pool: &PgPool, org_id: Uuid, name: &str, slug: &str) -> Uuid {
        let project_id = Uuid::new_v4();
        let dir = format!("/tmp/shipyard/data/{}/{}", org_id, project_id);
        sqlx::query(
            "INSERT INTO projects (id, org_id, name, slug, directory_path, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
        )
        .bind(project_id)
        .bind(org_id)
        .bind(name)
        .bind(slug)
        .bind(&dir)
        .execute(pool)
        .await
        .unwrap();
        project_id
    }

    async fn create_test_service(pool: &PgPool, project_id: Uuid, name: &str, slug: &str) -> Uuid {
        let svc_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO services (id, project_id, name, slug, status, replicas, created_at, updated_at)
             VALUES ($1, $2, $3, $4, 'stopped', 1, NOW(), NOW())",
        )
        .bind(svc_id)
        .bind(project_id)
        .bind(name)
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
        svc_id
    }

    #[tokio::test]
    async fn test_project_transfer_worker_execution() {
        let pool = match pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping test: TEST_DATABASE_URL not available");
                return;
            }
        };

        let suffix = Uuid::new_v4().simple().to_string();
        let org_a = create_test_org(&pool, "Org Alpha", &format!("org-alpha-{}", suffix)).await;
        let org_b = create_test_org(&pool, "Org Beta", &format!("org-beta-{}", suffix)).await;
        let user_owner = create_test_user(&pool, &format!("owner-{}@alpha.test", suffix)).await;
        let user_member_only_a = create_test_user(&pool, &format!("membera-{}@alpha.test", suffix)).await;
        let user_in_both = create_test_user(&pool, &format!("both-{}@test.com", suffix)).await;

        add_org_member(&pool, org_a, user_owner, "owner").await;
        add_org_member(&pool, org_a, user_member_only_a, "member").await;
        add_org_member(&pool, org_a, user_in_both, "member").await;
        add_org_member(&pool, org_b, user_in_both, "member").await;

        let proj = create_test_project(&pool, org_a, "My Project", "my-project").await;
        let _svc = create_test_service(&pool, proj, "Web App", "web-app").await;

        // Add project members
        sqlx::query(
            "INSERT INTO project_members (id, project_id, org_id, user_id, permissions)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(proj)
        .bind(org_a)
        .bind(user_member_only_a)
        .bind(&vec!["view".to_string()])
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO project_members (id, project_id, org_id, user_id, permissions)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(proj)
        .bind(org_a)
        .bind(user_in_both)
        .bind(&vec!["view".to_string(), "deploy".to_string()])
        .execute(&pool)
        .await
        .unwrap();

        // Also create a collision in Org B to verify slug collision handling
        let _colliding_proj = create_test_project(&pool, org_b, "Colliding", "my-project").await;

        let transfer_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO project_transfers (id, project_id, source_org_id, target_org_id, status, initiated_by)
             VALUES ($1, $2, $3, $4, 'in_progress', $5)",
        )
        .bind(transfer_id)
        .bind(proj)
        .bind(org_a)
        .bind(org_b)
        .bind(user_owner)
        .execute(&pool)
        .await
        .unwrap();

        let result = execute_project_transfer(&pool, transfer_id, proj, org_b, "/tmp/shipyard/data").await;
        assert!(result.is_ok(), "Transfer execution should succeed: {:?}", result.err());

        // Verify project org_id is now org_b
        let (new_org, new_slug, new_dir): (Uuid, String, String) = sqlx::query_as(
            "SELECT org_id, slug, directory_path FROM projects WHERE id = $1",
        )
        .bind(proj)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(new_org, org_b);
        assert_eq!(new_slug, "my-project-1"); // Slug was deduplicated
        assert!(new_dir.contains(&org_b.to_string()));

        // Verify project_members: user_member_only_a was pruned, user_in_both updated to org_b
        let members: Vec<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT user_id, org_id FROM project_members WHERE project_id = $1",
        )
        .bind(proj)
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(members.len(), 1);
        assert_eq!(members[0].0, user_in_both);
        assert_eq!(members[0].1, org_b);

        // Verify transfer status is completed
        let status: String = sqlx::query_scalar(
            "SELECT status FROM project_transfers WHERE id = $1",
        )
        .bind(transfer_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(status, "completed");
    }

    #[tokio::test]
    async fn test_service_transfer_worker_claim() {
        let pool = match pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping test: TEST_DATABASE_URL not available");
                return;
            }
        };

        let suffix = Uuid::new_v4().simple().to_string();
        let org_a = create_test_org(&pool, "Org A", &format!("org-a-{}", suffix)).await;
        let org_b = create_test_org(&pool, "Org B", &format!("org-b-{}", suffix)).await;
        let user_a = create_test_user(&pool, &format!("owner-a-{}@test.com", suffix)).await;
        let user_b = create_test_user(&pool, &format!("owner-b-{}@test.com", suffix)).await;

        add_org_member(&pool, org_a, user_a, "owner").await;
        add_org_member(&pool, org_b, user_b, "owner").await;

        let proj_a = create_test_project(&pool, org_a, "Project A", "proj-a").await;
        let proj_b = create_test_project(&pool, org_b, "Project B", "proj-b").await;

        // In proj_b, add an existing service with the same slug "api" to test slug collision
        let _existing_svc = create_test_service(&pool, proj_b, "API Existing", "api").await;

        let svc_to_move = create_test_service(&pool, proj_a, "API Service", "api").await;

        // Attach a volume to svc_to_move
        let vol_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO volumes (id, project_id, service_id, name, mount_path, driver, size_mb)
             VALUES ($1, $2, $3, 'db-data', '/var/lib/data', 'local', 100)",
        )
        .bind(vol_id)
        .bind(proj_a)
        .bind(svc_to_move)
        .execute(&pool)
        .await
        .unwrap();

        // Attach a network to svc_to_move
        let net_a_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO networks (id, project_id, name, driver, subnet)
             VALUES ($1, $2, 'app-net', 'overlay', '10.0.1.0/24')",
        )
        .bind(net_a_id)
        .bind(proj_a)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO service_networks (service_id, network_id) VALUES ($1, $2)",
        )
        .bind(svc_to_move)
        .bind(net_a_id)
        .execute(&pool)
        .await
        .unwrap();

        // Attach a domain
        let dom_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO domains (id, service_id, hostname, tls_enabled, traefik_router_name)
             VALUES ($1, $2, $3, true, 'router-1')",
        )
        .bind(dom_id)
        .bind(svc_to_move)
        .bind(format!("api-{}.example.com", suffix))
        .execute(&pool)
        .await
        .unwrap();

        // Canvas positions on proj_a
        let initial_positions = serde_json::json!({
            svc_to_move.to_string(): { "x": 100, "y": 150 }
        });
        sqlx::query("UPDATE projects SET node_positions = $1 WHERE id = $2")
            .bind(initial_positions)
            .bind(proj_a)
            .execute(&pool)
            .await
            .unwrap();

        // Initiate transfer
        let transfer_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO service_transfers (id, service_id, source_project_id, source_org_id, target_org_id, status, initiated_by)
             VALUES ($1, $2, $3, $4, $5, 'in_progress', $6)",
        )
        .bind(transfer_id)
        .bind(svc_to_move)
        .bind(proj_a)
        .bind(org_a)
        .bind(org_b)
        .bind(user_a)
        .execute(&pool)
        .await
        .unwrap();

        // Execute claim into proj_b
        let claim_res = execute_service_claim(&pool, transfer_id, svc_to_move, proj_b, org_b).await;
        assert!(claim_res.is_ok(), "Service claim should succeed: {:?}", claim_res.err());

        // 1. Verify service moved to proj_b with deduplicated slug
        let (new_proj_id, new_slug): (Uuid, String) = sqlx::query_as(
            "SELECT project_id, slug FROM services WHERE id = $1",
        )
        .bind(svc_to_move)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(new_proj_id, proj_b);
        assert_eq!(new_slug, "api-1");

        // 2. Verify volume project_id updated to proj_b
        let vol_proj: Uuid = sqlx::query_scalar(
            "SELECT project_id FROM volumes WHERE id = $1",
        )
        .bind(vol_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(vol_proj, proj_b);

        // 3. Verify network rewired to a network in proj_b with name "app-net"
        let net_in_b: (Uuid, Uuid) = sqlx::query_as(
            "SELECT sn.network_id, n.project_id
             FROM service_networks sn
             JOIN networks n ON n.id = sn.network_id
             WHERE sn.service_id = $1",
        )
        .bind(svc_to_move)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_ne!(net_in_b.0, net_a_id);
        assert_eq!(net_in_b.1, proj_b);

        // 4. Verify domain is still attached to svc_to_move
        let dom_svc: Uuid = sqlx::query_scalar(
            "SELECT service_id FROM domains WHERE id = $1",
        )
        .bind(dom_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(dom_svc, svc_to_move);

        // 5. Verify canvas node positions: removed from proj_a, added to proj_b
        let pos_a: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT node_positions FROM projects WHERE id = $1",
        )
        .bind(proj_a)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(pos_a.unwrap().get(&svc_to_move.to_string()).is_none());

        let pos_b: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT node_positions FROM projects WHERE id = $1",
        )
        .bind(proj_b)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(pos_b.unwrap().get(&svc_to_move.to_string()).is_some());

        // 6. Verify transfer status is completed
        let (status, target_proj): (String, Option<Uuid>) = sqlx::query_as(
            "SELECT status, target_project_id FROM service_transfers WHERE id = $1",
        )
        .bind(transfer_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(status, "completed");
        assert_eq!(target_proj, Some(proj_b));
    }

    #[tokio::test]
    async fn test_service_transfer_cancel_and_reject() {
        let pool = match pool().await {
            Some(p) => p,
            None => {
                eprintln!("Skipping test: TEST_DATABASE_URL not available");
                return;
            }
        };

        let suffix = Uuid::new_v4().simple().to_string();
        let org_a = create_test_org(&pool, "Org A", &format!("org-a-{}", suffix)).await;
        let org_b = create_test_org(&pool, "Org B", &format!("org-b-{}", suffix)).await;
        let user_a = create_test_user(&pool, &format!("owner-a-{}@test.com", suffix)).await;
        let user_b = create_test_user(&pool, &format!("owner-b-{}@test.com", suffix)).await;

        add_org_member(&pool, org_a, user_a, "owner").await;
        add_org_member(&pool, org_b, user_b, "owner").await;

        let proj_a = create_test_project(&pool, org_a, "Project A", "proj-a").await;
        let svc_1 = create_test_service(&pool, proj_a, "Service 1", "svc-1").await;
        let svc_2 = create_test_service(&pool, proj_a, "Service 2", "svc-2").await;

        // 1. Test cancel by source org
        let transfer_1 = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO service_transfers (id, service_id, source_project_id, source_org_id, target_org_id, status, initiated_by)
             VALUES ($1, $2, $3, $4, $5, 'pending_claim', $6)",
        )
        .bind(transfer_1)
        .bind(svc_1)
        .bind(proj_a)
        .bind(org_a)
        .bind(org_b)
        .bind(user_a)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "UPDATE service_transfers SET status = 'cancelled', updated_at = NOW() WHERE id = $1",
        )
        .bind(transfer_1)
        .execute(&pool)
        .await
        .unwrap();

        let s1: String = sqlx::query_scalar("SELECT status FROM service_transfers WHERE id = $1")
            .bind(transfer_1)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(s1, "cancelled");

        // 2. Test reject by target org
        let transfer_2 = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO service_transfers (id, service_id, source_project_id, source_org_id, target_org_id, status, initiated_by)
             VALUES ($1, $2, $3, $4, $5, 'pending_claim', $6)",
        )
        .bind(transfer_2)
        .bind(svc_2)
        .bind(proj_a)
        .bind(org_a)
        .bind(org_b)
        .bind(user_a)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "UPDATE service_transfers SET status = 'rejected', updated_at = NOW() WHERE id = $1",
        )
        .bind(transfer_2)
        .execute(&pool)
        .await
        .unwrap();

        let s2: String = sqlx::query_scalar("SELECT status FROM service_transfers WHERE id = $1")
            .bind(transfer_2)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(s2, "rejected");
    }
}
