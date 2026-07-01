-- Port bindings for services (array of "hostPort:containerPort" strings)
ALTER TABLE services ADD COLUMN IF NOT EXISTS ports JSONB NOT NULL DEFAULT '[]';
