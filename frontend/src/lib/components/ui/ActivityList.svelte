<script lang="ts">
	import type { Snippet } from 'svelte';
	import Card from './Card.svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();
</script>

<Card padding="6px 16px">
	{@render children()}
</Card>
