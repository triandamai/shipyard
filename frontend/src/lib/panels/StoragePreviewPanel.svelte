<script lang="ts">
	import { onMount } from 'svelte';
	import { Card, Button } from '$lib/components/ui';
	import { File, Download, Copy, Check } from '@lucide/svelte';

	interface Props {
		fileKey: string;
	}

	let { fileKey }: Props = $props();

	// Same render-mode decision logic as the pre-migration page: an image
	// extension renders inline via <img>; everything else is fetched as text
	// and shown in a <pre> unless the fetch leaves previewContent null with no
	// error, in which case it falls back to the binary/download view. Preserved
	// exactly, unchanged, from the original openPreview().
	let previewLoading = $state(false);
	let previewContent = $state<string | null>(null);
	let previewError = $state('');
	let previewIsImage = $derived(/\.(png|jpe?g|gif|svg|webp|ico)$/i.test(fileKey));
	let copied = $state(false);

	async function loadPreview() {
		previewError = '';
		previewContent = null;
		copied = false;

		if (previewIsImage) {
			previewContent = `/api/admin/storage/preview?key=${encodeURIComponent(fileKey)}`;
			return;
		}

		previewLoading = true;
		try {
			const token = localStorage.getItem('shipyard_token');
			const response = await fetch(`/api/admin/storage/preview?key=${encodeURIComponent(fileKey)}`, {
				headers: token ? { Authorization: `Bearer ${token}` } : {}
			});

			if (!response.ok) {
				throw new Error(`HTTP error ${response.status}`);
			}

			const text = await response.text();
			previewContent = text;
		} catch (e: any) {
			previewError = e.message ?? 'Failed to load preview';
		} finally {
			previewLoading = false;
		}
	}

	function copyToClipboard() {
		if (!previewContent) return;
		navigator.clipboard.writeText(previewContent);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	onMount(loadPreview);
</script>

<div class="preview-wrap">
	<div class="preview-toolbar">
		<span class="preview-filename mono trunc" title={fileKey}>{fileKey.split('/').pop()}</span>
		<div class="preview-actions">
			{#if previewContent && !previewIsImage}
				<Button variant="secondary" size="sm" onclick={copyToClipboard}>
					{#if copied}
						<Check size={13} style="color:var(--accent-green)" />
						Copied
					{:else}
						<Copy size={13} />
						Copy
					{/if}
				</Button>
			{/if}
			<a
				class="btn btn-secondary"
				href={`/api/admin/storage/preview?key=${encodeURIComponent(fileKey)}`}
				download={fileKey.split('/').pop()}
				target="_blank"
			>
				<Download size={13} />
				Download
			</a>
		</div>
	</div>

	<div class="preview-body">
		{#if previewLoading}
			<div class="preview-loading">
				<div class="spinner"></div>
				<span>Loading file content…</span>
			</div>
		{:else if previewError}
			<Card padding="24px">
				<div class="preview-error">
					<h3>Failed to render preview</h3>
					<p>{previewError}</p>
				</div>
			</Card>
		{:else if previewIsImage}
			<Card padding="16px">
				<div class="preview-image-container">
					<!-- svelte-ignore a11y_missing_attribute -->
					<img src={previewContent} class="preview-image" />
				</div>
			</Card>
		{:else if previewContent !== null}
			<Card padding="0">
				<pre class="preview-code mono">{previewContent}</pre>
			</Card>
		{:else}
			<Card padding="24px">
				<div class="preview-binary">
					<File size={48} style="color: var(--text-dim); margin-bottom: 12px;" />
					<h3>Binary File</h3>
					<p>Previews are not supported for this file type.</p>
					<a
						class="btn btn-primary"
						href={`/api/admin/storage/preview?key=${encodeURIComponent(fileKey)}`}
						download={fileKey.split('/').pop()}
						target="_blank"
					>
						<Download size={13} style="margin-right: 6px;" />
						Download File
					</a>
				</div>
			</Card>
		{/if}
	</div>
</div>

<style>
	.preview-wrap { padding: 16px; height: 100%; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; box-sizing: border-box; }

	.preview-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
	.preview-filename { font-size: 13px; font-weight: 700; color: var(--text-primary); min-width: 0; }
	.preview-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }

	.preview-body { flex: 1; min-height: 0; display: flex; flex-direction: column; }

	.preview-loading { display: flex; flex-direction: column; align-items: center; justify-content: center; flex: 1; gap: 12px; color: var(--text-dim); font-size: 13px; padding: 40px 0; }
	.spinner { width: 24px; height: 24px; border: 2.5px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.preview-error { text-align: center; }
	.preview-error h3 { font-size: 14px; color: var(--accent-red); margin: 0 0 8px; }
	.preview-error p { font-size: 12.5px; color: var(--text-muted); margin: 0; line-height: 1.5; }

	.preview-image-container { display: flex; align-items: center; justify-content: center; background: var(--bg-base); border-radius: var(--radius-sm); padding: 12px; overflow: auto; }
	.preview-image { max-width: 100%; max-height: 420px; object-fit: contain; border-radius: var(--radius-sm); }

	.preview-code { margin: 0; padding: 16px; font-size: 12px; line-height: 1.6; color: var(--text-secondary); white-space: pre; overflow: auto; }

	.preview-binary { display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; }
	.preview-binary h3 { font-size: 14px; color: var(--text-primary); margin: 0 0 6px; }
	.preview-binary p { font-size: 12.5px; color: var(--text-muted); margin: 0 0 16px; line-height: 1.5; }

	.mono { font-family: var(--font-mono); }
	.trunc { text-overflow: ellipsis; white-space: nowrap; overflow: hidden; }
</style>
