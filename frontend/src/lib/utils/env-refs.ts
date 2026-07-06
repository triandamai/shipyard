import { api } from '$lib/api/client';

const PLATFORM_REF_PATTERN = /\bplatform-([a-z][a-z0-9-]*)\b/g;

export interface EnvRef {
	envKey: string;
	fullMatch: string;
	resourceSlug: string;
}

export interface ResolvedRef {
	ref: EnvRef;
	type: 'service' | 'network' | 'volume';
	id: string;
	name: string;
	nodeId: string;
	projectId: string;
	projectName: string;
	isCrossProject: boolean;
}

export function parseEnvRefs(envs: Array<{ key: string; value: string }>): EnvRef[] {
	const refs: EnvRef[] = [];
	const seen = new Set<string>();
	for (const env of envs) {
		const re = new RegExp(PLATFORM_REF_PATTERN.source, 'g');
		let m: RegExpExecArray | null;
		while ((m = re.exec(env.value)) !== null) {
			const slug = m[1].toLowerCase();
			const dedupeKey = `${env.key}::${slug}`;
			if (!seen.has(dedupeKey)) {
				seen.add(dedupeKey);
				refs.push({ envKey: env.key, fullMatch: m[0], resourceSlug: slug });
			}
		}
	}
	return refs;
}

export async function resolveEnvRefs(
	refs: EnvRef[],
	orgId: string,
	currentProjectId: string
): Promise<ResolvedRef[]> {
	if (refs.length === 0) return [];

	const projectsRes = await api.getProjects(orgId);
	const projects = projectsRes.data ?? [];

	const results: ResolvedRef[] = [];

	for (const ref of refs) {
		let found = false;

		for (const project of projects) {
			if (found) break;

			const svcsRes = await api.getServices(project.id);
			for (const svc of svcsRes.data ?? []) {
				const match =
					svc.slug.toLowerCase() === ref.resourceSlug ||
					svc.name.toLowerCase().replace(/\s+/g, '-') === ref.resourceSlug;
				if (match) {
					results.push({
						ref, type: 'service', id: svc.id, name: svc.name,
						nodeId: `svc_${svc.id}`,
						projectId: project.id, projectName: project.name,
						isCrossProject: project.id !== currentProjectId,
					});
					found = true;
					break;
				}
			}
			if (found) break;

			const netsRes = await api.getNetworks(project.id);
			for (const net of netsRes.data ?? []) {
				if (net.name.toLowerCase().replace(/\s+/g, '-') === ref.resourceSlug) {
					results.push({
						ref, type: 'network', id: net.id, name: net.name,
						nodeId: `net_${net.id}`,
						projectId: project.id, projectName: project.name,
						isCrossProject: project.id !== currentProjectId,
					});
					found = true;
					break;
				}
			}
			if (found) break;

			const volsRes = await api.getProjectVolumes(project.id);
			for (const vol of volsRes.data ?? []) {
				if (vol.name.toLowerCase().replace(/\s+/g, '-') === ref.resourceSlug) {
					results.push({
						ref, type: 'volume', id: vol.id, name: vol.name,
						nodeId: `vol_${vol.id}`,
						projectId: project.id, projectName: project.name,
						isCrossProject: project.id !== currentProjectId,
					});
					found = true;
					break;
				}
			}
		}
	}

	return results;
}
