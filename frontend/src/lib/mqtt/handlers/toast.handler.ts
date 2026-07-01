import { eventBus } from '../eventBus';
import { toastStore } from '$lib/stores/toast.store';
import type { MqttPayload } from '$lib/api/types';

/**
 * Global MQTT → toast handler.
 * Call once at app init. Maps known event names to user-facing toast notifications.
 */
export function initToastHandler() {
	eventBus.on('*', (topic, payload) => {
		if (typeof topic !== 'string') return;
		const p = payload as MqttPayload;

		switch (p.event) {
			case 'service.created':
				toastStore.add({
					type: 'success',
					title: 'Service created',
					message: (p.meta as any)?.service_name
				});
				break;

			case 'service.updated':
				toastStore.add({
					type: 'info',
					title: 'Service updated',
					message: (p.meta as any)?.service_name
				});
				break;

			case 'service.deleted':
				toastStore.add({ type: 'warning', title: 'Service deleted' });
				break;

			case 'service.env.changed':
				toastStore.add({ type: 'info', title: 'Environment variables updated' });
				break;

			case 'deployment.started':
				toastStore.add({
					type: 'info',
					title: 'Deployment started',
					message: (p.meta as any)?.triggered_by
						? `Triggered by ${(p.meta as any).triggered_by}`
						: undefined
				});
				break;

			case 'deployment.success':
				toastStore.add({ type: 'success', title: 'Deployment completed' });
				break;

			case 'deployment.failed':
				toastStore.add({ type: 'error', title: 'Deployment failed' });
				break;

			case 'topology.changed': {
				const meta = p.meta as any;
				if (meta?.resource === 'domain') {
					if (meta?.reason === 'deleted') {
						toastStore.add({ type: 'info', title: 'Domain removed' });
					} else {
						toastStore.add({ type: 'success', title: 'Domain added' });
					}
				}
				break;
			}

			case 'settings.traefik.updated':
				toastStore.add({ type: 'success', title: 'Traefik settings saved' });
				break;

			case 'org.member.joined':
				toastStore.add({
					type: 'info',
					title: 'New member joined',
					message: (p.meta as any)?.email
				});
				break;

			case 'org.member.projects.updated':
				toastStore.add({ type: 'info', title: 'Member project access updated' });
				break;
		}
	});
}
