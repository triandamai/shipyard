-- ════════════════════════════════════════════════════════════════
-- Service templates registry
-- ════════════════════════════════════════════════════════════════

CREATE TABLE templates (
    id          TEXT PRIMARY KEY,           -- slug, e.g. "postgres-15"
    name        TEXT NOT NULL,
    description TEXT,
    type        TEXT NOT NULL,              -- service_type value as text
    image       TEXT,
    env         JSONB NOT NULL DEFAULT '[]',
    volumes     JSONB NOT NULL DEFAULT '[]',
    ports       JSONB NOT NULL DEFAULT '[]',
    icon        TEXT,
    is_builtin  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Pre-seed built-in templates
INSERT INTO templates (id, name, description, type, image, env, volumes, ports, icon, is_builtin) VALUES
(
  'postgres-15',
  'PostgreSQL 15',
  'Open-source relational database',
  'database',
  'postgres:15-alpine',
  '[{"key":"POSTGRES_USER","default":"postgres","required":true,"secret":false},{"key":"POSTGRES_PASSWORD","default":"","required":true,"secret":true},{"key":"POSTGRES_DB","default":"app","required":true,"secret":false}]',
  '[{"mount":"/var/lib/postgresql/data"}]',
  '[5432]',
  'database',
  true
),
(
  'mysql-8',
  'MySQL 8',
  'Popular open-source relational database',
  'database',
  'mysql:8-debian',
  '[{"key":"MYSQL_ROOT_PASSWORD","default":"","required":true,"secret":true},{"key":"MYSQL_DATABASE","default":"app","required":false,"secret":false},{"key":"MYSQL_USER","default":"app","required":false,"secret":false},{"key":"MYSQL_PASSWORD","default":"","required":false,"secret":true}]',
  '[{"mount":"/var/lib/mysql"}]',
  '[3306]',
  'database',
  true
),
(
  'redis-7',
  'Redis 7',
  'In-memory data store & cache',
  'database',
  'redis:7-alpine',
  '[]',
  '[{"mount":"/data"}]',
  '[6379]',
  'cache',
  true
),
(
  'mongodb-7',
  'MongoDB 7',
  'NoSQL document database',
  'database',
  'mongo:7',
  '[{"key":"MONGO_INITDB_ROOT_USERNAME","default":"admin","required":false,"secret":false},{"key":"MONGO_INITDB_ROOT_PASSWORD","default":"","required":false,"secret":true}]',
  '[{"mount":"/data/db"}]',
  '[27017]',
  'database',
  true
),
(
  'nginx',
  'Nginx',
  'High-performance web server & reverse proxy',
  'docker',
  'nginx:alpine',
  '[]',
  '[]',
  '[80, 443]',
  'web',
  true
),
(
  'node-20',
  'Node.js 20',
  'JavaScript runtime — bring your own Dockerfile or git repo',
  'git',
  'node:20-alpine',
  '[{"key":"NODE_ENV","default":"production","required":false,"secret":false},{"key":"PORT","default":"3000","required":false,"secret":false}]',
  '[]',
  '[3000]',
  'runtime',
  true
),
(
  'python-312',
  'Python 3.12',
  'General-purpose language — bring your own Dockerfile or git repo',
  'git',
  'python:3.12-slim',
  '[{"key":"PORT","default":"8000","required":false,"secret":false}]',
  '[]',
  '[8000]',
  'runtime',
  true
),
(
  'caddy-2',
  'Caddy 2',
  'Automatic HTTPS web server',
  'docker',
  'caddy:2-alpine',
  '[]',
  '[{"mount":"/data"},{"mount":"/config"}]',
  '[80, 443]',
  'web',
  true
);
