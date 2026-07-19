-- Store the provider-specific VM size slug on compute_nodes so the
-- provisioning worker doesn't have to hardcode it or guess from cpu/ram.
ALTER TABLE compute_nodes
    ADD COLUMN IF NOT EXISTS server_type TEXT NOT NULL DEFAULT 'cpx21';
