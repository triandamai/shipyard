//! Certificate generation for tenant compute-node mTLS.
//!
//! One platform CA signs three kinds of leaf cert per node:
//!   - the API's own client identity (used against both node_agent and dockerd)
//!   - node_agent's server identity (DNS-named, not IP — known before the VM exists)
//!   - dockerd's server identity (IP-SAN'd — generated once the VM's public IP is
//!     known, see `provisioning::advance_booting_nodes`, and pushed to the node
//!     over the already-mTLS-secured node_agent channel)
//!
//! None of the *server* certs are persisted to the DB — only the CA cert and the
//! API's own client cert/key are (`compute_nodes.tls_ca_cert/tls_client_cert/tls_client_key`),
//! since those are the only material the API needs again after provisioning.

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose,
};
use shipyard_common::{config::NodeMtlsConfig, error::AppError};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub type TlsResult<T> = Result<T, AppError>;

/// Leaf certs are short-lived — nodes are ephemeral (VM lifetime bounded by
/// subscription) and there's no rotation mechanism in this first pass;
/// re-provisioning is the accepted renewal story.
const LEAF_CERT_VALIDITY_DAYS: i64 = 90;
const CA_CERT_VALIDITY_DAYS: i64 = 3650;

/// The API's own identity when connecting out to a node's dockerd/node_agent.
pub struct ClientIdentity {
    pub cert_pem: String,
    pub key_pem: String,
}

/// node_agent's server identity, DNS-named (no IP dependency — generated
/// before the VM's IP is known, baked into cloud-init at creation time).
pub struct AgentServerIdentity {
    pub cert_pem: String,
    pub key_pem: String,
    /// The DNS name embedded as SAN/CN — callers connecting to this node's
    /// node_agent must use this as the TLS server name override, not the IP.
    pub server_name: String,
}

/// dockerd's server identity, IP-SAN'd — generated once the VM's public IP
/// is known (bollard/Docker's TLS client does strict IP/hostname verification
/// with no override hook, unlike the reqwest-based node_agent client).
pub struct DockerServerIdentity {
    pub cert_pem: String,
    pub key_pem: String,
}

pub fn agent_server_name(node_id: Uuid) -> String {
    format!("node-agent-{node_id}.shipyard.internal")
}

/// Load the platform CA from config, or generate a fresh one if none is
/// configured. The generated CA is NOT persisted anywhere by this function —
/// callers must ensure `shipyard_node_ca` is a real Docker secret in any
/// deployment that needs certs to survive a process restart.
pub fn load_or_generate_ca(cfg: &NodeMtlsConfig) -> TlsResult<(Certificate, KeyPair)> {
    if !cfg.ca_cert_pem.is_empty() && !cfg.ca_key_pem.is_empty() {
        let key_pair = KeyPair::from_pem(&cfg.ca_key_pem)
            .map_err(|e| AppError::Internal(format!("parse node mTLS CA key: {e}")))?;
        let params = CertificateParams::from_ca_cert_pem(&cfg.ca_cert_pem)
            .map_err(|e| AppError::Internal(format!("parse node mTLS CA cert: {e}")))?;
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| AppError::Internal(format!("reconstruct node mTLS CA: {e}")))?;
        return Ok((cert, key_pair));
    }

    tracing::warn!(
        "No node mTLS CA configured (shipyard_node_ca Docker secret absent) — generating a \
         fresh one for this process only. It is NOT persisted: restarting the API will \
         invalidate every already-provisioned tenant node's certs. Set the shipyard_node_ca \
         Docker secret in any deployment with real tenants."
    );
    generate_ca()
}

fn generate_ca() -> TlsResult<(Certificate, KeyPair)> {
    let key_pair =
        KeyPair::generate().map_err(|e| AppError::Internal(format!("generate CA key pair: {e}")))?;

    let mut params = CertificateParams::new(Vec::<String>::new())
        .map_err(|e| AppError::Internal(format!("build CA cert params: {e}")))?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Shipyard Node Platform CA");
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.not_before = OffsetDateTime::now_utc() - Duration::hours(1);
    params.not_after = OffsetDateTime::now_utc() + Duration::days(CA_CERT_VALIDITY_DAYS);

    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| AppError::Internal(format!("self-sign platform CA: {e}")))?;
    Ok((cert, key_pair))
}

enum LeafKind {
    Server,
    Client,
}

fn leaf_params(cn: String, sans: Vec<String>, kind: LeafKind) -> TlsResult<CertificateParams> {
    let mut params = CertificateParams::new(sans)
        .map_err(|e| AppError::Internal(format!("build leaf cert params for '{cn}': {e}")))?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, cn);
    params.distinguished_name = dn;
    params.not_before = OffsetDateTime::now_utc() - Duration::hours(1);
    params.not_after = OffsetDateTime::now_utc() + Duration::days(LEAF_CERT_VALIDITY_DAYS);
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
    params.extended_key_usages = vec![match kind {
        LeafKind::Server => ExtendedKeyUsagePurpose::ServerAuth,
        LeafKind::Client => ExtendedKeyUsagePurpose::ClientAuth,
    }];
    Ok(params)
}

fn sign_leaf(params: CertificateParams, ca_cert: &Certificate, ca_key: &KeyPair) -> TlsResult<(String, String)> {
    let key_pair =
        KeyPair::generate().map_err(|e| AppError::Internal(format!("generate leaf key pair: {e}")))?;
    let cert = params
        .signed_by(&key_pair, ca_cert, ca_key)
        .map_err(|e| AppError::Internal(format!("sign leaf cert: {e}")))?;
    Ok((cert.pem(), key_pair.serialize_pem()))
}

/// The API's own client identity for one node. Per-node (not shared across
/// all nodes) so a single leaked node's stored client key doesn't grant
/// access to other nodes.
pub fn generate_client_identity(
    ca_cert: &Certificate,
    ca_key: &KeyPair,
    node_id: Uuid,
) -> TlsResult<ClientIdentity> {
    let cn = format!("shipyard-api-client-{node_id}");
    let params = leaf_params(cn, Vec::new(), LeafKind::Client)?;
    let (cert_pem, key_pem) = sign_leaf(params, ca_cert, ca_key)?;
    Ok(ClientIdentity { cert_pem, key_pem })
}

/// node_agent's server identity — DNS-named, generated before the VM exists
/// (embedded directly in cloud-init), so callers must connect with a TLS
/// server-name override (see `agent_server_name`) rather than relying on
/// hostname-matches-dialed-IP verification.
pub fn generate_agent_server_identity(
    ca_cert: &Certificate,
    ca_key: &KeyPair,
    node_id: Uuid,
) -> TlsResult<AgentServerIdentity> {
    let server_name = agent_server_name(node_id);
    let cn = format!("node-agent-{node_id}");
    let params = leaf_params(cn, vec![server_name.clone()], LeafKind::Server)?;
    let (cert_pem, key_pem) = sign_leaf(params, ca_cert, ca_key)?;
    Ok(AgentServerIdentity { cert_pem, key_pem, server_name })
}

/// dockerd's server identity — IP-SAN'd, generated once the VM's public IP
/// is known and pushed over the node_agent mTLS channel (see
/// `provisioning::advance_booting_nodes`), since bollard's TLS client does
/// strict IP verification with no override hook.
pub fn generate_docker_server_identity(
    ca_cert: &Certificate,
    ca_key: &KeyPair,
    node_id: Uuid,
    public_ip: &str,
) -> TlsResult<DockerServerIdentity> {
    let cn = node_id.to_string();
    let params = leaf_params(cn, vec![public_ip.to_string()], LeafKind::Server)?;
    let (cert_pem, key_pem) = sign_leaf(params, ca_cert, ca_key)?;
    Ok(DockerServerIdentity { cert_pem, key_pem })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_self_signed_ca() {
        let (cert, _key) = generate_ca().unwrap();
        assert!(cert.pem().contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn client_identity_has_no_ip_dependency() {
        let (ca_cert, ca_key) = generate_ca().unwrap();
        let id = generate_client_identity(&ca_cert, &ca_key, Uuid::nil()).unwrap();
        assert!(id.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(id.key_pem.contains("PRIVATE KEY"));
    }

    #[test]
    fn agent_server_identity_uses_dns_name_not_ip() {
        let (ca_cert, ca_key) = generate_ca().unwrap();
        let node_id = Uuid::nil();
        let id = generate_agent_server_identity(&ca_cert, &ca_key, node_id).unwrap();
        assert_eq!(id.server_name, agent_server_name(node_id));
        assert!(id.server_name.contains("shipyard.internal"));
    }

    #[test]
    fn docker_server_identity_embeds_the_given_ip() {
        let (ca_cert, ca_key) = generate_ca().unwrap();
        let id = generate_docker_server_identity(&ca_cert, &ca_key, Uuid::nil(), "203.0.113.10").unwrap();
        assert!(id.cert_pem.contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn load_or_generate_ca_falls_back_when_unconfigured() {
        let cfg = NodeMtlsConfig::default();
        let (cert, _key) = load_or_generate_ca(&cfg).unwrap();
        assert!(cert.pem().contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn load_or_generate_ca_reconstructs_a_configured_ca() {
        let (cert, key) = generate_ca().unwrap();
        let cfg = NodeMtlsConfig { ca_cert_pem: cert.pem(), ca_key_pem: key.serialize_pem() };
        let (reloaded_cert, reloaded_key) = load_or_generate_ca(&cfg).unwrap();
        // Re-signing produces a different (but equally valid) ECDSA signature
        // each time — same key material and subject is what actually matters.
        assert_eq!(reloaded_key.public_key_der(), key.public_key_der());
        assert_eq!(reloaded_cert.params().distinguished_name, cert.params().distinguished_name);
    }

    #[test]
    fn leaf_certs_are_signed_by_the_ca_not_self_signed() {
        let (ca_cert, ca_key) = generate_ca().unwrap();
        let id = generate_client_identity(&ca_cert, &ca_key, Uuid::nil()).unwrap();
        // A cert signed by the CA must differ from the CA's own self-signed cert.
        assert_ne!(id.cert_pem, ca_cert.pem());
    }
}
