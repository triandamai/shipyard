export interface SpikeAlert {
	metric: 'cpu' | 'mem' | 'disk' | 'net';
	value: number;
	threshold: number;
	container_id: string | null;
	node_id: string;
	ts: number;
	id: string;
}

const MAX_ALERTS = 50;

function createAlertsStore() {
	let alerts = $state<SpikeAlert[]>([]);

	function add(raw: Omit<SpikeAlert, 'id'>) {
		const alert: SpikeAlert = { ...raw, id: `${Date.now()}-${Math.random()}` };
		alerts = [alert, ...alerts].slice(0, MAX_ALERTS);
	}

	function dismiss(id: string) {
		alerts = alerts.filter((a) => a.id !== id);
	}

	return {
		get alerts() { return alerts; },
		add,
		dismiss,
	};
}

export const alertsStore = createAlertsStore();
