-- Update database services based on docker image
UPDATE services
SET icon = 'postgresql'
WHERE type = 'database' AND image LIKE '%postgres%';

UPDATE services
SET icon = 'mysql'
WHERE type = 'database' AND image LIKE '%mysql%';

UPDATE services
SET icon = 'redis'
WHERE type = 'database' AND image LIKE '%redis%';

UPDATE services
SET icon = 'mongodb'
WHERE type = 'database' AND (image LIKE '%mongo%' OR image LIKE '%mongodb%');

-- Update static services by joining with static_site_configs framework setting
UPDATE services s
SET icon = CASE 
    WHEN c.framework = 'nextjs' OR c.framework = 'nextjs-export' THEN 'nextjs'
    WHEN c.framework = 'sveltekit' OR c.framework = 'sveltekit-static' THEN 'sveltekit'
    WHEN c.framework = 'nuxt' THEN 'nuxtjs'
    WHEN c.framework = 'astro' THEN 'astro'
    WHEN c.framework = 'vite' THEN 'vite'
    ELSE 'html5'
END
FROM static_site_configs c
WHERE s.id = c.service_id AND s.type = 'static' AND s.icon IS NULL;

-- Update docker and docker_compose services
UPDATE services
SET icon = 'docker'
WHERE type::text IN ('docker', 'docker_compose') AND icon IS NULL;
