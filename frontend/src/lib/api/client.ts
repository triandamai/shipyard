import type {
	ApiResponse,
	User,
	Organization,
	OrgMember,
	Invitation,
	PublicInvite,
	ProjectAssignment,
	MemberProjectAssignment,
	Project,
	Service,
	ServiceEnv,
	Deployment,
	DeploymentStep,
	DeploymentLog,
	Container,
	ContainerInspect,
	Domain,
	DnsCheckResult,
	Volume,
	Network,
	Topology,
	Template,
	TraefikFileResponse,
	TraefikDynamicResponse,
	ImportComposeResponse,
	AuditLogEntry,
	StaticSiteConfig,
} from './types';
import { authStore } from '$lib/stores/auth.store';
import { setAuthCookies } from '$lib/auth/cookies';

const API_BASE = '/api';

class ApiClient {
	private baseUrl: string;
	private token: string | null = null;
	private _refreshPromise: Promise<boolean> | null = null;

	constructor(baseUrl: string = API_BASE) {
		this.baseUrl = baseUrl;
	}

	setToken(token: string | null) {
		this.token = token;
	}

	private async _doRefresh(): Promise<boolean> {
		try {
			// The refresh token is an HttpOnly cookie — the browser sends it
			// automatically. No body needed; no JS-readable token to send.
			const res = await fetch(`${this.baseUrl}/auth/refresh`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'same-origin',
			});
			if (!res.ok) return false;
			const json = await res.json();
			const newToken: string | undefined = json?.data?.access_token;
			if (!newToken) return false;
			this.token = newToken;
			setAuthCookies(newToken);
			authStore.updateAccessToken(newToken);
			return true;
		} catch {
			return false;
		}
	}

	private _tryRefresh(): Promise<boolean> {
		if (!this._refreshPromise) {
			this._refreshPromise = this._doRefresh().finally(() => {
				this._refreshPromise = null;
			});
		}
		return this._refreshPromise;
	}

	private async request<T>(
		method: string,
		path: string,
		body?: unknown,
		options?: RequestInit,
		_retry = false
	): Promise<ApiResponse<T>> {
		const headers: Record<string, string> = {
			'Content-Type': 'application/json',
			...(options?.headers as Record<string, string>)
		};

		if (this.token) {
			headers['Authorization'] = `Bearer ${this.token}`;
		}

		try {
			const response = await fetch(`${this.baseUrl}${path}`, {
				method,
				headers,
				body: body ? JSON.stringify(body) : undefined,
				...options
			});

			const text = await response.text();

			if (!text) {
				return response.ok
					? { data: null as T, error: null }
					: { data: null, error: { code: 'ERROR', message: `HTTP ${response.status}` } };
			}

			let data: ApiResponse<T>;
			try {
				data = JSON.parse(text);
			} catch {
				return {
					data: null,
					error: {
						code: 'PARSE_ERROR',
						message: `Server returned non-JSON response (${response.status}): ${text.slice(0, 120)}`
					}
				};
			}

			if (response.status === 401) {
				if (!_retry) {
					const refreshed = await this._tryRefresh();
					if (refreshed) {
						return this.request<T>(method, path, body, options, true);
					}
				}
				// Refresh failed or already retried — session is dead.
				authStore.markSessionExpired();
			}

			if (response.status === 403) {
				authStore.markForbidden();
			}

			return data;
		} catch (error) {
			return {
				data: null,
				error: {
					code: 'NETWORK_ERROR',
					message: error instanceof Error ? error.message : 'Network request failed'
				}
			};
		}
	}

	async get<T>(path: string): Promise<ApiResponse<T>> {
		return this.request<T>('GET', path);
	}

	async post<T>(path: string, body?: unknown): Promise<ApiResponse<T>> {
		return this.request<T>('POST', path, body);
	}

	async put<T>(path: string, body?: unknown): Promise<ApiResponse<T>> {
		return this.request<T>('PUT', path, body);
	}

	async patch<T>(path: string, body?: unknown): Promise<ApiResponse<T>> {
		return this.request<T>('PATCH', path, body);
	}

	async delete<T>(path: string): Promise<ApiResponse<T>> {
		return this.request<T>('DELETE', path);
	}

	async postForm<T>(path: string, form: FormData): Promise<ApiResponse<T>> {
		const headers: Record<string, string> = {};
		if (this.token) headers['Authorization'] = `Bearer ${this.token}`;
		try {
			const response = await fetch(`${this.baseUrl}${path}`, {
				method: 'POST',
				headers,
				body: form,
				credentials: 'same-origin',
			});
			const text = await response.text();
			if (!text) return { data: null as T, error: null };
			try {
				return JSON.parse(text) as ApiResponse<T>;
			} catch {
				return { data: null, error: { code: 'PARSE_ERROR', message: `Non-JSON: ${text.slice(0, 120)}` } };
			}
		} catch (error) {
			return { data: null, error: { code: 'NETWORK_ERROR', message: error instanceof Error ? error.message : 'Network error' } };
		}
	}

	// ─── Auth ────────────────────────────────────────────────────────
	async login(
		email: string,
		password: string
	): Promise<ApiResponse<{ access_token: string; user: User }>> {
		return this.post('/auth/login', { email, password });
	}

	async register(
		email: string,
		password: string
	): Promise<ApiResponse<{ id: string; email: string }>> {
		return this.post('/auth/register', { email, password });
	}

	async getMe(): Promise<ApiResponse<User>> {
		return this.get('/auth/me');
	}

	async logout(): Promise<ApiResponse<null>> {
		// No body needed — server reads and clears the HttpOnly refresh cookie.
		return this.post('/auth/logout');
	}

	async changePassword(currentPassword: string, newPassword: string): Promise<ApiResponse<{ message: string }>> {
		return this.put('/auth/me/password', { current_password: currentPassword, new_password: newPassword });
	}

	// ─── Setup ───────────────────────────────────────────────────────
	async getSetupStatus(): Promise<ApiResponse<{ initialized: boolean; step: string }>> {
		return this.get('/setup/status');
	}

	async checkDocker(): Promise<ApiResponse<{ success: boolean; message: string }>> {
		return this.post('/setup/check-docker');
	}

	async setupInit(data: {
		admin_email: string;
		admin_password: string;
		org_name: string;
		org_slug?: string;
	}): Promise<ApiResponse<unknown>> {
		return this.post('/setup/init', data);
	}

	// ─── Orgs ────────────────────────────────────────────────────────
	async getOrgs(): Promise<ApiResponse<Organization[]>> {
		return this.get('/orgs');
	}

	async createOrg(name: string, slug: string): Promise<ApiResponse<Organization>> {
		return this.post('/orgs', { name, slug });
	}

	// ─── Projects ────────────────────────────────────────────────────
	async getProjects(orgId: string): Promise<ApiResponse<Project[]>> {
		return this.get(`/orgs/${orgId}/projects`);
	}

	async createProject(
		orgId: string,
		name: string,
		slug: string
	): Promise<ApiResponse<Project>> {
		return this.post(`/orgs/${orgId}/projects`, { name, slug });
	}

	async getProject(orgId: string, projectId: string): Promise<ApiResponse<Project>> {
		return this.get(`/orgs/${orgId}/projects/${projectId}`);
	}

	async deleteProject(orgId: string, projectId: string): Promise<ApiResponse<{ message: string }>> {
		return this.delete(`/orgs/${orgId}/projects/${projectId}`);
	}

	async patchNodePositions(
		orgId: string,
		projectId: string,
		positions: Record<string, { x: number; y: number }>
	): Promise<ApiResponse<{ ok: boolean }>> {
		return this.patch(`/orgs/${orgId}/projects/${projectId}/node-positions`, { positions });
	}

	// ─── Services ────────────────────────────────────────────────────
	async getServices(projectId: string): Promise<ApiResponse<Service[]>> {
		return this.get(`/projects/${projectId}/services`);
	}

	async getService(projectId: string, serviceId: string): Promise<ApiResponse<Service>> {
		return this.get(`/projects/${projectId}/services/${serviceId}`);
	}

	async createService(
		projectId: string,
		data: {
			name: string;
			slug: string;
			type: string;
			directory_path?: string;
			replicas?: number;
			git_repo_url?: string | null;
			git_branch?: string;
		}
	): Promise<ApiResponse<Service>> {
		return this.post(`/projects/${projectId}/services`, data);
	}

	async updateService(
		projectId: string,
		serviceId: string,
		data: Partial<{
			name: string;
			replicas: number;
			ports: string[];
			image: string;
			git_branch: string;
			auto_deploy: boolean;
		}>
	): Promise<ApiResponse<Service>> {
		return this.put(`/projects/${projectId}/services/${serviceId}`, data);
	}

	async deleteService(projectId: string, serviceId: string): Promise<ApiResponse<null>> {
		return this.delete(`/projects/${projectId}/services/${serviceId}`);
	}

	async getWebhookToken(projectId: string, serviceId: string): Promise<ApiResponse<{ token: string }>> {
		return this.get(`/projects/${projectId}/services/${serviceId}/webhook`);
	}

	async rotateWebhookToken(projectId: string, serviceId: string): Promise<ApiResponse<{ token: string }>> {
		return this.post(`/projects/${projectId}/services/${serviceId}/webhook/rotate`, {});
	}

	async getConnectionInfo(projectId: string, serviceId: string): Promise<ApiResponse<import('./types').ConnectionInfo>> {
		return this.get(`/projects/${projectId}/services/${serviceId}/connection`);
	}

	async revealEnv(projectId: string, serviceId: string, envId: string): Promise<ApiResponse<{ value: string }>> {
		return this.get(`/projects/${projectId}/services/${serviceId}/env/${envId}/reveal`);
	}

	async deployService(serviceId: string): Promise<ApiResponse<Deployment>> {
		return this.post(`/services/${serviceId}/deploy`);
	}

	async stopService(serviceId: string): Promise<ApiResponse<null>> {
		return this.post(`/services/${serviceId}/stop`);
	}

	async redeployService(serviceId: string): Promise<ApiResponse<Deployment>> {
		return this.post(`/services/${serviceId}/redeploy`);
	}

	async rollbackDeployment(serviceId: string, deploymentId: string): Promise<ApiResponse<{ deployment_id: string }>> {
		return this.post(`/services/${serviceId}/deployments/${deploymentId}/rollback`);
	}

	async restartService(serviceId: string): Promise<ApiResponse<unknown>> {
		return this.post(`/services/${serviceId}/restart`);
	}

	// ─── Static Sites ────��────────────────────────────────────────────
	async getStaticConfig(serviceId: string): Promise<ApiResponse<StaticSiteConfig>> {
		return this.get(`/services/${serviceId}/static/config`);
	}

	async updateStaticConfig(serviceId: string, body: Partial<StaticSiteConfig>): Promise<ApiResponse<StaticSiteConfig>> {
		return this.put(`/services/${serviceId}/static/config`, body);
	}

	async uploadStaticSite(serviceId: string, file: File, message?: string): Promise<ApiResponse<{ deployment_id: string }>> {
		const form = new FormData();
		form.append('artifact', file);
		if (message) form.append('message', message);
		return this.postForm(`/services/${serviceId}/static/upload`, form);
	}

	// ─── Env Vars ────────────────────────────────────────────────────
	async getServiceEnvs(serviceId: string): Promise<ApiResponse<ServiceEnv[]>> {
		return this.get(`/services/${serviceId}/env`);
	}

	async upsertEnv(
		serviceId: string,
		data: { key: string; value: string; is_secret: boolean }
	): Promise<ApiResponse<ServiceEnv>> {
		return this.post(`/services/${serviceId}/env`, data);
	}

	async deleteEnv(serviceId: string, envId: string): Promise<ApiResponse<null>> {
		return this.delete(`/services/${serviceId}/env/${envId}`);
	}

	async bulkSetEnvs(
		serviceId: string,
		envs: Array<{ key: string; value: string; is_secret: boolean }>
	): Promise<ApiResponse<ServiceEnv[]>> {
		return this.post(`/services/${serviceId}/env/bulk`, { envs });
	}

	// ─── Deployments ─────────────────────────────────────────────────
	async getDeployments(serviceId: string): Promise<ApiResponse<Deployment[]>> {
		return this.get(`/services/${serviceId}/deployments`);
	}

	async getDeployment(deploymentId: string): Promise<ApiResponse<Deployment>> {
		return this.get(`/deployments/${deploymentId}`);
	}

	async getDeploymentSteps(deploymentId: string): Promise<ApiResponse<DeploymentStep[]>> {
		return this.get(`/deployments/${deploymentId}/steps`);
	}

	async getDeploymentLogs(
		deploymentId: string,
		stepId?: string
	): Promise<ApiResponse<DeploymentLog[]>> {
		const path = stepId
			? `/deployments/${deploymentId}/logs?step_id=${stepId}`
			: `/deployments/${deploymentId}/logs`;
		return this.get(path);
	}

	async cancelDeployment(deploymentId: string): Promise<ApiResponse<Deployment>> {
		return this.post(`/deployments/${deploymentId}/cancel`);
	}

	// ─── Containers ──────────────────────────────────────────────────
	async getServiceContainers(serviceId: string): Promise<ApiResponse<Container[]>> {
		return this.get(`/services/${serviceId}/containers`);
	}

	async getProjectContainers(projectId: string): Promise<ApiResponse<Container[]>> {
		return this.get(`/projects/${projectId}/containers`);
	}

	// ─── Topology ────────────────────────────────────────────────────
	async getTopology(projectId: string): Promise<ApiResponse<Topology>> {
		return this.get(`/projects/${projectId}/topology`);
	}

	// ─── Containers ──────────────────────────────────────────────────
	async deleteContainer(serviceId: string, containerId: string): Promise<ApiResponse<null>> {
		return this.delete(`/services/${serviceId}/containers/${containerId}`);
	}

	async inspectContainer(serviceId: string, containerId: string): Promise<ApiResponse<ContainerInspect>> {
		return this.get(`/services/${serviceId}/containers/${containerId}/inspect`);
	}

	async stopContainer(serviceId: string, containerId: string): Promise<ApiResponse<unknown>> {
		return this.post(`/services/${serviceId}/containers/${containerId}/stop`);
	}

	async restartContainer(serviceId: string, containerId: string): Promise<ApiResponse<unknown>> {
		return this.post(`/services/${serviceId}/containers/${containerId}/restart`);
	}

	// ─── Resources ───────────────────────────────────────────────────
	async getDomains(serviceId: string): Promise<ApiResponse<Domain[]>> {
		return this.get(`/services/${serviceId}/domains`);
	}

	async createDomain(
		serviceId: string,
		data: { hostname: string; tls_enabled: boolean; cert_provider: string; port: number | null }
	): Promise<ApiResponse<Domain>> {
		return this.post(`/services/${serviceId}/domains`, data);
	}

	async deleteDomain(serviceId: string, domainId: string): Promise<ApiResponse<null>> {
		return this.delete(`/services/${serviceId}/domains/${domainId}`);
	}

	async checkDomainDns(serviceId: string, domainId: string): Promise<ApiResponse<DnsCheckResult>> {
		return this.get(`/services/${serviceId}/domains/${domainId}/check-dns`);
	}

	async getVolumes(serviceId: string): Promise<ApiResponse<Volume[]>> {
		return this.get(`/services/${serviceId}/volumes`);
	}

	async getNetworks(projectId: string): Promise<ApiResponse<Network[]>> {
		return this.get(`/projects/${projectId}/networks`);
	}

	async getProjectVolumes(projectId: string): Promise<ApiResponse<Volume[]>> {
		return this.get(`/projects/${projectId}/volumes`);
	}

	async attachVolumeToService(projectId: string, volumeId: string, serviceId: string): Promise<ApiResponse<unknown>> {
		return this.post(`/projects/${projectId}/volumes/${volumeId}/attach`, { service_id: serviceId });
	}

	async createNetwork(
		projectId: string,
		data: { name: string; driver: string; subnet: string }
	): Promise<ApiResponse<Network>> {
		return this.post(`/projects/${projectId}/networks`, data);
	}

	async attachNetwork(projectId: string, networkId: string, serviceId: string): Promise<ApiResponse<unknown>> {
		return this.post(`/projects/${projectId}/networks/${networkId}/attach`, { service_id: serviceId });
	}

	async detachNetwork(projectId: string, networkId: string, serviceId: string): Promise<ApiResponse<unknown>> {
		return this.delete(`/projects/${projectId}/networks/${networkId}/services/${serviceId}`);
	}

	async getServiceNetworks(serviceId: string): Promise<ApiResponse<Network[]>> {
		return this.get(`/services/${serviceId}/networks`);
	}

	// ─── Templates ───────────────────────────────────────────────────
	async getTemplates(): Promise<ApiResponse<Template[]>> {
		return this.get('/templates');
	}

	async getTemplate(id: string): Promise<ApiResponse<Template>> {
		return this.get(`/templates/${id}`);
	}

	// ─── Members ─────────────────────────────────────────────────────
	async getMembers(orgId: string): Promise<ApiResponse<OrgMember[]>> {
		return this.get(`/orgs/${orgId}/members`);
	}

	async inviteMember(
		orgId: string,
		email: string,
		role: string,
		permissions: string[] = [],
		project_assignments: ProjectAssignment[] = []
	): Promise<ApiResponse<Invitation>> {
		return this.post(`/orgs/${orgId}/members/invite`, { email, role, permissions, project_assignments });
	}

	async changeMemberRole(orgId: string, userId: string, role: string): Promise<ApiResponse<OrgMember>> {
		return this.put(`/orgs/${orgId}/members/${userId}/role`, { role });
	}

	async setMemberPermissions(orgId: string, userId: string, permissions: string[]): Promise<ApiResponse<string[]>> {
		return this.put(`/orgs/${orgId}/members/${userId}/permissions`, { permissions });
	}

	async removeMember(orgId: string, userId: string): Promise<ApiResponse<unknown>> {
		return this.delete(`/orgs/${orgId}/members/${userId}`);
	}

	// ─── Invitations ──────────────────────────────────────────────────
	async getAuditLogs(orgId: string, cursor?: string, limit = 50): Promise<ApiResponse<{ items: AuditLogEntry[]; next_cursor: string | null }>> {
		const qs = cursor
			? `?cursor=${encodeURIComponent(cursor)}&limit=${limit}`
			: `?limit=${limit}`;
		return this.get(`/orgs/${orgId}/audit-logs${qs}`);
	}

	async getInvitations(orgId: string): Promise<ApiResponse<Invitation[]>> {
		return this.get(`/orgs/${orgId}/invitations`);
	}

	async cancelInvitation(orgId: string, invitationId: string): Promise<ApiResponse<unknown>> {
		return this.delete(`/orgs/${orgId}/invitations/${invitationId}`);
	}

	async getMemberProjects(orgId: string, userId: string): Promise<ApiResponse<MemberProjectAssignment[]>> {
		return this.get(`/orgs/${orgId}/members/${userId}/projects`);
	}

	async setMemberProjects(
		orgId: string,
		userId: string,
		assignments: { project_id: string; permissions: string[] }[]
	): Promise<ApiResponse<MemberProjectAssignment[]>> {
		return this.put(`/orgs/${orgId}/members/${userId}/projects`, { assignments });
	}

	// ─── Public Invitations (no auth) ────────────────────────────────
	async getInvitation(token: string): Promise<ApiResponse<PublicInvite>> {
		return this.get(`/invite/${token}`);
	}

	async completeInvitation(
		token: string,
		password: string
	): Promise<ApiResponse<{ access_token: string; user_id: string; email: string }>> {
		return this.post(`/invite/${token}/complete`, { password });
	}

	async rejectInvitation(token: string): Promise<ApiResponse<{ message: string }>> {
		return this.post(`/invite/${token}/reject`, {});
	}

	async acceptInvitation(
		orgId: string,
		token: string
	): Promise<ApiResponse<{ message: string; org_id: string; role: string }>> {
		return this.post(`/orgs/${orgId}/invitations/${token}/accept`, {});
	}

	async getMyMembership(orgId: string): Promise<ApiResponse<OrgMember>> {
		return this.get(`/orgs/${orgId}/members/me`);
	}

	// ─── Docker Compose ──────────────────────────────────────────────
	async importCompose(
		projectId: string,
		composeYaml: string,
		rootName: string,
		rootSlug: string
	): Promise<ApiResponse<ImportComposeResponse>> {
		return this.post(`/projects/${projectId}/compose/import`, {
			compose_yaml: composeYaml,
			root_name: rootName,
			root_slug: rootSlug
		});
	}

	// ─── Traefik Files ────────────────────────────────────────────────
	async getTraefikStatic(): Promise<ApiResponse<TraefikFileResponse>> {
		return this.get('/settings/traefik/static');
	}

	async getTraefikDynamic(): Promise<ApiResponse<TraefikDynamicResponse>> {
		return this.get('/settings/traefik/dynamic');
	}

	async getTraefikDynamicFile(filename: string): Promise<ApiResponse<TraefikFileResponse>> {
		return this.get(`/settings/traefik/dynamic/${encodeURIComponent(filename)}`);
	}

	// ─── Admin ────────────────────────────────────────────────────────
	async checkVersion(): Promise<ApiResponse<import('./types').VersionInfo>> {
		return this.get('/admin/version');
	}

	async getSwarmNodes(orgId: string): Promise<ApiResponse<import('./types').SwarmNode[]>> {
		return this.get(`/admin/docker/nodes?org_id=${orgId}`);
	}

	async getSwarmJoinTokens(orgId: string): Promise<ApiResponse<import('./types').SwarmJoinTokens>> {
		return this.get(`/admin/docker/swarm/join-tokens?org_id=${orgId}`);
	}

	async triggerUpdate(): Promise<ApiResponse<{ message: string; output: string }>> {
		return this.post('/admin/update');
	}

	// ─── Admin Deployments ────────────────────────────────────────────
	async listAllDeployments(orgId: string, params?: { status?: string; page?: number; per_page?: number }): Promise<ApiResponse<import('./types').AdminDeploymentsResponse>> {
		const qs = new URLSearchParams();
		qs.set('org_id', orgId);
		if (params?.status) qs.set('status', params.status);
		if (params?.page) qs.set('page', String(params.page));
		if (params?.per_page) qs.set('per_page', String(params.per_page));
		return this.get(`/admin/deployments?${qs}`);
	}

	// ─── API Key Management ───────────────────────────────────────────
	async listApiKeys(orgId: string): Promise<ApiResponse<import('./types').ApiKeyItem[]>> {
		return this.get(`/admin/api-keys?org_id=${orgId}`);
	}

	async createApiKey(orgId: string, body: import('./types').CreateApiKeyRequest): Promise<ApiResponse<import('./types').CreatedApiKey>> {
		return this.post(`/admin/api-keys?org_id=${orgId}`, body);
	}

	async revokeApiKey(orgId: string, keyId: string): Promise<ApiResponse<null>> {
		return this.delete(`/admin/api-keys/${keyId}?org_id=${orgId}`);
	}

	// ─── DB Client ──────────────────────────────────────────────────────────────

	async getDbMeta(serviceId: string): Promise<ApiResponse<import('./types').DbMeta>> {
		return this.get(`/services/${serviceId}/db/meta`);
	}

	async runDbQuery(serviceId: string, body: import('./types').DbQueryRequest): Promise<ApiResponse<import('./types').DbQueryResult>> {
		return this.post(`/services/${serviceId}/db/query`, body);
	}
}

export const api = new ApiClient();
export default api;
