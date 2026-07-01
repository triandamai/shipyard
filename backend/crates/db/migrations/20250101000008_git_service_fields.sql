-- Git-based service source fields
ALTER TABLE services ADD COLUMN IF NOT EXISTS git_repo_url TEXT;
ALTER TABLE services ADD COLUMN IF NOT EXISTS git_branch   TEXT NOT NULL DEFAULT 'main';
