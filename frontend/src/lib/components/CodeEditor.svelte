<script lang="ts">
	import { onMount } from 'svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { yaml } from '@codemirror/lang-yaml';
	import { EditorState } from '@codemirror/state';

	interface Props {
		value?: string;
		onChange?: (val: string) => void;
		height?: string;
		readonly?: boolean;
	}

	let { value = '', onChange, height = '280px', readonly = false }: Props = $props();

	let container: HTMLDivElement | undefined = $state();
	let view: EditorView | null = null;

	// Called by parent to reset content imperatively (e.g. "clear")
	export function setValue(newVal: string) {
		if (!view) return;
		view.dispatch({
			changes: { from: 0, to: view.state.doc.length, insert: newVal },
		});
	}

	onMount(() => {
		if (!container) return;

		const theme = EditorView.theme({
			'&': { height, display: 'flex', flexDirection: 'column' },
			'&.cm-focused': { outline: 'none' },
			'.cm-scroller': {
				overflow: 'auto',
				fontFamily: 'var(--font-mono)',
				fontSize: '12.5px',
				lineHeight: '1.65',
				flex: '1',
			},
			'.cm-content': { padding: '12px 14px', minHeight: '100px', color: 'var(--text-secondary)' },
			'.cm-editor': { backgroundColor: 'var(--bg-base)' },
			'.cm-gutters': {
				backgroundColor: 'var(--bg-elevated)',
				borderRight: '1px solid var(--border)',
				color: 'var(--text-dim)',
				paddingRight: '8px',
				paddingLeft: '6px',
				minWidth: '32px',
			},
			'.cm-activeLineGutter': { backgroundColor: 'transparent' },
			'.cm-activeLine': { backgroundColor: 'rgba(255,255,255,0.025)' },
			'.cm-selectionBackground, ::selection': {
				backgroundColor: 'rgba(37,99,235,0.25) !important',
			},
			'.cm-cursor': { borderLeftColor: 'var(--accent)', borderLeftWidth: '2px' },
			'.cm-matchingBracket': { outline: '1px solid var(--accent)', borderRadius: '2px' },
		});

		const extensions = [
			basicSetup,
			yaml(),
			theme,
			EditorView.lineWrapping,
			EditorView.updateListener.of((update) => {
				if (update.docChanged) onChange?.(update.state.doc.toString());
			}),
		];

		if (readonly) extensions.push(EditorState.readOnly.of(true));

		view = new EditorView({
			state: EditorState.create({ doc: value, extensions }),
			parent: container,
		});

		return () => {
			view?.destroy();
			view = null;
		};
	});
</script>

<div bind:this={container} class="editor-wrap"></div>

<style>
	.editor-wrap {
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-base);
		transition: border-color var(--transition-fast);
	}

	.editor-wrap:focus-within {
		border-color: var(--accent);
	}
</style>
