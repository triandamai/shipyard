<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';

	$effect(() => {
		goto(`/orgs/${$page.params.orgSlug}/settings/general`, { replaceState: true });
	});
</script>
