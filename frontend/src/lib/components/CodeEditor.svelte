<script lang="ts">
	import { onMount } from 'svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { yaml } from '@codemirror/lang-yaml';
	import { javascript } from '@codemirror/lang-javascript';
	import { json } from '@codemirror/lang-json';
	import { EditorState, Compartment } from '@codemirror/state';
	import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
	import { tags as t } from '@lezer/highlight';

	interface Props {
		value?: string;
		onChange?: (val: string) => void;
		height?: string;
		readonly?: boolean;
		language?: 'yaml' | 'javascript' | 'json' | 'plain';
	}

	let { value = '', onChange, height = '280px', readonly = false, language = 'yaml' }: Props = $props();

	let container: HTMLDivElement | undefined = $state();
	let view: EditorView | null = null;

	const languageCompartment = new Compartment();

	function languageExtension(lang: string) {
		switch (lang) {
			case 'javascript': return javascript();
			case 'json': return json();
			case 'yaml': return yaml();
			default: return [];
		}
	}

	// Maps token types to the app's own design-token colors (layout.css) so
	// the editor reads as part of Shipyard rather than a generic default
	// CodeMirror palette — and follows the same light/dark theme switch
	// automatically, since these resolve as real CSS custom properties.
	const syntaxTheme = HighlightStyle.define([
		{ tag: [t.keyword, t.controlKeyword, t.operatorKeyword, t.definitionKeyword, t.moduleKeyword], color: 'var(--accent)', fontWeight: 600 },
		{ tag: [t.string, t.special(t.string), t.docString, t.regexp], color: 'var(--accent-green)' },
		{ tag: [t.number, t.integer, t.float, t.bool, t.null, t.atom, t.escape], color: 'var(--accent-yellow)' },
		{ tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: 'var(--text-dim)', fontStyle: 'italic' },
		{ tag: t.function(t.variableName), color: 'var(--accent-blue)' },
		{ tag: t.definition(t.variableName), color: 'var(--text-primary)' },
		{ tag: t.variableName, color: 'var(--text-secondary)' },
		{ tag: [t.propertyName, t.attributeName], color: 'var(--text-primary)' },
		{ tag: [t.typeName, t.className, t.namespace], color: 'var(--accent-yellow)' },
		{ tag: t.tagName, color: 'var(--accent)' },
		{ tag: [t.operator, t.derefOperator, t.arithmeticOperator, t.logicOperator, t.bitwiseOperator, t.compareOperator, t.updateOperator, t.definitionOperator], color: 'var(--text-secondary)' },
		{ tag: [t.punctuation, t.bracket, t.squareBracket, t.paren, t.brace], color: 'var(--text-muted)' },
		{ tag: t.invalid, color: 'var(--accent-red)' },
	]);

	// Called by parent to reset content imperatively (e.g. "clear")
	export function setValue(newVal: string) {
		if (!view) return;
		view.dispatch({
			changes: { from: 0, to: view.state.doc.length, insert: newVal },
		});
	}

	export function setLanguage(lang: string) {
		if (!view) return;
		view.dispatch({
			effects: languageCompartment.reconfigure(languageExtension(lang)),
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
				backgroundColor: 'rgba(242,107,29,0.22) !important',
			},
			'.cm-cursor': { borderLeftColor: 'var(--accent)', borderLeftWidth: '2px' },
			'.cm-matchingBracket': { outline: '1px solid var(--accent)', borderRadius: '2px' },
		});

		const extensions = [
			basicSetup,
			languageCompartment.of(languageExtension(language)),
			theme,
			syntaxHighlighting(syntaxTheme),
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

<div
	bind:this={container}
	class="editor-wrap"
	class:fill={height === '100%'}
	style={height !== '100%' ? `height: ${height}` : undefined}
></div>

<style>
	.editor-wrap {
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-base);
		transition: border-color var(--transition-fast);
	}

	/* height:100% on the CodeMirror root (below) only resolves to something
	   real if THIS wrapper has a definite height too — a plain block element
	   with no height set (the default) computes to its content's height, which
	   for a one-line file is one line tall, not "fill the available space".
	   Growing as a flex child instead gives it a real, definite height to
	   fill, wherever this component sits inside a flex column layout. */
	.editor-wrap.fill {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}

	.editor-wrap:focus-within {
		border-color: var(--accent);
	}
</style>
