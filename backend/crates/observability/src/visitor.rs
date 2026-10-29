//! Visitor hashing for unique visitor counting without storing PII.
//!
//! Hashes are computed as:
//! `sha256(daily_salt || ip || user_agent)` truncated to 16 hex chars (64 bits).
//!
//! Salts rotate daily (at 00:00 UTC). Once a salt is destroyed, previous visitor
//! hashes cannot be linked to IP/UA or correlated across days.

use sha2::{Digest, Sha256};

pub const VISITOR_HASH_LEN: usize = 16;

/// Compute a privacy-preserving visitor hash.
pub fn hash_visitor(salt: &[u8], ip: &str, user_agent: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(b"|");
    hasher.update(ip.trim().as_bytes());
    hasher.update(b"|");
    hasher.update(user_agent.trim().as_bytes());
    let full = hex::encode(hasher.finalize());
    full[..VISITOR_HASH_LEN].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_salt_and_client() {
        let salt = b"2026-10-29-secret-salt-123";
        let h1 = hash_visitor(salt, "192.168.1.1", "Mozilla/5.0");
        let h2 = hash_visitor(salt, " 192.168.1.1 ", "Mozilla/5.0\n");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), VISITOR_HASH_LEN);
    }

    #[test]
    fn different_salt_produces_unlinkable_hash() {
        let salt_day1 = b"salt-day-1-xyz";
        let salt_day2 = b"salt-day-2-abc";
        let ip = "203.0.113.195";
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)";

        let h_day1 = hash_visitor(salt_day1, ip, ua);
        let h_day2 = hash_visitor(salt_day2, ip, ua);

        assert_ne!(h_day1, h_day2);
        // Hashes should differ significantly (not a trivial shift)
        assert!(!h_day1.starts_with(&h_day2[..4]));
    }

    #[test]
    fn different_ip_or_ua_produces_different_hash() {
        let salt = b"constant-salt";
        let h1 = hash_visitor(salt, "1.1.1.1", "curl/7.68.0");
        let h2 = hash_visitor(salt, "1.1.1.2", "curl/7.68.0");
        let h3 = hash_visitor(salt, "1.1.1.1", "curl/8.0.0");

        assert_ne!(h1, h2);
        assert_ne!(h1, h3);
        assert_ne!(h2, h3);
    }
}
