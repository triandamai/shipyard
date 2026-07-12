-- Make services the single root table for all resource types.
-- Edge function groups previously had no service row; this links each group
-- to a synthetic service row (type='edge_functions') so that domains, topology,
-- and all shared infrastructure work without special-casing.

ALTER TYPE service_type ADD VALUE IF NOT EXISTS 'edge_functions';

ALTER TABLE edge_function_groups
    ADD COLUMN service_id UUID REFERENCES services(id) ON DELETE CASCADE;
