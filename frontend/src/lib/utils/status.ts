export type DotStatus = 'running' | 'pending' | 'deploying' | 'warning' | 'failed' | 'stopped';

// Maps every status the old global `.status-dot` class supported onto
// StatusDot's states. Pulsing (pending/deploying) is reserved for genuinely
// transitional states.
const DOT_STATUS: Record<string, DotStatus> = {
	running: 'running',
	pending: 'pending',
	preparing: 'pending',
	queued: 'pending',
	stopping: 'pending',
	deploying: 'deploying',
	need_attention: 'warning',
	failed: 'failed',
	rejected: 'failed',
	stopped: 'stopped',
	shutdown: 'stopped',
	complete: 'stopped'
};

export function toDotStatus(status: string | null | undefined): DotStatus {
	return DOT_STATUS[(status ?? '').toLowerCase()] ?? 'stopped';
}
