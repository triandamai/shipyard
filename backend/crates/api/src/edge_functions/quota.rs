use shipyard_common::config::EdgeFunctionsConfig;

pub struct TierQuota {
    pub max_functions: Option<u32>,   // None = unlimited
    pub max_bundle_kb: u64,
    pub max_timeout_secs: u32,
    pub max_invocations_per_day: Option<u64>,  // None = unlimited
    pub runtime_memory_mb: u32,
}

pub fn quota_for_tier(tier: &str, cfg: &EdgeFunctionsConfig) -> TierQuota {
    match tier {
        "pro" => TierQuota {
            max_functions: Some(10),
            max_bundle_kb: cfg.max_bundle_kb_pro,
            max_timeout_secs: 15,
            max_invocations_per_day: Some(cfg.max_invocations_pro),
            runtime_memory_mb: 512,
        },
        "max" => TierQuota {
            max_functions: None,
            max_bundle_kb: cfg.max_bundle_kb_max,
            max_timeout_secs: 30,
            max_invocations_per_day: None,
            runtime_memory_mb: 1024,
        },
        _ => TierQuota {
            // free tier
            max_functions: Some(1),
            max_bundle_kb: cfg.max_bundle_kb_free,
            max_timeout_secs: 5,
            max_invocations_per_day: Some(cfg.max_invocations_free),
            runtime_memory_mb: 256,
        },
    }
}
