import { writable, derived, get } from 'svelte/store';
import type { LogLevel } from '$lib/api/types';

const MAX_LOG_LINES = 10_000;

export interface LogLine {
	id: string;
	timestamp: string;
	level: LogLevel;
	message: string;
	replicaId?: string;
}

interface LogState {
	lines: LogLine[];
	levelFilter: LogLevel | null;
	activeReplicaId: string | null;
	isFollowing: boolean;
	isLoading: boolean;
}

const initialState: LogState = {
	lines: [],
	levelFilter: null,
	activeReplicaId: null,
	isFollowing: true,
	isLoading: false
};

function createLogStore() {
	const store = writable<LogState>(initialState);
	const { subscribe, set, update } = store;

	return {
		subscribe,

		/** Append a log line to the ring buffer. Drops oldest when full. */
		append(line: LogLine) {
			update((state) => {
				const newLines = [...state.lines, line];
				// Ring buffer: cap at MAX_LOG_LINES, drop from the top (oldest)
				if (newLines.length > MAX_LOG_LINES) {
					newLines.splice(0, newLines.length - MAX_LOG_LINES);
				}
				return { ...state, lines: newLines };
			});
		},

		/** Append multiple log lines at once (e.g., from history fetch). */
		appendBatch(lines: LogLine[]) {
			update((state) => {
				const newLines = [...state.lines, ...lines];
				if (newLines.length > MAX_LOG_LINES) {
					newLines.splice(0, newLines.length - MAX_LOG_LINES);
				}
				return { ...state, lines: newLines };
			});
		},

		/** Clear all log lines. */
		clear() {
			update((state) => ({ ...state, lines: [] }));
		},

		setLevelFilter(level: LogLevel | null) {
			update((state) => ({ ...state, levelFilter: level }));
		},

		setActiveReplicaId(replicaId: string | null) {
			update((state) => ({ ...state, activeReplicaId: replicaId, lines: [] }));
		},

		setFollowing(following: boolean) {
			update((state) => ({ ...state, isFollowing: following }));
		},

		setLoading(loading: boolean) {
			update((state) => ({ ...state, isLoading: loading }));
		},

		/** Get filtered lines based on current level filter. */
		getFilteredLines(): LogLine[] {
			const state = get(store);
			if (!state.levelFilter) return state.lines;
			return state.lines.filter((l) => l.level === state.levelFilter);
		}
	};
}

export const logStore = createLogStore();
