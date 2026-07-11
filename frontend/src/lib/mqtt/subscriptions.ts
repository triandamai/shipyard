import { subscribeTopic, unsubscribeTopic } from './client';

/**
 * Subscribe to all MQTT topics for a given org/project/service scope.
 * Returns an unsubscribe function.
 */
export function subscribeToService(
	orgId: string,
	projectId: string,
	serviceId: string
): () => void {
	const prefix = `platform/orgs/${orgId}/projects/${projectId}/services/${serviceId}`;

	const topics = [
		`${prefix}/status`,
		`${prefix}/containers`,
		`${prefix}/replicas/count`
	];

	topics.forEach(subscribeTopic);

	return () => topics.forEach(unsubscribeTopic);
}

/**
 * Subscribe to deployment events for a specific deployment.
 */
export function subscribeToDeployment(
	orgId: string,
	projectId: string,
	serviceId: string,
	deploymentId: string
): () => void {
	const prefix = `platform/orgs/${orgId}/projects/${projectId}/services/${serviceId}/deployments/${deploymentId}`;

	const topics = [`${prefix}/status`];

	topics.forEach(subscribeTopic);

	return () => topics.forEach(unsubscribeTopic);
}

/**
 * Subscribe to all step log and status events for a deployment using MQTT wildcards.
 * Use this when opening the deployment log overlay — it covers every step without
 * needing individual step IDs up front.
 */
export function subscribeToDeploymentSteps(
	orgId: string,
	projectId: string,
	serviceId: string,
	deploymentId: string
): () => void {
	const prefix = `platform/orgs/${orgId}/projects/${projectId}/services/${serviceId}/deployments/${deploymentId}/steps`;

	const topics = [`${prefix}/+/log`, `${prefix}/+/status`];

	topics.forEach(subscribeTopic);

	return () => topics.forEach(unsubscribeTopic);
}

/**
 * Subscribe to project topology updates AND all service/deployment events for
 * the project so the canvas receives realtime status changes without relying
 * solely on the org-level wildcard subscription from the layout.
 */
export function subscribeToTopology(orgId: string, projectId: string): () => void {
	const prefix = `platform/orgs/${orgId}/projects/${projectId}`;
	const topics = [
		`${prefix}/topology`,
		`${prefix}/services/+/status`,
		`${prefix}/services/+/containers`,
		`${prefix}/services/+/deployments/+/status`,
	];
	topics.forEach(subscribeTopic);
	return () => topics.forEach(unsubscribeTopic);
}

/**
 * Subscribe to all events for an org (wildcard). Used to power global toast notifications.
 */
export function subscribeToOrgEvents(orgId: string): () => void {
	const topic = `platform/orgs/${orgId}/#`;
	subscribeTopic(topic);
	return () => unsubscribeTopic(topic);
}
