<script lang="ts">
	import { onMount } from 'svelte';
	import { Anchor, ChevronRight, Menu, X } from '@lucide/svelte';

	let activeSection = $state('installation');
	let sidebarOpen = $state(false);

	const nav = [
		{
			group: 'Getting Started',
			items: [
				{ id: 'installation',    label: 'Installation' },
				{ id: 'initial-setup',   label: 'Initial Setup' },
			],
		},
		{
			group: 'Organization',
			items: [
				{ id: 'manage-org',     label: 'Manage Organization' },
				{ id: 'members',        label: 'Members & Roles' },
				{ id: 'permissions',    label: 'Permissions Reference' },
			],
		},
		{
			group: 'Projects & Services',
			items: [
				{ id: 'projects',       label: 'Manage Projects' },
				{ id: 'services',       label: 'Services & Resources' },
				{ id: 'env-vars',       label: 'Environment Variables' },
				{ id: 'domains',        label: 'Domains & HTTPS' },
			],
		},
		{
			group: 'Deployments',
			items: [
				{ id: 'deploy-git',     label: 'Deploy from Git' },
				{ id: 'deploy-image',   label: 'Deploy Docker Image' },
				{ id: 'deploy-compose', label: 'Docker Compose Import' },
				{ id: 'webhooks',       label: 'Webhook Triggers' },
				{ id: 'rollback',       label: 'Rollback' },
				{ id: 'audit-log',      label: 'Audit Log' },
			],
		},
		{
			group: 'Integrations',
			items: [
				{ id: 'git-providers',  label: 'Git Providers' },
				{ id: 'smtp',           label: 'SMTP' },
				{ id: 'api-keys',       label: 'API Keys' },
			],
		},
		{
			group: 'Infrastructure',
			items: [
				{ id: 'infra',            label: 'Infra Monitoring' },
				{ id: 'swarm',            label: 'Swarm & Multi-node' },
				{ id: 'docker-resources', label: 'Docker Resources' },
				{ id: 'static-server',    label: 'Static Server' },
				{ id: 'mqtt',             label: 'MQTT Settings' },
				{ id: 'traefik',          label: 'Traefik Settings' },
			],
		},
		{
			group: 'Account & Platform',
			items: [
				{ id: 'profile',        label: 'Update Profile' },
				{ id: 'update',         label: 'Update Shipyard' },
				{ id: 'command-palette', label: 'Command Palette' },
			],
		},
	];

	onMount(() => {
		const observer = new IntersectionObserver(
			(entries) => {
				for (const e of entries) {
					if (e.isIntersecting) activeSection = e.target.id;
				}
			},
			{ rootMargin: '-20% 0px -70% 0px' },
		);
		document.querySelectorAll('section[id]').forEach((el) => observer.observe(el));
		return () => observer.disconnect();
	});

	function scrollTo(id: string) {
		document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
		activeSection = id;
		sidebarOpen = false;
	}

	const ghActionsSnippet = `- name: Deploy to Shipyard
  run: |
    curl -fsS -X POST "\${{ secrets.SHIPYARD_WEBHOOK_URL }}"`;

</script>

<svelte:head>
	<title>Docs — Shipyard</title>
	<meta name="description" content="Shipyard documentation — installation, projects, services, deployments, infrastructure, and more." />
</svelte:head>

<!-- ─── Top nav ────────────────────────────────────────────────────────────── -->
<header class="topbar">
	<nav class="topbar-inner">
		<a href="/" class="brand">
			<Anchor size={18} strokeWidth={2.5} />
			<span>Shipyard</span>
		</a>
		<div class="topbar-links">
			<a href="/" class="topbar-link">Home</a>
			<a href="/docs" class="topbar-link active">Docs</a>
			<a href="/docs/api" class="topbar-link">API Reference</a>
			<a href="https://github.com/triandamai/shipyard" target="_blank" rel="noopener noreferrer" class="topbar-link">GitHub</a>
		</div>
		<button class="mobile-menu-btn" onclick={() => sidebarOpen = !sidebarOpen} aria-label="Toggle menu">
			{#if sidebarOpen}<X size={18} />{:else}<Menu size={18} />{/if}
		</button>
	</nav>
</header>

<!-- ─── Layout ─────────────────────────────────────────────────────────────── -->
<div class="docs-layout">

	<!-- Sidebar -->
	<aside class="sidebar" class:open={sidebarOpen}>
		<nav class="sidebar-nav">
			{#each nav as group}
				<div class="nav-group">
					<div class="nav-group-label">{group.group}</div>
					{#each group.items as item}
						<button
							class="nav-item"
							class:active={activeSection === item.id}
							onclick={() => scrollTo(item.id)}
						>
							<ChevronRight size={12} />
							{item.label}
						</button>
					{/each}
				</div>
			{/each}
		</nav>
	</aside>

	<!-- Content -->
	<main class="content">

		<!-- ── Getting Started ─────────────────────────────────────────── -->
		<section id="installation">
			<h1>Installation</h1>
			<p>Shipyard runs on any Linux VPS with Docker installed. One script sets everything up.</p>

			<h3>Prerequisites</h3>
			<ul>
				<li>Ubuntu 22+ / Debian 12+ / any modern Linux distro</li>
				<li>Docker ≥ 24 and Docker Compose v2 (the script installs them if missing)</li>
				<li>A domain name pointed at your server (optional but recommended for HTTPS)</li>
				<li>Ports <code>80</code>, <code>443</code>, and <code>8080</code> open</li>
			</ul>

			<h3>Run the install script</h3>
			<div class="code-block">
				<div class="code-label">bash</div>
				<pre>curl -fsSL https://shipyard.trian.space/install.sh | sudo bash</pre>
			</div>

			<p>The script will prompt you for:</p>
			<ul>
				<li><strong>Domain</strong> — e.g. <code>ship.example.com</code> (or leave blank to use the server IP)</li>
				<li><strong>Enable HTTPS</strong> — Yes sets up Let's Encrypt via Traefik automatically</li>
				<li><strong>Admin email</strong> — used for the Let's Encrypt certificate</li>
			</ul>

			<div class="callout callout-info">
				All configuration is written to <code>/opt/shipyard/</code>. The stack runs as Docker Compose services. To restart: <code>cd /opt/shipyard && docker compose restart</code>
			</div>

			<h3>What gets installed</h3>
			<ul>
				<li><strong>shipyard-backend</strong> — Rust API server (Axum)</li>
				<li><strong>shipyard-frontend</strong> — SvelteKit dashboard</li>
				<li><strong>PostgreSQL</strong> — all platform state</li>
				<li><strong>Traefik</strong> — reverse proxy + automatic TLS</li>
				<li><strong>RMQTT</strong> — MQTT broker for real-time events</li>
			</ul>

			<div class="cluster-highlight">
				<div class="cluster-highlight-icon">⚡</div>
				<div class="cluster-highlight-body">
					<div class="cluster-highlight-title">Multi-node cluster support included</div>
					<p>
						This installs Shipyard on a single VPS as the <strong>manager node</strong>. You can
						scale to a full Docker Swarm cluster at any time — no reinstall needed. Add more
						VPS machines as worker nodes with one script and Swarm distributes your workloads
						automatically across all machines.
					</p>
					<button class="cluster-link-btn" onclick={() => scrollTo('swarm')}>
						Learn about Swarm &amp; Multi-node
						<ChevronRight size={13} />
					</button>
				</div>
			</div>
		</section>

		<section id="initial-setup">
			<h2>Initial Setup</h2>
			<p>On first visit, Shipyard shows a setup wizard.</p>

			<ol>
				<li>Open <code>http://&lt;your-server&gt;</code> (or your domain if HTTPS is configured)</li>
				<li>Create your <strong>admin account</strong> — enter an email and password</li>
				<li>Create your first <strong>organization</strong> — give it a name and a URL slug</li>
				<li>You are taken to the dashboard and are ready to deploy</li>
			</ol>

			<div class="callout callout-tip">
				The setup wizard only runs once. After the admin account is created, the <code>/setup</code> route redirects to login.
			</div>
		</section>

		<!-- ── Organization ────────────────────────────────────────────── -->
		<section id="manage-org">
			<h2>Manage Organization</h2>
			<p>Organizations are the top-level container for all projects and members. You can belong to multiple organizations.</p>

			<h3>General settings</h3>
			<p>Navigate to <strong>Settings → General</strong> to rename the organization or change its URL slug.</p>

			<div class="callout callout-warn">
				Changing the slug changes all URLs. Share the new link with your team after updating.
			</div>

			<h3>Switching organizations</h3>
			<p>Click the organization name in the top-left of the sidebar to open the org switcher. You can create a new organization from there as well.</p>
		</section>

		<section id="members">
			<h2>Members & Roles</h2>
			<p>Navigate to <strong>Settings → Members</strong> to manage who has access to the organization.</p>

			<h3>Roles</h3>
			<div class="table-wrap">
				<table>
					<thead><tr><th>Role</th><th>What they can do</th></tr></thead>
					<tbody>
						<tr><td><span class="badge badge-owner">Owner</span></td><td>Full access — billing, delete org, manage all settings</td></tr>
						<tr><td><span class="badge badge-admin">Admin</span></td><td>Manage members, projects, services, and all settings</td></tr>
						<tr><td><span class="badge badge-member">Member</span></td><td>Deploy and manage services in assigned projects</td></tr>
						<tr><td><span class="badge badge-viewer">Viewer</span></td><td>Read-only access — can view logs and deployment status</td></tr>
					</tbody>
				</table>
			</div>

			<h3>Inviting members</h3>
			<ol>
				<li>Click <strong>Invite member</strong></li>
				<li>Enter the email address and choose a role</li>
				<li>Optionally assign them to specific projects</li>
				<li>Click <strong>Send invitation</strong> — the invitee receives an email with a join link</li>
			</ol>

			<div class="callout callout-info">
				SMTP must be configured under <strong>Settings → SMTP</strong> for invitation emails to be delivered.
			</div>

			<h3>Fine-grained permissions</h3>
			<p>
				Owners and Admins bypass all permission checks automatically.
				For Members and Viewers, you can grant individual org-level permissions (e.g. <code>settings:write</code>, <code>keys:read</code>)
				and project-level access tiers (<em>View / Deploy / Manage</em>).
				Click a member → <strong>Org Permissions</strong> or <strong>Project Access</strong> to edit.
			</p>
			<p>See the full reference in <button class="inline-link" onclick={() => scrollTo('permissions')}>Permissions Reference →</button></p>
		</section>

		<!-- ── Permissions Reference ──────────────────────────────── -->
		<section id="permissions">
			<h2>Permissions Reference</h2>
			<p>
				Shipyard uses a two-layer permission model.
				<strong>Org-level</strong> permissions control access to platform-wide features.
				<strong>Project-level</strong> tiers control what a member can do within a specific project.
			</p>
			<p>
				Permission strings follow the pattern <code>shipyard:&lt;org_id&gt;:&lt;resource&gt;:&lt;action&gt;</code> for org-level
				and <code>shipyard:&lt;org_id&gt;:&lt;project_id&gt;:&lt;resource&gt;:&lt;action&gt;</code> for project-level.
				Owners and Admins bypass all checks; these only apply to Members and Viewers.
			</p>

			<h3>Org-level permissions</h3>
			<p>These are granted directly to a member within the organization and are independent of any project.</p>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Permission string</th><th>Label</th><th>What it allows</th></tr>
					</thead>
					<tbody>
						<tr><td><code>settings:read</code></td><td>View settings</td><td>Read org settings, main domain, and Traefik config</td></tr>
						<tr><td><code>settings:write</code></td><td>Edit settings</td><td>Modify org settings, SMTP, and domain config</td></tr>
						<tr><td><code>members:read</code></td><td>View members</td><td>See the member list and their roles</td></tr>
						<tr><td><code>members:invite</code></td><td>Invite members</td><td>Send invitations to new members</td></tr>
						<tr><td><code>members:manage</code></td><td>Manage members</td><td>Change roles, set permissions, and remove members</td></tr>
						<tr><td><code>projects:read</code></td><td>View all projects</td><td>Access any project in the organization</td></tr>
						<tr><td><code>projects:write</code></td><td>Manage projects</td><td>Create and delete projects</td></tr>
						<tr><td><code>providers:read</code></td><td>View providers</td><td>View connected Git provider accounts and webhook config (Settings → Providers)</td></tr>
						<tr><td><code>providers:write</code></td><td>Manage providers</td><td>Connect / disconnect GitHub, GitLab, Bitbucket and set webhook secrets</td></tr>
						<tr><td><code>infra:read</code></td><td>View infrastructure</td><td>View system metrics, swarm nodes, join tokens, and core service health</td></tr>
						<tr><td><code>infra:write</code></td><td>Manage infrastructure</td><td>Add/remove swarm nodes and modify cluster config</td></tr>
						<tr><td><code>static:read</code></td><td>View static server</td><td>View nginx static server configuration and site conf files (Settings → Static)</td></tr>
						<tr><td><code>docker:read</code></td><td>View Docker</td><td>Browse containers, services, volumes, and networks</td></tr>
						<tr><td><code>docker:write</code></td><td>Manage Docker</td><td>Prune containers and perform destructive Docker operations</td></tr>
						<tr><td><code>deployments:read</code></td><td>View deployments</td><td>View deployment history and status across all projects</td></tr>
						<tr><td><code>deployments:write</code></td><td>Manage deployments</td><td>Configure deployment parallelism and settings</td></tr>
						<tr><td><code>smtp:read</code></td><td>View SMTP config</td><td>View outgoing email configuration</td></tr>
						<tr><td><code>smtp:write</code></td><td>Manage SMTP config</td><td>Edit and test SMTP / email settings</td></tr>
						<tr><td><code>audit:read</code></td><td>View audit logs</td><td>Read organization activity history</td></tr>
						<tr><td><code>keys:read</code></td><td>View API keys</td><td>List API keys in the organization</td></tr>
						<tr><td><code>keys:write</code></td><td>Manage API keys</td><td>Create and revoke API keys</td></tr>
						<tr><td><code>system:update</code></td><td>Update Shipyard</td><td>Trigger platform updates and view update logs</td></tr>
					</tbody>
				</table>
			</div>

			<div class="callout callout-info">
				Full string stored in the database: <code>shipyard:&lt;org_id&gt;:settings:read</code>.
				The UI works with the suffix only (<code>settings:read</code>) and prepends the org ID at save time.
			</div>

			<h3>Project-level permission tiers</h3>
			<p>
				When assigning a member to a project you choose a tier — <em>View</em>, <em>Deploy</em>, or <em>Manage</em>.
				Each tier is additive: <em>Deploy</em> includes everything in <em>View</em>, and <em>Manage</em> includes everything in <em>Deploy</em>.
			</p>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Tier</th><th>What it allows</th><th>Permission strings granted</th></tr>
					</thead>
					<tbody>
						<tr>
							<td><span class="badge badge-viewer">View</span></td>
							<td>Read-only access to services and deployments</td>
							<td>
								<code>project:view</code><br/>
								<code>service:view</code>
							</td>
						</tr>
						<tr>
							<td><span class="badge badge-member">Deploy</span></td>
							<td>Trigger deployments, restarts, and rebuilds</td>
							<td>
								<em>(View permissions, plus:)</em><br/>
								<code>service:deploy</code>
							</td>
						</tr>
						<tr>
							<td><span class="badge badge-admin">Manage</span></td>
							<td>Create, edit, delete services; manage envs, domains, volumes, networks; access DB client</td>
							<td>
								<em>(Deploy permissions, plus:)</em><br/>
								<code>project:manage</code> · <code>service:write</code> · <code>service:delete</code><br/>
								<code>env:read</code> · <code>env:write</code> · <code>domain:write</code><br/>
								<code>volume:write</code> · <code>network:write</code>
							</td>
						</tr>
					</tbody>
				</table>
			</div>

			<div class="callout callout-info">
				Full string format: <code>shipyard:&lt;org_id&gt;:&lt;project_id&gt;:service:write</code>.
				These are stored as an array in <code>project_members.permissions</code> and checked by each backend endpoint.
			</div>

			<h3>Which tier is needed for each feature</h3>
			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Feature</th><th>Minimum tier / permission</th></tr>
					</thead>
					<tbody>
						<tr><td>View services, logs, topology</td><td><span class="badge badge-viewer">View</span></td></tr>
						<tr><td>View deployment history</td><td><span class="badge badge-viewer">View</span></td></tr>
						<tr><td>Trigger deploy / redeploy</td><td><span class="badge badge-member">Deploy</span></td></tr>
						<tr><td>Restart / stop / start service</td><td><span class="badge badge-member">Deploy</span></td></tr>
						<tr><td>Create or edit a service</td><td><span class="badge badge-admin">Manage</span></td></tr>
						<tr><td>Delete a service</td><td><span class="badge badge-admin">Manage</span></td></tr>
						<tr><td>Add / edit environment variables</td><td><span class="badge badge-admin">Manage</span></td></tr>
						<tr><td>Add / edit domains</td><td><span class="badge badge-admin">Manage</span></td></tr>
						<tr><td>Manage volumes & networks</td><td><span class="badge badge-admin">Manage</span></td></tr>
						<tr><td>Open DB Client</td><td><span class="badge badge-admin">Manage</span> (<code>service:write</code>)</td></tr>
						<tr><td>View org settings</td><td>Org: <code>settings:read</code></td></tr>
						<tr><td>Edit org settings / SMTP / OAuth</td><td>Org: <code>settings:write</code></td></tr>
						<tr><td>View infrastructure metrics</td><td>Org: <code>infra:read</code></td></tr>
						<tr><td>Invite new members</td><td>Org: <code>members:invite</code></td></tr>
						<tr><td>Change member roles / permissions</td><td>Org: <code>members:manage</code></td></tr>
						<tr><td>View / create API keys</td><td>Org: <code>keys:read</code> / <code>keys:write</code></td></tr>
						<tr><td>Trigger Shipyard platform update</td><td>Org: <code>system:update</code></td></tr>
					</tbody>
				</table>
			</div>
		</section>

		<!-- ── Projects & Services ─────────────────────────────────────── -->
		<section id="projects">
			<h2>Manage Projects</h2>
			<p>Projects group related services together. Every service belongs to exactly one project.</p>

			<h3>Creating a project</h3>
			<ol>
				<li>Click <strong>New Project</strong> in the sidebar</li>
				<li>Enter a project name — the slug is generated automatically</li>
				<li>Click <strong>Create</strong></li>
			</ol>

			<h3>Topology Canvas</h3>
			<p>Each project has a visual canvas showing all its services as nodes. Drag nodes to reposition them. Edges between nodes represent network connections between services.</p>
			<ul>
				<li>Click a node to open the <strong>Service Detail Panel</strong> on the right</li>
				<li>Status dots update in real time via MQTT — no page refresh needed</li>
				<li>Right-click a node to access quick actions (deploy, stop, delete)</li>
			</ul>
		</section>

		<section id="services">
			<h2>Services & Resources</h2>
			<p>A service is a containerized workload. Shipyard supports several service types:</p>

			<div class="table-wrap">
				<table>
					<thead><tr><th>Type</th><th>Description</th></tr></thead>
					<tbody>
						<tr><td><code>docker</code></td><td>Pull and run any Docker image</td></tr>
						<tr><td><code>git</code></td><td>Build from a Git repository (Dockerfile or Nixpacks)</td></tr>
						<tr><td><code>static</code></td><td>Serve static files via nginx — defaults to <code>nginx:alpine</code></td></tr>
						<tr><td><code>database</code></td><td>Run a database image with preset options (Postgres, MySQL, Redis, etc.)</td></tr>
						<tr><td><code>docker_compose</code></td><td>Import a Compose file as managed services</td></tr>
					</tbody>
				</table>
			</div>

			<h3>Creating a service</h3>
			<ol>
				<li>Inside a project, click <strong>Add Service</strong></li>
				<li>Choose the service type</li>
				<li>Fill in the image, port, and replica count</li>
				<li>Click <strong>Create</strong> — the service is created in <em>idle</em> state</li>
				<li>Click <strong>Deploy</strong> to start it</li>
			</ol>

			<h3>Resource limits</h3>
			<p>In the <strong>Service Detail Panel → Settings</strong>, scroll to <em>Resource Limits</em>. Set a CPU limit (in cores, e.g. <code>0.5</code>) and a memory limit (in MB, e.g. <code>512</code>). These are enforced by Docker Swarm on each task.</p>

			<h3>Replicas & scaling</h3>
			<p>Adjust the <strong>Replicas</strong> field in Settings and hit Save. Swarm spreads replicas across available nodes. With a single node all replicas run on that node.</p>
		</section>

		<section id="env-vars">
			<h2>Environment Variables</h2>
			<p>Click the <strong>Env</strong> button in the service header to open the environment variable manager.</p>

			<ul>
				<li>Variables marked <strong>Secret</strong> are stored encrypted and masked in the UI</li>
				<li>Changes take effect on the next deployment — existing containers are not restarted automatically</li>
				<li>Use <code>__IMAGE__</code> as a special key to override the service image at deploy time</li>
			</ul>

			<div class="callout callout-tip">
				Bulk-import variables by pasting a <code>.env</code> file format (KEY=value) into the import field.
			</div>
		</section>

		<section id="domains">
			<h2>Domains & HTTPS</h2>
			<p>Open a service → <strong>Domains</strong> tab to add a custom hostname.</p>

			<ol>
				<li>Click <strong>Add domain</strong></li>
				<li>Enter the hostname (e.g. <code>api.example.com</code>)</li>
				<li>Set the internal port the service listens on</li>
				<li>Toggle <strong>TLS</strong> to enable Let's Encrypt — Traefik requests and renews the certificate automatically</li>
			</ol>

			<div class="callout callout-info">
				The domain must resolve to your server's IP before TLS provisioning will succeed. Traefik uses HTTP-01 challenge by default.
			</div>

			<p>You can assign multiple domains to a single service. Each domain becomes its own Traefik router rule.</p>
		</section>

		<!-- ── Deployments ─────────────────────────────────────────────── -->
		<section id="deploy-git">
			<h2>Deploy from Git</h2>
			<p>Create a service with type <code>git</code>, connect a repository, and Shipyard builds and deploys it on demand.</p>

			<h3>Build process</h3>
			<ol>
				<li>Shipyard clones the repository at the configured branch</li>
				<li>If a <code>Dockerfile</code> is found in the directory path, it builds with Docker Build</li>
				<li>Otherwise it falls back to <strong>Nixpacks</strong> for automatic language detection</li>
				<li>The built image is pushed to the local registry and deployed to Swarm</li>
			</ol>

			<h3>Deployment steps</h3>
			<p>Every deployment goes through numbered steps visible in the <strong>Deploy</strong> tab:</p>
			<div class="steps-list">
				<div class="step-item"><span class="step-num">0</span><span>Validate config</span></div>
				<div class="step-item"><span class="step-num">1</span><span>Acquire image (clone + build, or pull)</span></div>
				<div class="step-item"><span class="step-num">2</span><span>Prepare networks</span></div>
				<div class="step-item"><span class="step-num">3</span><span>Prepare volumes</span></div>
				<div class="step-item"><span class="step-num">4</span><span>Configure domains</span></div>
				<div class="step-item"><span class="step-num">5</span><span>Create or update Swarm service</span></div>
				<div class="step-item"><span class="step-num">6</span><span>Write audit log</span></div>
			</div>

			<p>Click any step to expand its log output.</p>
		</section>

		<section id="deploy-image">
			<h2>Deploy Docker Image</h2>
			<p>Create a service with type <code>docker</code> (or <code>database</code> / <code>static</code>), set the image tag, and click <strong>Deploy</strong>.</p>

			<h3>Image digest pinning</h3>
			<p>After pulling the image Shipyard resolves it to its <code>sha256</code> digest (<code>nginx@sha256:abc...</code>). Docker Swarm compares image refs by string — using the digest means Swarm detects a new image even when the tag (<code>:latest</code>) hasn't changed. This is how redeployments pick up the freshest image without a manual restart.</p>

			<h3>Private registry</h3>
			<p>In <strong>Settings → Docker Image → Registry Credentials</strong>, enter the registry URL, username, and password. These are stored as service-scoped secrets and injected into the Swarm service spec at deploy time.</p>

			<h3>Database presets</h3>
			<p>For <code>database</code> type services, the Settings tab shows one-click preset buttons: <code>postgres:16</code>, <code>mysql:8</code>, <code>redis:7-alpine</code>, <code>mongo:7</code>, <code>mariadb:11</code>. Clicking a preset fills the image field.</p>
		</section>

		<section id="deploy-compose">
			<h2>Docker Compose Import</h2>
			<p>Navigate to a project → <strong>Import Compose</strong> to turn a <code>docker-compose.yml</code> into managed Shipyard services.</p>

			<ol>
				<li>Paste or upload your Compose YAML</li>
				<li>Set the root service name — used as the parent in the topology canvas</li>
				<li>Click <strong>Import</strong> — Shipyard creates a service, network, and volume entry for each definition in the file</li>
				<li>Deploy each imported service individually or trigger them in order</li>
			</ol>

			<div class="callout callout-warn">
				<code>build:</code> directives are not supported during import — use a pre-built image reference instead.
			</div>
		</section>

		<section id="webhooks">
			<h2>Webhook Triggers</h2>
			<p>Trigger a deployment from any external system (GitHub Actions, GitLab CI, a cron job) without using the dashboard.</p>

			<h3>Getting the webhook URL</h3>
			<ol>
				<li>Open a service → <strong>Settings</strong> tab → scroll to <em>Webhook</em></li>
				<li>Click <strong>Reveal token</strong> to see your webhook URL</li>
				<li>Copy the URL — it looks like <code>POST /api/projects/:id/services/:id/deploy/webhook?token=...</code></li>
			</ol>

			<h3>Triggering a deployment</h3>
			<div class="code-block">
				<div class="code-label">bash</div>
				<pre>curl -X POST "https://ship.example.com/api/projects/PROJECT_ID/services/SERVICE_ID/deploy/webhook?token=TOKEN"</pre>
			</div>

			<p>A <code>201 Created</code> response means a deployment was queued. The response body contains the deployment ID.</p>

			<h3>GitHub Actions example</h3>
			<div class="code-block">
				<div class="code-label">yaml</div>
				<pre>{ghActionsSnippet}</pre>
			</div>
		</section>

		<section id="rollback">
			<h2>Rollback</h2>
			<p>Every successful deployment records the exact image digest that was running. You can roll back to any prior successful deployment.</p>

			<h3>How to roll back</h3>
			<ol>
				<li>Open a service → <strong>Deploy</strong> tab</li>
				<li>Find a past deployment with status <span class="badge badge-success">success</span></li>
				<li>Click the <strong>↩</strong> rollback button next to it</li>
				<li>A new deployment is created — steps 0 and 1 are skipped (image is already known), the service is updated to the pinned digest</li>
			</ol>

			<div class="callout callout-info">
				Only deployments created after the rollback feature was enabled have a recorded image digest. Earlier deployments show no rollback button.
			</div>
		</section>

		<section id="audit-log">
			<h2>Audit Log</h2>
			<p>Navigate to <strong>Settings → Audit</strong> to view a full history of actions taken across the organization.</p>

			<p>Each entry records:</p>
			<ul>
				<li>The <strong>action</strong> (e.g. <code>service.deploy</code>, <code>member.invite</code>, <code>service.delete</code>)</li>
				<li>The <strong>user</strong> who performed it</li>
				<li>The <strong>resource</strong> affected</li>
				<li>The <strong>IP address</strong> of the request</li>
				<li>The <strong>timestamp</strong></li>
			</ul>

			<p>The log is paginated (50 entries per page). Use the Prev / Next buttons to navigate.</p>
		</section>

		<!-- ── Integrations ────────────────────────────────────────────── -->
		<section id="git-providers">
			<h2>Git Providers</h2>
			<p>
				Connect your GitHub, GitLab, or Bitbucket account to enable OAuth login and private repository access.
				Navigate to <strong>Settings → Providers</strong> to manage all connections. Requires the <code>providers:read</code> permission (or Admin/Owner role).
			</p>

			<h3>Personal Access Token (PAT)</h3>
			<ol>
				<li>Open <strong>Settings → Providers</strong> and click <strong>Connect</strong> next to the provider</li>
				<li>Paste a Personal Access Token with repository read scope</li>
				<li>Click <strong>Save Token</strong> — Shipyard stores it encrypted and uses it for all deploys</li>
			</ol>

			<h3>GitHub OAuth setup</h3>
			<ol>
				<li>Go to GitHub → <strong>Settings → Developer settings → OAuth Apps → New OAuth App</strong></li>
				<li>Set <em>Homepage URL</em> to your Shipyard domain</li>
				<li>Set <em>Authorization callback URL</em> to <code>https://ship.example.com/auth/oauth/github/callback</code></li>
				<li>Copy the <strong>Client ID</strong> and <strong>Client Secret</strong></li>
				<li>In Shipyard, open <strong>Settings → Providers</strong>, click <strong>Connect via OAuth</strong> and complete the flow</li>
			</ol>

			<p>GitLab and Bitbucket follow the same pattern — create an OAuth application in each provider's developer settings and complete the OAuth flow in Shipyard.</p>

			<h3>Webhook secret</h3>
			<p>
				To verify push event signatures from your provider, set a <strong>Webhook Secret</strong> on the Providers page and configure the same value in each webhook you create on GitHub/GitLab.
				The incoming webhook URL is shown on the same page and in each service's detail panel.
			</p>

			<div class="callout callout-info">
				Admins can grant <code>providers:read</code> / <code>providers:write</code> to Members so they can view or manage provider connections without full settings access.
			</div>
		</section>

		<section id="smtp">
			<h2>SMTP</h2>
			<p>Configure SMTP so Shipyard can send invitation emails and notifications.</p>

			<h3>Configuration</h3>
			<ol>
				<li>Go to <strong>Settings → SMTP</strong></li>
				<li>Toggle <strong>Enable SMTP</strong></li>
				<li>Enter host, port, username, password, and from address</li>
				<li>Click <strong>Save</strong>, then <strong>Send test email</strong> to verify delivery</li>
			</ol>

			<div class="table-wrap">
				<table>
					<thead><tr><th>Field</th><th>Example</th></tr></thead>
					<tbody>
						<tr><td>Host</td><td><code>smtp.gmail.com</code></td></tr>
						<tr><td>Port</td><td><code>587</code> (STARTTLS) or <code>465</code> (SSL)</td></tr>
						<tr><td>Username</td><td><code>you@gmail.com</code></td></tr>
						<tr><td>Password</td><td>App password (not your account password)</td></tr>
						<tr><td>From address</td><td><code>shipyard@example.com</code></td></tr>
					</tbody>
				</table>
			</div>

			<div class="callout callout-info">
				SMTP settings are read from the database at send-time. Changing them takes effect immediately — no backend restart required.
			</div>
		</section>

		<section id="api-keys">
			<h2>API Keys</h2>
			<p>API keys give programmatic access to Shipyard's API — useful for CI/CD pipelines or automation scripts.</p>

			<h3>Creating a key</h3>
			<ol>
				<li>Go to <strong>Settings → API Keys</strong></li>
				<li>Click <strong>New API key</strong></li>
				<li>Give it a name, choose scopes, and optionally set an expiry date</li>
				<li>Copy the key immediately — it is shown only once</li>
			</ol>

			<p>Keys look like <code>ship_...</code> and are authenticated via the <code>Authorization: Bearer ship_...</code> header.</p>

			<div class="code-block">
				<div class="code-label">bash</div>
				<pre>curl https://ship.example.com/api/orgs \
  -H "Authorization: Bearer ship_your_key_here"</pre>
			</div>

			<div class="callout callout-warn">
				Keys are stored as SHA-256 hashes. If you lose the key, revoke it and create a new one.
			</div>
		</section>

		<!-- ── Infrastructure ──────────────────────────────────────────── -->
		<section id="infra">
			<h2>Infra Monitoring</h2>
			<p>Navigate to <strong>Settings → Infrastructure</strong> to view real-time metrics about the host machine.</p>

			<h3>Metrics streamed live</h3>
			<ul>
				<li><strong>CPU</strong> — usage percentage with color-coded gauge (green / orange / red)</li>
				<li><strong>Memory</strong> — used vs total, with swap if present</li>
				<li><strong>Disk</strong> — per-mount usage and percentage</li>
				<li><strong>Network</strong> — cumulative RX/TX bytes per interface since boot (Docker internal interfaces are hidden)</li>
			</ul>

			<p>Metrics are delivered via Server-Sent Events (SSE) — the <em>Live</em> badge in the toolbar turns green when the stream is connected. Click <strong>Reconnect</strong> if the stream drops.</p>
		</section>

		<section id="swarm">
			<h2>Swarm & Multi-node</h2>
			<p>Shipyard runs on Docker Swarm. You can add additional VPS nodes to distribute workloads across machines.</p>

			<h3>How Swarm works</h3>
			<ul>
				<li>Your Shipyard server is the <strong>manager node</strong> — it schedules services across the cluster</li>
				<li>Additional VPS machines join as <strong>worker nodes</strong> — they run containers but have no scheduling control</li>
				<li>Swarm automatically re-schedules replicas if a worker goes offline</li>
				<li>Stateless services (APIs, web apps) work across nodes with zero extra config</li>
				<li>Stateful services (databases) should be pinned to a specific node using placement constraints</li>
			</ul>

			<h3>Adding a worker node</h3>
			<p>On the new VPS, run the guided setup script:</p>
			<div class="code-block">
				<div class="code-label">bash</div>
				<pre>curl -fsSL https://shipyard.trian.space/worker-setup.sh | sudo bash</pre>
			</div>

			<p>The script will:</p>
			<ol>
				<li>Check and install Docker if needed</li>
				<li>Prompt for the manager address and join token (find these in <strong>Settings → Infrastructure → Join Tokens</strong>)</li>
				<li>Optionally configure Docker Hub, GitHub Container Registry (<code>ghcr.io</code>), or a custom registry</li>
				<li>Join the swarm and verify the connection</li>
			</ol>

			<h3>Join tokens</h3>
			<p>Find the ready-to-copy <code>docker swarm join</code> commands in <strong>Settings → Infrastructure → Join Tokens</strong>. There are two token types:</p>
			<div class="table-wrap">
				<table>
					<thead><tr><th>Token type</th><th>Use for</th></tr></thead>
					<tbody>
						<tr><td><span class="badge badge-member">worker</span></td><td>Standard nodes that run workloads — use this for most VPS additions</td></tr>
						<tr><td><span class="badge badge-admin">manager</span></td><td>Nodes that also participate in scheduling — use for HA setups (3+ managers)</td></tr>
					</tbody>
				</table>
			</div>

			<div class="callout callout-warn">
				Worker nodes only need Docker installed — no Shipyard stack, no Postgres, no Traefik.
			</div>

			<h3>Viewing nodes</h3>
			<p>The <strong>Swarm Nodes</strong> table on the infra page shows each node's hostname, role, status (<em>ready / down</em>), availability (<em>active / drain / pause</em>), address, and Docker engine version.</p>
		</section>

		<section id="static-server">
			<h2>Static Server</h2>
			<p>
				Shipyard includes an nginx-based static file server (<code>shipyard-nginx-static</code>) that hosts static sites deployed from the platform.
				Navigate to <strong>Settings → Static</strong> to inspect its current nginx configuration. Requires the <code>static:read</code> permission (or <code>infra:read</code> / Admin).
			</p>

			<h3>Conf file viewer</h3>
			<p>
				The left panel lists all <code>.conf</code> files inside the container's <code>/etc/nginx/conf.d/</code> directory.
				Select a file to view its full content in the right panel. Files are named after the service slug (e.g. <code>my-landing-page.conf</code>).
			</p>

			<div class="callout callout-info">
				A conf file is only written when at least one domain is assigned to a static service. Services without a domain return 404 until a domain is attached.
			</div>
		</section>

		<section id="docker-resources">
			<h2>Docker Resources</h2>
			<p>Navigate to <strong>Settings → Docker</strong> to inspect and manage raw Docker resources on the host.</p>

			<h3>Containers</h3>
			<p>Lists all containers (running and stopped) with their image, status, and port bindings. Use <strong>Prune stopped containers</strong> to reclaim disk space.</p>

			<h3>Services</h3>
			<p>Lists all Swarm services currently deployed — name, image, running vs desired replicas, and exposed ports.</p>

			<h3>Volumes</h3>
			<p>Lists all named volumes with their driver, mountpoint, and scope. You can remove unused volumes from here.</p>

			<h3>Networks</h3>
			<p>Lists all Docker networks — driver, scope, subnet, and attached container count. Internal networks created by Shipyard for service isolation appear here.</p>
		</section>

		<section id="mqtt">
			<h2>MQTT Settings</h2>
			<p>Shipyard includes an embedded RMQTT broker that powers real-time updates in the dashboard (service status, topology changes, deployment logs).</p>

			<p>Navigate to <strong>Settings → MQTT</strong> to view:</p>
			<ul>
				<li><strong>Connected clients</strong> — browser tabs and backend workers subscribed to the broker</li>
				<li><strong>Active subscriptions</strong> — per-client topic list</li>
				<li><strong>Active topics</strong> — topic names with subscriber counts</li>
			</ul>

			<h3>Topic structure</h3>
			<div class="code-block">
				<div class="code-label">topics</div>
				<pre>shipyard/services/{'{service_id}'}/status
shipyard/services/{'{service_id}'}/containers
shipyard/deployments/{'{deployment_id}'}/logs
shipyard/topology/{'{project_id}'}</pre>
			</div>

			<div class="callout callout-info">
				You can subscribe to these topics from any MQTT client (e.g. MQTTX) using the broker credentials shown in Settings → MQTT.
			</div>
		</section>

		<section id="traefik">
			<h2>Traefik Settings</h2>
			<p>Traefik is the reverse proxy that routes incoming requests to your services and handles TLS certificates.</p>

			<p>Navigate to <strong>Settings → Traefik</strong> to inspect the generated configuration files.</p>

			<h3>Static config</h3>
			<p>The static configuration (<code>traefik.yml</code>) sets up the entry points and certificate resolver. It is generated once at install time. Key settings:</p>
			<div class="code-block">
				<div class="code-label">yaml</div>
				<pre>entryPoints:
  web:      # port 80  — HTTP, redirects to HTTPS
  websecure: # port 443 — HTTPS
certificatesResolvers:
  letsencrypt:
    acme:
      email: your@email.com
      storage: /letsencrypt/acme.json</pre>
			</div>

			<h3>Dynamic config</h3>
			<p>Shipyard writes dynamic configuration (routers and services) automatically when you add or update a domain on a service. View the current dynamic config in <strong>Settings → Traefik → Dynamic</strong>.</p>

			<div class="callout callout-tip">
				If a certificate isn't being issued, check that port 80 is open and the domain resolves to your server's IP. Traefik logs are visible via <code>docker logs shipyard-traefik-1</code>.
			</div>
		</section>

		<!-- ── Account & Platform ──────────────────────────────────────── -->
		<section id="profile">
			<h2>Update Profile</h2>
			<p>Click your avatar or email in the top-right of the sidebar → <strong>Profile</strong>.</p>

			<h3>Change email</h3>
			<p>Enter a new email address and confirm your current password. The change takes effect immediately.</p>

			<h3>Change password</h3>
			<p>Enter your current password, then the new password twice. You will be logged out of all other sessions.</p>
		</section>

		<section id="update">
			<h2>Update Shipyard</h2>
			<p>Navigate to <strong>Settings → General</strong> to check for and apply updates.</p>

			<h3>One-click update</h3>
			<ol>
				<li>The current version is shown alongside the latest available release</li>
				<li>Click <strong>Update now</strong> — Shipyard pulls the new images and restarts the stack</li>
				<li>The update log streams in real time so you can follow the progress</li>
			</ol>

			<p>Alternatively, update manually from the server:</p>
			<div class="code-block">
				<div class="code-label">bash</div>
				<pre>cd /opt/shipyard
docker compose pull
docker compose up -d</pre>
			</div>

			<div class="callout callout-info">
				Database migrations run automatically on startup. No manual migration step is needed.
			</div>
		</section>

		<section id="command-palette">
			<h2>Command Palette</h2>
			<p>The command palette gives you keyboard-driven access to any page, service, or action in Shipyard without touching the mouse.</p>

			<h3>Opening the palette</h3>
			<div class="kbd-group">
				<kbd>⌘</kbd> + <kbd>K</kbd>
				<span class="kbd-sep">on macOS</span>
			</div>
			<div class="kbd-group">
				<kbd>Ctrl</kbd> + <kbd>K</kbd>
				<span class="kbd-sep">on Windows / Linux</span>
			</div>

			<h3>What you can do</h3>
			<ul>
				<li>Search services by name and jump to them directly</li>
				<li>Navigate to any settings page</li>
				<li>Trigger a deployment on a specific service</li>
				<li>Switch organizations</li>
				<li>Open the audit log, API keys, or member management</li>
			</ul>

			<p>Type to filter. Use <kbd>↑</kbd> <kbd>↓</kbd> to navigate results and <kbd>Enter</kbd> to select. Press <kbd>Esc</kbd> to close.</p>
		</section>

	</main>
</div>

<style>
	:global(*, *::before, *::after) { box-sizing: border-box; margin: 0; padding: 0; }
	:global(html) { scroll-behavior: smooth; }
	:global(body) {
		font-family: 'Inter', system-ui, -apple-system, sans-serif;
		background: #0a0a0f;
		color: #cbd5e1;
		line-height: 1.7;
		-webkit-font-smoothing: antialiased;
	}

	/* ── Topbar ─────────────────────────────────────────────────── */
	.topbar {
		position: sticky; top: 0; z-index: 100;
		background: rgba(10,10,15,0.85);
		backdrop-filter: blur(16px);
		border-bottom: 1px solid rgba(255,255,255,0.07);
	}
	.topbar-inner {
		max-width: 1280px; margin: 0 auto; padding: 0 24px;
		height: 56px; display: flex; align-items: center; gap: 24px;
	}
	.brand {
		display: flex; align-items: center; gap: 8px;
		font-size: 15px; font-weight: 700; color: #fff;
		text-decoration: none; flex-shrink: 0;
	}
	.brand :global(svg) { color: #3b82f6; }
	.topbar-links { display: flex; align-items: center; gap: 4px; margin-left: auto; }
	.topbar-link {
		padding: 5px 12px; font-size: 13px; font-weight: 500;
		color: rgba(255,255,255,0.5); text-decoration: none;
		border-radius: 6px; transition: color 0.15s, background 0.15s;
	}
	.topbar-link:hover { color: #fff; background: rgba(255,255,255,0.06); }
	.topbar-link.active { color: #60a5fa; }
	.mobile-menu-btn {
		display: none; align-items: center; justify-content: center;
		width: 36px; height: 36px; background: transparent;
		border: 1px solid rgba(255,255,255,0.1); border-radius: 6px;
		color: rgba(255,255,255,0.6); cursor: pointer; margin-left: auto;
	}

	/* ── Layout ─────────────────────────────────────────────────── */
	.docs-layout {
		max-width: 1280px; margin: 0 auto;
		display: grid; grid-template-columns: 240px 1fr;
		min-height: calc(100vh - 56px);
	}

	/* ── Sidebar ─────────────────────────────────────────────────── */
	.sidebar {
		position: sticky; top: 56px; height: calc(100vh - 56px);
		overflow-y: auto; border-right: 1px solid rgba(255,255,255,0.07);
		padding: 24px 0; scrollbar-width: thin;
		scrollbar-color: rgba(255,255,255,0.1) transparent;
	}
	.sidebar-nav { display: flex; flex-direction: column; gap: 24px; padding: 0 16px; }
	.nav-group { display: flex; flex-direction: column; gap: 2px; }
	.nav-group-label {
		font-size: 10px; font-weight: 700; letter-spacing: 0.1em;
		text-transform: uppercase; color: rgba(255,255,255,0.3);
		padding: 0 8px; margin-bottom: 4px;
	}
	.nav-item {
		display: flex; align-items: center; gap: 6px;
		padding: 6px 8px; font-size: 13px; font-weight: 500;
		color: rgba(255,255,255,0.45); background: transparent;
		border: none; border-radius: 6px; cursor: pointer;
		text-align: left; width: 100%;
		transition: color 0.15s, background 0.15s;
	}
	.nav-item :global(svg) { flex-shrink: 0; opacity: 0; transition: opacity 0.15s; }
	.nav-item:hover { color: rgba(255,255,255,0.8); background: rgba(255,255,255,0.05); }
	.nav-item.active {
		color: #60a5fa; background: rgba(59,130,246,0.1);
	}
	.nav-item.active :global(svg) { opacity: 1; }

	/* ── Content ─────────────────────────────────────────────────── */
	.content {
		padding: 48px 64px 96px 64px;
		max-width: 820px;
	}

	section {
		padding-top: 16px;
		margin-bottom: 64px;
		scroll-margin-top: 72px;
	}
	section:first-child { padding-top: 0; }

	h1 {
		font-size: 2rem; font-weight: 800; color: #f1f5f9;
		letter-spacing: -0.03em; margin-bottom: 16px;
	}
	h2 {
		font-size: 1.5rem; font-weight: 700; color: #f1f5f9;
		letter-spacing: -0.02em; margin-bottom: 14px;
		padding-bottom: 10px; border-bottom: 1px solid rgba(255,255,255,0.07);
	}
	h3 {
		font-size: 1rem; font-weight: 650; color: #e2e8f0;
		margin-top: 28px; margin-bottom: 10px;
	}

	p { color: rgba(255,255,255,0.6); margin-bottom: 14px; font-size: 14.5px; }

	ul, ol {
		color: rgba(255,255,255,0.6); font-size: 14.5px;
		padding-left: 20px; display: flex; flex-direction: column; gap: 6px;
		margin-bottom: 14px;
	}
	li { line-height: 1.65; }

	code {
		font-family: 'Fira Code', 'JetBrains Mono', ui-monospace, monospace;
		font-size: 12.5px; color: #93c5fd;
		background: rgba(59,130,246,0.1); padding: 1px 5px; border-radius: 4px;
	}

	/* ── Code blocks ─────────────────────────────────────────────── */
	.code-block {
		background: #0d1017; border: 1px solid rgba(255,255,255,0.08);
		border-radius: 8px; overflow: hidden; margin: 16px 0;
	}
	.code-label {
		padding: 6px 14px; font-size: 11px; font-weight: 600;
		letter-spacing: 0.06em; text-transform: uppercase;
		color: rgba(255,255,255,0.3); background: rgba(255,255,255,0.03);
		border-bottom: 1px solid rgba(255,255,255,0.06);
	}
	.code-block pre {
		padding: 16px; font-family: 'Fira Code', 'JetBrains Mono', ui-monospace, monospace;
		font-size: 13px; line-height: 1.7; color: #93c5fd;
		overflow-x: auto; white-space: pre;
	}

	/* ── Callouts ─────────────────────────────────────────────────── */
	.callout {
		display: flex; align-items: flex-start; gap: 10px;
		padding: 12px 16px; border-radius: 8px;
		font-size: 13.5px; line-height: 1.6;
		margin: 16px 0;
	}
	.callout::before { flex-shrink: 0; font-weight: 700; margin-top: 1px; }
	.callout-info {
		background: rgba(59,130,246,0.08); border: 1px solid rgba(59,130,246,0.2);
		color: #93c5fd;
	}
	.callout-info::before { content: 'ℹ'; color: #60a5fa; }
	.callout-tip {
		background: rgba(34,197,94,0.07); border: 1px solid rgba(34,197,94,0.2);
		color: #86efac;
	}
	.callout-tip::before { content: '✦'; color: #4ade80; }
	.callout-warn {
		background: rgba(234,179,8,0.07); border: 1px solid rgba(234,179,8,0.2);
		color: #fde68a;
	}
	.callout-warn::before { content: '⚠'; color: #facc15; }

	/* ── Cluster highlight ───────────────────────────────────────── */
	.cluster-highlight {
		display: flex; gap: 16px; align-items: flex-start;
		padding: 18px 20px; margin: 24px 0;
		background: linear-gradient(135deg, rgba(37,99,235,0.1) 0%, rgba(99,102,241,0.08) 100%);
		border: 1px solid rgba(99,102,241,0.3);
		border-radius: 10px;
	}
	.cluster-highlight-icon {
		font-size: 22px; flex-shrink: 0; margin-top: 2px;
	}
	.cluster-highlight-body { display: flex; flex-direction: column; gap: 8px; }
	.cluster-highlight-title {
		font-size: 14px; font-weight: 700; color: #a5b4fc;
	}
	.cluster-highlight-body p {
		font-size: 13.5px; color: rgba(255,255,255,0.55); margin: 0; line-height: 1.65;
	}
	.cluster-link-btn {
		display: inline-flex; align-items: center; gap: 5px;
		margin-top: 4px; padding: 6px 14px;
		font-size: 12.5px; font-weight: 600; font-family: inherit;
		color: #818cf8; background: rgba(99,102,241,0.12);
		border: 1px solid rgba(99,102,241,0.3); border-radius: 6px;
		cursor: pointer; transition: all 0.15s; width: fit-content;
	}
	.cluster-link-btn:hover {
		background: rgba(99,102,241,0.22); border-color: rgba(99,102,241,0.5);
		color: #a5b4fc; transform: translateX(2px);
	}

	/* ── Tables ──────────────────────────────────────────────────── */
	.table-wrap { overflow-x: auto; margin: 16px 0; }
	table { width: 100%; border-collapse: collapse; font-size: 13.5px; }
	thead th {
		padding: 9px 14px; text-align: left;
		font-size: 11px; font-weight: 600; letter-spacing: 0.05em; text-transform: uppercase;
		color: rgba(255,255,255,0.35); background: rgba(255,255,255,0.03);
		border-bottom: 1px solid rgba(255,255,255,0.08);
	}
	tbody td {
		padding: 10px 14px; color: rgba(255,255,255,0.6);
		border-bottom: 1px solid rgba(255,255,255,0.05);
	}
	tbody tr:last-child td { border-bottom: none; }
	tbody tr:hover td { background: rgba(255,255,255,0.02); }
	table { border: 1px solid rgba(255,255,255,0.08); border-radius: 8px; overflow: hidden; }

	/* ── Badges ──────────────────────────────────────────────────── */
	.badge {
		display: inline-block; padding: 2px 8px;
		font-size: 11px; font-weight: 600; border-radius: 999px;
		letter-spacing: 0.02em;
	}
	.badge-owner   { background: rgba(239,68,68,0.15);  color: #fca5a5; }
	.badge-admin   { background: rgba(234,179,8,0.15);  color: #fde68a; }
	.badge-member  { background: rgba(99,102,241,0.15); color: #a5b4fc; }
	.badge-viewer  { background: rgba(255,255,255,0.08); color: rgba(255,255,255,0.5); }
	.badge-success { background: rgba(34,197,94,0.15);  color: #86efac; }

	/* ── Deployment steps list ───────────────────────────────────── */
	.steps-list {
		display: flex; flex-direction: column; gap: 0;
		border: 1px solid rgba(255,255,255,0.08); border-radius: 8px;
		overflow: hidden; margin: 16px 0;
	}
	.step-item {
		display: flex; align-items: center; gap: 12px;
		padding: 10px 16px; font-size: 13.5px; color: rgba(255,255,255,0.6);
		border-bottom: 1px solid rgba(255,255,255,0.05);
	}
	.step-item:last-child { border-bottom: none; }
	.step-num {
		width: 24px; height: 24px; border-radius: 50%;
		background: rgba(59,130,246,0.15); border: 1px solid rgba(59,130,246,0.3);
		color: #60a5fa; font-size: 11px; font-weight: 700;
		display: flex; align-items: center; justify-content: center; flex-shrink: 0;
	}

	/* ── Keyboard shortcut ───────────────────────────────────────── */
	kbd {
		display: inline-flex; align-items: center; justify-content: center;
		padding: 3px 8px; font-size: 12px; font-family: inherit; font-weight: 600;
		background: rgba(255,255,255,0.07); border: 1px solid rgba(255,255,255,0.15);
		border-bottom-width: 2px; border-radius: 5px; color: rgba(255,255,255,0.7);
	}
	.kbd-group { display: flex; align-items: center; gap: 6px; margin: 10px 0; }
	.kbd-sep { font-size: 12px; color: rgba(255,255,255,0.3); }

	/* ── Inline link button ─────────────────────────────────────────── */
	.inline-link {
		background: none; border: none; padding: 0;
		color: #60a5fa; font-size: inherit; font-family: inherit;
		cursor: pointer; text-decoration: underline; text-underline-offset: 3px;
	}
	.inline-link:hover { color: #93c5fd; }

	/* ── Responsive ──────────────────────────────────────────────── */
	@media (max-width: 900px) {
		.docs-layout { grid-template-columns: 1fr; }
		.sidebar {
			position: fixed; top: 56px; left: 0; bottom: 0; z-index: 50;
			width: 260px; background: #0d0d14;
			border-right: 1px solid rgba(255,255,255,0.1);
			transform: translateX(-100%); transition: transform 0.25s ease;
		}
		.sidebar.open { transform: translateX(0); }
		.mobile-menu-btn { display: flex; }
		.topbar-links { display: none; }
		.content { padding: 32px 24px 80px; }
	}
	@media (max-width: 480px) {
		h1 { font-size: 1.6rem; }
		h2 { font-size: 1.25rem; }
		.content { padding: 24px 16px 80px; }
	}
</style>
