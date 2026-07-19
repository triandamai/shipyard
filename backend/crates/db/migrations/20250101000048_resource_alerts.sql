CREATE TABLE resource_alerts (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    metric      TEXT        NOT NULL,
    value       DOUBLE PRECISION NOT NULL,
    threshold   DOUBLE PRECISION NOT NULL,
    container_id TEXT,
    node_id     TEXT        NOT NULL,
    fired_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX resource_alerts_fired_at_idx ON resource_alerts (fired_at DESC);
CREATE INDEX resource_alerts_metric_node_idx ON resource_alerts (metric, node_id, container_id, fired_at DESC);
