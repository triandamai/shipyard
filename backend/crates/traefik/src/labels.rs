use std::collections::HashMap;

/// Generates Traefik Docker labels for swarm services.
///
/// Full implementation in Milestone 1.3.
pub struct TraefikLabelGenerator {
    /// Traefik entrypoint for HTTPS (e.g., "websecure")
    pub entrypoint_https: String,
    /// ACME cert resolver name (e.g., "letsencrypt")
    pub cert_resolver: String,
}

impl TraefikLabelGenerator {
    pub fn new(entrypoint_https: &str, cert_resolver: &str) -> Self {
        Self {
            entrypoint_https: entrypoint_https.to_string(),
            cert_resolver: cert_resolver.to_string(),
        }
    }

    /// Generate Traefik labels for a domain routing to a service.
    ///
    /// Returns a HashMap of label key → value pairs to be applied to the swarm service.
    pub fn generate_labels(
        &self,
        router_name: &str,
        hostname: &str,
        service_port: u16,
        tls_enabled: bool,
    ) -> HashMap<String, String> {
        let mut labels = HashMap::new();

        labels.insert("traefik.enable".to_string(), "true".to_string());

        // Router rule
        labels.insert(
            format!("traefik.http.routers.{router_name}.rule"),
            format!("Host(`{hostname}`)"),
        );

        if tls_enabled {
            // HTTPS entrypoint
            labels.insert(
                format!("traefik.http.routers.{router_name}.entrypoints"),
                self.entrypoint_https.clone(),
            );
            // TLS cert resolver
            labels.insert(
                format!("traefik.http.routers.{router_name}.tls.certresolver"),
                self.cert_resolver.clone(),
            );
        }

        // Load balancer target port
        labels.insert(
            format!("traefik.http.services.{router_name}.loadbalancer.server.port"),
            service_port.to_string(),
        );

        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_labels_with_tls() {
        let gen = TraefikLabelGenerator::new("websecure", "letsencrypt");
        let labels = gen.generate_labels("my-api", "api.example.com", 8080, true);

        assert_eq!(labels.get("traefik.enable").unwrap(), "true");
        assert_eq!(
            labels.get("traefik.http.routers.my-api.rule").unwrap(),
            "Host(`api.example.com`)"
        );
        assert_eq!(
            labels.get("traefik.http.routers.my-api.entrypoints").unwrap(),
            "websecure"
        );
        assert_eq!(
            labels.get("traefik.http.routers.my-api.tls.certresolver").unwrap(),
            "letsencrypt"
        );
        assert_eq!(
            labels.get("traefik.http.services.my-api.loadbalancer.server.port").unwrap(),
            "8080"
        );
    }

    #[test]
    fn test_generate_labels_without_tls() {
        let gen = TraefikLabelGenerator::new("websecure", "letsencrypt");
        let labels = gen.generate_labels("nginx", "nginx.local", 80, false);

        assert_eq!(labels.get("traefik.enable").unwrap(), "true");
        assert!(labels.get("traefik.http.routers.nginx.tls.certresolver").is_none());
    }
}
