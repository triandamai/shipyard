import { eventBus } from '../eventBus';
import { deploymentStore } from '$lib/stores/deployment.store';
import { logStore, type LogLine } from '$lib/stores/log.store';
import type { MqttPayload } from '$lib/api/types';

/**
 * Global deployment MQTT handler.
 * Must be called once at app init (layout.svelte onMount).
 * Handles deployment status, step status changes, and step log lines.
 */
export function initDeploymentHandler() {
	eventBus.on('*', (topic, payload) => {
		if (typeof topic !== 'string') return;
		const p = payload as MqttPayload;

		// Deployment status  (…/deployments/{id}/status)
		const deployStatusMatch = topic.match(/\/deployments\/([^/]+)\/status$/);
		if (deployStatusMatch) {
			const deploymentId = deployStatusMatch[1];
			const status = (p.meta as any)?.status ?? p.event;
			deploymentStore.updateDeploymentStatus(deploymentId, status);
			return;
		}

		// Step status change  (…/steps/{id}/status)
		const stepStatusMatch = topic.match(/\/steps\/([^/]+)\/status$/);
		if (stepStatusMatch) {
			const meta = p.meta as any;
			if (meta?.step_id && meta?.status) {
				deploymentStore.updateStepStatus(meta.step_id, meta.status);
			}
			return;
		}

		// Step log line  (…/steps/{id}/log)
		const stepLogMatch = topic.match(/\/steps\/([^/]+)\/log$/);
		if (stepLogMatch) {
			const line: LogLine = {
				id: `${Date.now()}-${Math.random()}`,
				timestamp: p.timestamp,
				level: p.level ?? 'info',
				message: p.message ?? ''
			};
			logStore.append(line);
			return;
		}
	});
}
