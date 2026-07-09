-- Update static services that have NULL icon to fallback to 'html5'
UPDATE services
SET icon = 'html5'
WHERE type = 'static' AND icon IS NULL;

-- Update docker and docker_compose services to fallback to 'docker'
UPDATE services
SET icon = 'docker'
WHERE type::text IN ('docker', 'docker_compose') AND icon IS NULL;
