//! Traefik Configuration Generator
//!
//! Generates Docker labels for Traefik routing, TLS, health checks,
//! and middleware on swarm services.
//!
//! Full implementation in Milestone 1.3.

pub mod labels;

pub use labels::TraefikLabelGenerator;
