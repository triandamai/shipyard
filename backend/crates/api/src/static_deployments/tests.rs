#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::TempDir;
    use uuid::Uuid;
    use zip::write::SimpleFileOptions;

    use crate::static_deployments::models::{
        validate_alias_name, validate_project_slug, StaticDeploymentStatus,
    };
    use crate::static_deployments::serving::{compute_cache_control, resolve_path_in_deployment, ServeOptions};
    use crate::static_deployments::storage::{LocalStorage, StaticStorageBackend};
    use crate::static_deployments::upload::{ExtractionLimits, ZipExtractor};
    use crate::static_deployments::verify::{verify_extracted_deployment, VerificationConfig};

    #[test]
    fn test_deployment_status_transitions() {
        assert!(StaticDeploymentStatus::Pending.can_transition_to(StaticDeploymentStatus::Uploading));
        assert!(StaticDeploymentStatus::Uploading.can_transition_to(StaticDeploymentStatus::Verifying));
        assert!(StaticDeploymentStatus::Verifying.can_transition_to(StaticDeploymentStatus::Ready));

        assert!(StaticDeploymentStatus::Pending.can_transition_to(StaticDeploymentStatus::Failed));
        assert!(StaticDeploymentStatus::Uploading.can_transition_to(StaticDeploymentStatus::Failed));
        assert!(StaticDeploymentStatus::Verifying.can_transition_to(StaticDeploymentStatus::Failed));

        assert!(!StaticDeploymentStatus::Ready.can_transition_to(StaticDeploymentStatus::Pending));
        assert!(!StaticDeploymentStatus::Ready.can_transition_to(StaticDeploymentStatus::Uploading));
        assert!(!StaticDeploymentStatus::Ready.can_transition_to(StaticDeploymentStatus::Verifying));

        assert!(StaticDeploymentStatus::Ready.can_transition_to(StaticDeploymentStatus::Deleting));
        assert!(StaticDeploymentStatus::Failed.can_transition_to(StaticDeploymentStatus::Deleting));
        assert!(StaticDeploymentStatus::Deleting.can_transition_to(StaticDeploymentStatus::Deleted));

        assert!(!StaticDeploymentStatus::Deleted.can_transition_to(StaticDeploymentStatus::Ready));
    }

    #[test]
    fn test_deployment_is_servable() {
        assert!(!StaticDeploymentStatus::Pending.is_servable());
        assert!(!StaticDeploymentStatus::Uploading.is_servable());
        assert!(!StaticDeploymentStatus::Verifying.is_servable());
        assert!(StaticDeploymentStatus::Ready.is_servable());
        assert!(!StaticDeploymentStatus::Failed.is_servable());
        assert!(!StaticDeploymentStatus::Deleting.is_servable());
        assert!(!StaticDeploymentStatus::Deleted.is_servable());
    }

    #[test]
    fn test_slug_and_alias_validation() {
        assert!(validate_project_slug("my-app").is_ok());
        assert!(validate_project_slug("web-123").is_ok());
        assert!(validate_project_slug("").is_err());
        assert!(validate_project_slug("My-App").is_err());
        assert!(validate_project_slug("-leading").is_err());
        assert!(validate_project_slug("trailing-").is_err());
        assert!(validate_project_slug("has spaces").is_err());
        assert!(validate_project_slug("bad/slash").is_err());

        assert!(validate_alias_name("production").is_ok());
        assert!(validate_alias_name("staging").is_ok());
        assert!(validate_alias_name("v1-0").is_ok());
        assert!(validate_alias_name("").is_err());
        assert!(validate_alias_name("PROD").is_err());
        assert!(validate_alias_name("has.dot").is_err());
    }

    fn create_test_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (name, content) in entries {
                if name.ends_with('/') {
                    writer.add_directory(*name, options).unwrap();
                } else {
                    writer.start_file(*name, options).unwrap();
                    writer.write_all(content).unwrap();
                }
            }
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn test_zip_extraction_success_and_single_root_unwrap() {
        let tmp = TempDir::new().unwrap();
        let zip_bytes = create_test_zip(&[
            ("dist/", b""),
            ("dist/index.html", b"<html><body>Hello</body></html>"),
            ("dist/assets/app.js", b"console.log('hi');"),
        ]);

        let limits = ExtractionLimits {
            max_compressed_bytes: 10 * 1024 * 1024,
            max_uncompressed_bytes: 50 * 1024 * 1024,
            max_files: 100,
        };

        let extractor = ZipExtractor::new(limits);
        let dest = tmp.path().join("extracted");
        let result = extractor.extract_from_bytes(&zip_bytes, &dest).unwrap();

        assert_eq!(result.file_count, 2);
        assert!(dest.join("index.html").exists());
        assert!(dest.join("assets/app.js").exists());
        assert_eq!(std::fs::read_to_string(dest.join("index.html")).unwrap(), "<html><body>Hello</body></html>");
        assert!(!result.checksum_sha256.is_empty());
    }

    #[test]
    fn test_zip_extraction_rejects_path_traversal() {
        let tmp = TempDir::new().unwrap();
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            writer.start_file("../outside.txt", options).unwrap();
            writer.write_all(b"malicious").unwrap();
            writer.finish().unwrap();
        }

        let extractor = ZipExtractor::default();
        let dest = tmp.path().join("extracted");
        let result = extractor.extract_from_bytes(&buf, &dest);
        assert!(result.is_err());
    }

    #[test]
    fn test_zip_extraction_rejects_oversized_uncompressed() {
        let tmp = TempDir::new().unwrap();
        let payload = vec![b'a'; 10_000];
        let zip_bytes = create_test_zip(&[("index.html", &payload)]);

        let limits = ExtractionLimits {
            max_compressed_bytes: 10 * 1024 * 1024,
            max_uncompressed_bytes: 5_000,
            max_files: 10,
        };

        let extractor = ZipExtractor::new(limits);
        let dest = tmp.path().join("extracted");
        let result = extractor.extract_from_bytes(&zip_bytes, &dest);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_local_storage_backend() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path().to_str().unwrap());

        let project_id = Uuid::now_v7();
        let deployment_id = Uuid::now_v7();
        let dep_dir = storage.deployment_dir(project_id, deployment_id);

        std::fs::create_dir_all(&dep_dir).unwrap();
        std::fs::write(dep_dir.join("index.html"), b"<h1>Hello World</h1>").unwrap();
        std::fs::create_dir_all(dep_dir.join("assets")).unwrap();
        std::fs::write(dep_dir.join("assets/style.css"), b"body { color: red; }").unwrap();

        assert!(storage.verify_deployment(project_id, deployment_id, "index.html").await.unwrap());
        assert!(!storage.verify_deployment(project_id, deployment_id, "missing.html").await.unwrap());

        let (bytes, mime) = storage.read_file(project_id, deployment_id, "index.html").await.unwrap().unwrap();
        assert_eq!(bytes, b"<h1>Hello World</h1>");
        assert_eq!(mime, "text/html; charset=utf-8");

        let (bytes_css, mime_css) = storage.read_file(project_id, deployment_id, "assets/style.css").await.unwrap().unwrap();
        assert_eq!(bytes_css, b"body { color: red; }");
        assert_eq!(mime_css, "text/css; charset=utf-8");

        let traversal = storage.read_file(project_id, deployment_id, "../outside.txt").await;
        assert!(traversal.is_err());

        storage.delete_deployment(project_id, deployment_id).await.unwrap();
        assert!(!dep_dir.exists());
    }

    #[test]
    fn test_verification_gate() {
        let tmp = TempDir::new().unwrap();
        let cfg = VerificationConfig::default();

        // Empty directory fails verification
        assert!(verify_extracted_deployment(tmp.path(), "index.html", &cfg).is_err());

        // Missing entry file fails verification
        std::fs::write(tmp.path().join("other.txt"), b"foo").unwrap();
        assert!(verify_extracted_deployment(tmp.path(), "index.html", &cfg).is_err());

        // Zero-byte entry file fails verification
        std::fs::write(tmp.path().join("index.html"), b"").unwrap();
        assert!(verify_extracted_deployment(tmp.path(), "index.html", &cfg).is_err());

        // Valid non-empty entry file succeeds
        std::fs::write(tmp.path().join("index.html"), b"<!doctype html><html></html>").unwrap();
        assert!(verify_extracted_deployment(tmp.path(), "index.html", &cfg).is_ok());
    }

    #[test]
    fn test_cache_control_headers() {
        // HTML is always no-store
        assert_eq!(compute_cache_control("index.html", false), "no-cache, no-store, must-revalidate");
        assert_eq!(compute_cache_control("about.html", false), "no-cache, no-store, must-revalidate");

        // Mutable aliases requests are always no-store
        assert_eq!(compute_cache_control("assets/app-1234abcd.js", true), "no-cache, no-store, must-revalidate");

        // Immutable hashed assets
        assert_eq!(compute_cache_control("assets/app-a1b2c3d4.js", false), "public, max-age=31536000, immutable");
        assert_eq!(compute_cache_control("chunk.8f7e6d5c.css", false), "public, max-age=31536000, immutable");

        // Non-hashed assets
        assert_eq!(compute_cache_control("favicon.ico", false), "public, max-age=3600, must-revalidate");
        assert_eq!(compute_cache_control("images/logo.png", false), "public, max-age=3600, must-revalidate");
    }

    #[tokio::test]
    async fn test_path_resolution_and_spa_fallback() {
        let tmp = TempDir::new().unwrap();
        let storage = LocalStorage::new(tmp.path().to_str().unwrap());

        let project_id = Uuid::now_v7();
        let deployment_id = Uuid::now_v7();
        let dep_dir = storage.deployment_dir(project_id, deployment_id);
        std::fs::create_dir_all(&dep_dir).unwrap();
        std::fs::write(dep_dir.join("index.html"), b"<h1>Home</h1>").unwrap();
        std::fs::create_dir_all(dep_dir.join("about")).unwrap();
        std::fs::write(dep_dir.join("about/index.html"), b"<h1>About</h1>").unwrap();

        let opts_spa = ServeOptions {
            spa_fallback: true,
            entry_file: "index.html".to_string(),
            is_alias: false,
        };

        // Root resolves to index.html
        let res = resolve_path_in_deployment(&storage, project_id, deployment_id, "/", &opts_spa).await.unwrap();
        assert_eq!(res.0, b"<h1>Home</h1>");

        // Nested directory resolves to about/index.html
        let res_about = resolve_path_in_deployment(&storage, project_id, deployment_id, "/about", &opts_spa).await.unwrap();
        assert_eq!(res_about.0, b"<h1>About</h1>");

        // Unknown route with SPA fallback resolves to index.html
        let res_spa = resolve_path_in_deployment(&storage, project_id, deployment_id, "/dashboard/settings", &opts_spa).await.unwrap();
        assert_eq!(res_spa.0, b"<h1>Home</h1>");

        // Unknown route without SPA fallback returns None
        let opts_no_spa = ServeOptions {
            spa_fallback: false,
            entry_file: "index.html".to_string(),
            is_alias: false,
        };
        let res_404 = resolve_path_in_deployment(&storage, project_id, deployment_id, "/non-existent", &opts_no_spa).await;
        assert!(res_404.is_none());
    }
}
