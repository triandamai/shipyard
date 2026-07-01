-- PostgreSQL initialization script for Shipyard PaaS
-- This runs once when the container is first created.

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- The actual schema is managed by sqlx migrations in the backend.
-- This script just ensures extensions are available.
