/// OCI annotation key that carries the Shipyard artifact kind.
pub const ANNOTATION_KIND: &str = "org.shipyard.artifact.kind";

/// Known artifact kind identifiers.
pub mod kind {
    pub const DOCKER_IMAGE:    &str = "docker_image";
    pub const STATIC_BUNDLE:   &str = "static_bundle";
    pub const EDGE_FUNCTION:   &str = "edge_function";
    pub const BUILD_CACHE:     &str = "build_cache";
    pub const WASM_MODULE:     &str = "wasm_module";
    pub const DB_SNAPSHOT:     &str = "db_snapshot";
    pub const GENERIC_BLOB:    &str = "generic_blob";
}

/// Extract the artifact kind and kind-specific metadata from an OCI manifest's annotations.
/// Falls back to `docker_image` if no annotation is present (standard Docker push).
///
/// Returns `(kind: String, metadata: serde_json::Value)`.
pub fn extract_kind_metadata(manifest: &serde_json::Value) -> (String, serde_json::Value) {
    let annotations = manifest
        .get("annotations")
        .and_then(|a| a.as_object());

    let kind = annotations
        .and_then(|a| a.get(ANNOTATION_KIND))
        .and_then(|v| v.as_str())
        .unwrap_or(kind::DOCKER_IMAGE)
        .to_string();

    let metadata = build_metadata(&kind, manifest, annotations);
    (kind, metadata)
}

fn build_metadata(
    kind: &str,
    manifest: &serde_json::Value,
    annotations: Option<&serde_json::Map<String, serde_json::Value>>,
) -> serde_json::Value {
    let get = |key: &str| -> Option<String> {
        annotations?.get(key)?.as_str().map(|s| s.to_string())
    };

    match kind {
        kind::STATIC_BUNDLE => serde_json::json!({
            "framework":     get("org.shipyard.framework"),
            "build_command": get("org.shipyard.build_command"),
            "output_dir":    get("org.shipyard.output_dir"),
            "file_count":    manifest.get("layers")
                                 .and_then(|l| l.as_array())
                                 .map(|l| l.len())
        }),
        kind::EDGE_FUNCTION => serde_json::json!({
            "runtime":     get("org.shipyard.runtime").unwrap_or_else(|| "js".into()),
            "entry_point": get("org.shipyard.entry_point"),
            "bundler":     get("org.shipyard.bundler"),
        }),
        kind::BUILD_CACHE => serde_json::json!({
            "tool":           get("org.shipyard.cache_tool"),
            "lock_file_hash": get("org.shipyard.lock_file_hash"),
            "cache_key":      get("org.shipyard.cache_key"),
        }),
        kind::DOCKER_IMAGE => {
            serde_json::json!({
                "architecture": get("org.opencontainers.image.architecture"),
                "os":           get("org.opencontainers.image.os"),
                "layers": manifest.get("layers")
                    .and_then(|l| l.as_array())
                    .map(|l| l.len())
                    .unwrap_or(0),
            })
        }
        _ => serde_json::json!({}),
    }
}

/// Build a minimal OCI manifest wrapping a single artifact blob.
/// Used by the build engine to push static_bundle and edge_function artifacts
/// through the standard OCI push flow.
pub fn wrap_single_blob(
    blob_digest: &str,
    blob_size: i64,
    kind: &str,
    extra_annotations: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let mut annotations = extra_annotations;
    annotations.insert(
        ANNOTATION_KIND.to_string(),
        serde_json::Value::String(kind.to_string()),
    );

    serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.empty.v1+json",
            "digest":    "sha256:44136fa355ba77b9ad7b35b12de09c5ef5b26f90b5a65d0b5a7e2a40a83a4e3",
            "size":      2
        },
        "layers": [{
            "mediaType": "application/octet-stream",
            "digest":    blob_digest,
            "size":      blob_size
        }],
        "annotations": annotations
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_kind_metadata_defaults_to_docker_image_when_no_annotation() {
        let manifest = serde_json::json!({ "layers": [{}, {}, {}] });
        let (kind, meta) = extract_kind_metadata(&manifest);
        assert_eq!(kind, kind::DOCKER_IMAGE);
        assert_eq!(meta["layers"], 3);
    }

    #[test]
    fn extract_kind_metadata_reads_static_bundle_annotations() {
        let manifest = serde_json::json!({
            "layers": [{}, {}],
            "annotations": {
                ANNOTATION_KIND: kind::STATIC_BUNDLE,
                "org.shipyard.framework": "sveltekit",
                "org.shipyard.build_command": "npm run build",
                "org.shipyard.output_dir": "build",
            }
        });
        let (kind, meta) = extract_kind_metadata(&manifest);
        assert_eq!(kind, kind::STATIC_BUNDLE);
        assert_eq!(meta["framework"], "sveltekit");
        assert_eq!(meta["build_command"], "npm run build");
        assert_eq!(meta["file_count"], 2);
    }

    #[test]
    fn extract_kind_metadata_edge_function_defaults_runtime_to_js() {
        let manifest = serde_json::json!({
            "annotations": { ANNOTATION_KIND: kind::EDGE_FUNCTION }
        });
        let (kind, meta) = extract_kind_metadata(&manifest);
        assert_eq!(kind, kind::EDGE_FUNCTION);
        assert_eq!(meta["runtime"], "js");
        assert!(meta["entry_point"].is_null());
    }

    #[test]
    fn extract_kind_metadata_unknown_kind_yields_empty_metadata() {
        let manifest = serde_json::json!({
            "annotations": { ANNOTATION_KIND: "something_unrecognized" }
        });
        let (kind, meta) = extract_kind_metadata(&manifest);
        assert_eq!(kind, "something_unrecognized");
        assert_eq!(meta, serde_json::json!({}));
    }

    #[test]
    fn wrap_single_blob_sets_kind_annotation_and_layer() {
        let manifest = wrap_single_blob(
            "sha256:abc123",
            42,
            kind::STATIC_BUNDLE,
            serde_json::Map::new(),
        );
        assert_eq!(manifest["annotations"][ANNOTATION_KIND], kind::STATIC_BUNDLE);
        assert_eq!(manifest["layers"][0]["digest"], "sha256:abc123");
        assert_eq!(manifest["layers"][0]["size"], 42);
        assert_eq!(manifest["schemaVersion"], 2);
    }

    #[test]
    fn wrap_single_blob_preserves_extra_annotations() {
        let mut extra = serde_json::Map::new();
        extra.insert("custom.key".to_string(), serde_json::Value::String("value".to_string()));
        let manifest = wrap_single_blob("sha256:def456", 10, kind::EDGE_FUNCTION, extra);
        assert_eq!(manifest["annotations"]["custom.key"], "value");
        assert_eq!(manifest["annotations"][ANNOTATION_KIND], kind::EDGE_FUNCTION);
    }
}
