-- Add project_id to edge_function_groups so groups appear on the project canvas.
ALTER TABLE edge_function_groups
    ADD COLUMN IF NOT EXISTS project_id UUID REFERENCES projects(id) ON DELETE SET NULL;
