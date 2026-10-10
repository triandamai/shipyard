<script lang="ts">
	import DocPage from '$lib/docs/DocPage.svelte';

	let { data } = $props();
</script>

<DocPage doc={data.doc} markdown={data.markdown} />
