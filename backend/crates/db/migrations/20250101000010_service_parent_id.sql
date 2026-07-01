-- Add parent service reference so docker-compose child services link to their root.
ALTER TABLE services
    ADD COLUMN service_parent_id UUID REFERENCES services(id) ON DELETE SET NULL;

CREATE INDEX idx_services_parent_id ON services(service_parent_id);
