-- Add icon column to services table
ALTER TABLE services ADD COLUMN IF NOT EXISTS icon TEXT;
