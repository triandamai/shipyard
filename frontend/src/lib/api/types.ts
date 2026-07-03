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
	id: string;
	label: string;
	description: string;
}

export interface PermissionGroup {
	group: string;
	permissions: PermissionDef[];
}

export const PERMISSION_GROUPS: PermissionGroup[] = [
	{
		group: 'Organization',
		permissions: [
			{ id: 'app:org:settings:read',  label: 'View settings',    description: 'Read organization settings and configuration' },
			{ id: 'app:org:settings:write', label: 'Edit settings',    description: 'Modify organization settings, domain, and Traefik config' },
			{ id: 'app:org:members:read',   label: 'View members',     description: 'See the member list and their roles' },
			{ id: 'app:org:members:invite', label: 'Invite members',   description: 'Send invitations to new members' },
			{ id: 'app:org:members:manage', label: 'Manage members',   description: 'Change roles, set permissions, and remove members' },
		],
	},
	{
		group: 'Projects',
		permissions: [
			{ id: 'app:project:read',   label: 'View projects',          description: 'See all projects and their services' },
			{ id: 'app:project:write',  label: 'Create & edit projects', description: 'Create new projects and edit existing ones' },
			{ id: 'app:project:delete', label: 'Delete projects',        description: 'Permanently delete projects and all their resources' },
		],
	},
	{
		group: 'Services',
		permissions: [
			{ id: 'app:project:service:read',   label: 'View services',          description: 'View service configuration, logs, and deployment history' },
			{ id: 'app:project:service:write',  label: 'Create & edit services', description: 'Create services and edit their configuration' },
			{ id: 'app:project:service:deploy', label: 'Deploy services',         description: 'Trigger deployments, rollbacks, and restarts' },
			{ id: 'app:project:service:delete', label: 'Delete services',         description: 'Remove services and their associated resources' },
		],
	},
	{
		group: 'Infrastructure',
		permissions: [
			{ id: 'app:project:volume:read',   label: 'View volumes',    description: 'See volumes attached to services' },
			{ id: 'app:project:volume:write',  label: 'Manage volumes',  description: 'Create, attach, and delete volumes' },
			{ id: 'app:project:network:read',  label: 'View networks',   description: 'See project networks and their topology' },
			{ id: 'app:project:network:write', label: 'Manage networks', description: 'Create networks and attach services' },
			{ id: 'app:project:domain:read',   label: 'View domains',    description: 'See domain mappings and TLS status' },
			{ id: 'app:project:domain:write',  label: 'Manage domains',  description: 'Add, configure, and remove domain mappings' },
		],
	},
];

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
	type: 'service' | 'network' | 'volume' | 'domain' | 'container';
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
