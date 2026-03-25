<!-- web/src/routes/auth/callback/+page.svelte -->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { githubStatus } from '$lib/stores';
  import { auth } from '$lib/api';
  import { onMount } from 'svelte';

  let status: 'loading' | 'success' | 'error' = 'loading';
  let message = '';

  onMount(async () => {
    try {
      const s = await auth.status();
      githubStatus.set(s);
      if (s.connected) {
        status = 'success';
        setTimeout(() => goto('/auth'), 1500);
      } else {
        status = 'error';
        message = 'GitHub connection did not complete.';
      }
    } catch (e: any) {
      status = 'error';
      message = e.message;
    }
  });
</script>

{#if status === 'loading'}
  <p>Completing GitHub connection…</p>
{:else if status === 'success'}
  <p class="success">✓ GitHub connected! Redirecting…</p>
{:else}
  <p class="error">Connection failed: {message}</p>
  <a href="/auth">Try again</a>
{/if}

<style>
  .success { color: var(--color-success); }
  .error { color: var(--color-error); }
</style>
