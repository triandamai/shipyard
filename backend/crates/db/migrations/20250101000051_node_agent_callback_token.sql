-- Per-node random token for node_agent's outbound spike-alert callback,
-- replacing the single global AGENT_TOKEN shared across every tenant node.
ALTER TABLE compute_nodes
    ADD COLUMN IF NOT EXISTS agent_callback_token TEXT;
