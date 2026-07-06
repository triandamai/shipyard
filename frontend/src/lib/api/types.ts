// ─── API Types ─────────────────────────────────────────────────
// Mirror of Rust backend types for type safety across the stack.

// Enums
export type MemberRole = 'owner' | 'admin' | 'member' | 'viewer';
export type ServiceType = 'git' | 'docker' | 'docker_compose' | 'manual' | 'static' | 'database';
export type ContainerStatus = 'pending' | 'preparing' | 'running' | 'complete' | 'failed' | 'shutdown' | 'rejected' | 'orphan';
export type DeploymentStatus = 'pending' | 'queued' | 'running' | 'success' | 'failed' | 'cancelled';
export type StepStatus = 'pending' | 'running' | 'success' | 'failed' | 'skipped';
export type LogLevel = 'debug' | 'info' | 'warn' | 'error';
export type EdgeType = 'network' | 'volume' | 'domain' | 'depends_on' | 'replica' | 'compose_child';

// Traefik file reading
export interface TraefikFileResponse {
	path: string;
	content: string | null;
	exists: boolean;
	error: string | null;
}

export interface TraefikDirEntry {
	name: string;
}

export interface TraefikDynamicResponse {
	dir: string;
	files: TraefikDirEntry[];
	error: string | null;
}

// API Envelope
export interface ApiResponse<T> {
	data: T | null;
	error: ApiError | null;
}

export interface ApiError {
	code: string;
	message: string;
}

// MQTT Payload
export interface MqttPayload {
	event: string;
	timestamp: string;
	level?: LogLevel;
	message?: string;
	meta?: Record<string, unknown>;
}

// Domain Models
export interface User {
	id: string;
	email: string;
	created_at: string;
	updated_at: string;
}

export interface Organization {
	id: string;
	name: string;
	slug: string;
	created_at: string;
}

export interface OrgMember {
	id: string;
	org_id: string;
	user_id: string;
	email: string;
	role: MemberRole;
	permissions: string[];
	user?: User;
	created_at: string;
}

export interface ProjectAssignment {
	project_id: string;
	permissions: string[];
}

export interface Invitation {
	id: string;
	org_id: string;
	email: string;
	role: MemberRole;
	permissions: string[];
	project_assignments: ProjectAssignment[];
	token: string;
	expires_at: string;
	accepted_at: string | null;
	created_at: string;
}

export interface MemberProjectAssignment {
	project_id: string;
	project_name: string;
	project_slug: string;
	permissions: string[];
}

export interface PublicInvite {
	token: string;
	org_id: string;
	org_name: string;
	email: string;
	role: MemberRole;
	permissions: string[];
	project_assignments: ProjectAssignment[];
	expires_at: string;
	is_expired: boolean;
	is_accepted: boolean;
}

// ─── Permission definitions ───────────────────────────────────────────────────

export interface PermissionDef {
	id: string;   // suffix only: e.g. "settings:read" — prepend "shipyard:<orgId>:" when saving
	label: string;
	description: string;
}

export interface PermissionGroup {
	group: string;
	permissions: PermissionDef[];
}

/**
 * Org-level permission definitions.
 * `id` is the suffix after `shipyard:<orgId>:` — the full string is built at save time.
 * Use `buildOrgPermission(orgId, def.id)` to get the canonical string.
 */
export const PERMISSION_GROUPS: PermissionGroup[] = [
	{
		group: 'Organization',
		permissions: [
			{ id: 'settings:read',   label: 'View settings',   description: 'Read org settings and Traefik config' },
			{ id: 'settings:write',  label: 'Edit settings',   description: 'Modify org settings and domain config' },
			{ id: 'members:read',    label: 'View members',    description: 'See the member list and their roles' },
			{ id: 'members:invite',  label: 'Invite members',  description: 'Send invitations to new members' },
			{ id: 'members:manage',  label: 'Manage members',  description: 'Change roles, set permissions, and remove members' },
		],
	},
	{
		group: 'Projects',
		permissions: [
			{ id: 'projects:read',   label: 'View all projects', description: 'Access any project in the organization' },
			{ id: 'projects:write',  label: 'Manage projects',   description: 'Create and delete projects' },
		],
	},
	{
		group: 'Git Providers',
		permissions: [
			{ id: 'providers:read',  label: 'View providers',   description: 'View connected Git provider accounts and webhook config' },
			{ id: 'providers:write', label: 'Manage providers', description: 'Connect / disconnect GitHub, GitLab, Bitbucket and set webhook secrets' },
		],
	},
	{
		group: 'Infrastructure',
		permissions: [
			{ id: 'infra:read',   label: 'View infrastructure',   description: 'View system metrics, swarm nodes, join tokens, and core services' },
			{ id: 'infra:write',  label: 'Manage infrastructure', description: 'Add/remove swarm nodes and modify cluster config' },
			{ id: 'static:read',  label: 'View static server',    description: 'View nginx static server configuration and site conf files' },
		],
	},
	{
		group: 'Docker',
		permissions: [
			{ id: 'docker:read',  label: 'View Docker',   description: 'Browse containers, services, volumes, and networks' },
			{ id: 'docker:write', label: 'Manage Docker', description: 'Prune containers and perform destructive Docker operations' },
		],
	},
	{
		group: 'Deployments',
		permissions: [
			{ id: 'deployments:read',  label: 'View deployments',   description: 'View deployment history and status across all projects' },
			{ id: 'deployments:write', label: 'Manage deployments', description: 'Configure deployment parallelism and settings' },
		],
	},
	{
		group: 'Email (SMTP)',
		permissions: [
			{ id: 'smtp:read',  label: 'View SMTP config',   description: 'View outgoing email configuration' },
			{ id: 'smtp:write', label: 'Manage SMTP config', description: 'Edit and test SMTP / email settings' },
		],
	},
	{
		group: 'Audit',
		permissions: [
			{ id: 'audit:read', label: 'View audit logs', description: 'Read organization activity history' },
		],
	},
	{
		group: 'API Keys',
		permissions: [
			{ id: 'keys:read',  label: 'View API keys',   description: 'List API keys in the organization' },
			{ id: 'keys:write', label: 'Manage API keys', description: 'Create and revoke API keys' },
		],
	},
	{
		group: 'System',
		permissions: [
			{ id: 'system:update', label: 'Update Shipyard', description: 'Trigger platform updates and view update logs' },
		],
	},
];

/** Build a full org-level shipyard: permission string from a suffix id. */
export function buildOrgPermission(orgId: string, suffixId: string): string {
	return `shipyard:${orgId}:${suffixId}`;
}

/** Extract the suffix id from a full shipyard: org-level permission string. */
export function parseOrgPermissionSuffix(orgId: string, fullPerm: string): string | null {
	const prefix = `shipyard:${orgId}:`;
	return fullPerm.startsWith(prefix) ? fullPerm.slice(prefix.length) : null;
}

/**
 * Project permission tiers used in the UI.
 * - view   → read-only access to services and deployments
 * - deploy → can trigger deploys, restarts, rebuilds
 * - manage → full CRUD: create/edit/delete services, envs, domains, volumes, networks
 */
export const PROJECT_PERM_OPTIONS: { id: ProjectPermTier; label: string; desc: string }[] = [
	{ id: 'view',   label: 'View',   desc: 'Read-only access to services and deployments' },
	{ id: 'deploy', label: 'Deploy', desc: 'Trigger deployments and restarts' },
	{ id: 'manage', label: 'Manage', desc: 'Create, edit, and delete services' },
];

export type ProjectPermTier = 'view' | 'deploy' | 'manage';

/**
 * Expand UI shorthand tiers to the full set of `shipyard:` permission strings
 * stored in `project_members.permissions`. Each tier is additive.
 *
 * Permissions mapped to backend checks:
 *   view   → project:view, service:view (org membership suffices for reads, but stored for clarity)
 *   deploy → + service:deploy
 *   manage → + project:manage, service:write, service:delete,
 *              env:read, env:write, domain:write, volume:write, network:write
 */
export function expandProjectPermissions(
	orgId: string,
	projectId: string,
	tiers: Set<ProjectPermTier> | string[],
): string[] {
	const set = new Set(tiers);
	const base = `shipyard:${orgId}:${projectId}:`;
	const out: string[] = [];

	const hasView   = set.has('view')   || set.has('deploy') || set.has('manage');
	const hasDeploy = set.has('deploy') || set.has('manage');
	const hasManage = set.has('manage');

	if (hasView) {
		out.push(base + 'project:view', base + 'service:view');
	}
	if (hasDeploy) {
		out.push(base + 'service:deploy');
	}
	if (hasManage) {
		out.push(
			base + 'project:manage',
			base + 'service:write',
			base + 'service:delete',
			base + 'env:read',
			base + 'env:write',
			base + 'domain:write',
			base + 'volume:write',
			base + 'network:write',
		);
	}

	return [...new Set(out)].sort();
}

/**
 * Reverse-map a stored permissions array back to the highest UI tier.
 * Returns an empty Set if no recognized tier is present.
 */
export function collapseProjectPermissions(
	orgId: string,
	projectId: string,
	fullPerms: string[],
): Set<ProjectPermTier> {
	const base = `shipyard:${orgId}:${projectId}:`;
	const has = (suffix: string) => fullPerms.includes(base + suffix);
	if (has('project:manage')) return new Set<ProjectPermTier>(['manage']);
	if (has('service:deploy')) return new Set<ProjectPermTier>(['deploy']);
	if (has('project:view'))   return new Set<ProjectPermTier>(['view']);
	return new Set<ProjectPermTier>();
}

export interface Project {
	id: string;
	org_id: string;
	name: string;
	slug: string;
	directory_path: string;
	node_positions: Record<string, { x: number; y: number }> | null;
	created_at: string;
	updated_at: string;
}

export interface Service {
	id: string;
	project_id: string;
	name: string;
	slug: string;
	type: ServiceType;
	image: string;
	git_repo_url: string | null;
	git_branch: string;
	auto_deploy: boolean;
	directory_path: string;
	ports: string[];
	status: string;
	replicas: number;
	service_parent_id: string | null;
	created_at: string;
	updated_at: string;
}

export interface ServiceEnv {
	id: string;
	service_id: string;
	key: string;
	value_encrypted: string;
	is_secret: boolean;
	created_at: string;
}

export interface Container {
	id: string;
	docker_container_id: string;
	docker_task_id: string;
	node_id: string;
	replica_index: number;
	status: ContainerStatus;
	status_message: string | null;
	image: string;
	started_at: string | null;
	finished_at: string | null;
	exit_code: number | null;
}

export interface PortBinding {
	container_port: number;
	protocol: string;
	host_ip: string;
	host_port: number;
}

export interface ContainerInspect {
	id: string;
	name: string;
	image: string;
	status: string;
	state: string;
	created: string | null;
	started_at: string | null;
	finished_at: string | null;
	exit_code: number | null;
	restart_count: number;
	platform: string | null;
	port_bindings: PortBinding[];
	env: string[];
	labels: Record<string, string>;
}

export interface Deployment {
	id: string;
	service_id: string;
	triggered_by: string;
	source_ref: string;
	status: DeploymentStatus;
	deployed_image: string | null;
	created_at: string;
	finished_at: string | null;
}

export interface DeploymentStep {
	id: string;
	deployment_id: string;
	name: string;
	status: StepStatus;
	order_index: number;
	started_at: string | null;
	finished_at: string | null;
}

export interface DeploymentLog {
	id: string;
	deployment_id: string;
	step_id: string;
	level: LogLevel;
	message: string;
	timestamp: string;
}

export interface Domain {
	id: string;
	service_id: string;
	hostname: string;
	tls_enabled: boolean;
	traefik_router_name: string;
	cert_provider: string;
	port: number | null;
	created_at: string;
}

export interface DnsCheckResult {
	hostname: string;
	resolves: boolean;
	addresses: string[];
}

export interface Volume {
	id: string;
	service_id: string;
	name: string;
	mount_path: string;
	driver: string;
	size_mb: number;
	created_at: string;
}

export interface Network {
	id: string;
	project_id: string;
	name: string;
	driver: string;
	subnet: string;
	created_at: string;
}

// Topology
export interface TopologyNode {
	id: string;
	type: 'service' | 'network' | 'volume' | 'domain' | 'container' | 'static_site';
	data: Record<string, unknown>;
}

export interface TopologyEdge {
	id: string;
	source: string;
	target: string;
	type: EdgeType;
}

export interface Topology {
	nodes: TopologyNode[];
	edges: TopologyEdge[];
}

// Templates
export interface TemplateEnvSpec {
	key: string;
	default: string;
	required: boolean;
	secret: boolean;
}

export interface TemplateVolumeSpec {
	mount: string;
}

export interface Template {
	id: string;
	name: string;
	description: string | null;
	type: ServiceType;
	image: string | null;
	env: TemplateEnvSpec[];
	volumes: TemplateVolumeSpec[];
	ports: number[];
	icon: string | null;
	is_builtin: boolean;
	created_at: string;
}

export interface ContainerStats {
	cpu_percent: number;
	memory_usage_bytes: number;
	memory_limit_bytes: number;
	memory_percent: number;
	net_rx_bytes: number;
	net_tx_bytes: number;
	block_read_bytes: number;
	block_write_bytes: number;
	pids: number;
	timestamp: string;
}

export interface ImportComposeResponse {
	root_service_id: string;
	services_created: number;
	networks_created: number;
	service_ids: string[];
	warnings: string[];
}

// Pagination
export interface Paginated<T> {
	data: T[];
	page: number;
	per_page: number;
	total: number;
}

// Auth
export interface AuthTokens {
	access_token: string;
	refresh_token?: string;
}

export interface LoginRequest {
	email: string;
	password: string;
}

export interface RegisterRequest {
	email: string;
	password: string;
}

export interface VersionInfo {
	current: string;
	latest: string;
	update_available: boolean;
	release_url: string;
	release_notes: string | null;
}

// Audit log
export interface AuditLogEntry {
	id: string;
	user_id: string | null;
	action: string;
	resource_type: string | null;
	resource_id: string | null;
	ip_address: string | null;
	metadata: Record<string, unknown> | null;
	created_at: string;
}

// Admin deployments view
export interface AdminDeploymentRow {
	id: string;
	service_id: string;
	service_name: string;
	project_id: string;
	project_name: string;
	triggered_by: string;
	source_ref: string;
	status: DeploymentStatus;
	created_at: string;
	finished_at: string | null;
}

export interface AdminDeploymentStats {
	total: number;
	running: number;
	queued: number;
	failed: number;
	success: number;
}

export interface AdminDeploymentsResponse {
	data: AdminDeploymentRow[];
	stats: AdminDeploymentStats;
	page: number;
	per_page: number;
	total: number;
}

// API Key Management
export type ApiKeyScope = 'read' | 'deploy' | 'write' | 'admin';

export interface ApiKeyItem {
	id: string;
	name: string;
	key_prefix: string;
	scopes: ApiKeyScope[];
	last_used_at: string | null;
	expires_at: string | null;
	created_at: string;
}

export interface CreatedApiKey {
	id: string;
	name: string;
	key: string;
	key_prefix: string;
	scopes: ApiKeyScope[];
	expires_at: string | null;
	created_at: string;
}

export interface CreateApiKeyRequest {
	name: string;
	scopes: ApiKeyScope[];
	expires_at?: string | null;
}

export interface ConnectionInfo {
	host: string;
	port: number;
	url_template: string;
	driver: string;
}

export interface SwarmNode {
	id: string;
	hostname: string;
	role: string;
	status: string;
	availability: string;
	engine_version: string | null;
	addr: string | null;
}

export interface SwarmJoinTokens {
	worker: string;
	manager: string;
	addr: string;
}

// ─── DB Client ─────────────────────────────────────────────────────────────────

export type DbEngine = 'postgres' | 'mysql' | 'mariadb' | 'redis' | 'mongodb';

export interface DbMeta {
	engine: DbEngine | null;
	host: string | null;
	port: number | null;
	username: string | null;
	detected: boolean;
}

export interface DbQueryRequest {
	engine: DbEngine;
	host: string;
	port: number;
	database: string;
	username: string;
	password: string;
	sql: string;
}

export interface DbQueryResult {
	columns: string[];
	rows: (string | number | boolean | null)[][];
	row_count: number;
	truncated: boolean;
	execution_time_ms: number;
}

// Static site hosting
export interface StaticSiteConfig {
	service_id:      string;
	source:          'git' | 'upload';
	build_command:   string;
	output_dir:      string;
	node_version:    string;
	install_command: string;
	framework:       string;
	deploy_config:   Record<string, unknown> | null;
}
