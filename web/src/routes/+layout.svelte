<!-- web/src/routes/+layout.svelte -->
<script lang="ts">
  import '../app.css';
  import { page } from '$app/stores';
  import { agentState, githubStatus, startPolling } from '$lib/stores';
  import { agent, auth } from '$lib/api';
  import { onMount } from 'svelte';

  export let data;

  $: agentState.set(data.agentState);
  $: githubStatus.set(data.githubStatus);

  onMount(() => {
    const stop = startPolling(async () => {
      const [a, g] = await Promise.all([agent.getState().catch(() => null), auth.status().catch(() => null)]);
      agentState.set(a);
      githubStatus.set(g);
    });
    return stop;
  });

  const navItems = [
    { href: '/', label: 'Dashboard' },
    { href: '/tasks', label: 'Tasks' },
    { href: '/repos', label: 'Repos' },
    { href: '/settings', label: 'Settings' },
  ];
</script>

<div class="shell">
  <nav>
    <a class="brand" href="/">🐚 crabbit</a>
    <div class="nav-links">
      {#each navItems as item}
        <a class:active={$page.url.pathname === item.href} href={item.href}>{item.label}</a>
      {/each}
    </div>
    <div class="nav-end">
      {#if $githubStatus?.connected}
        <a class="gh-user" href="/auth">@{$githubStatus.github_login}</a>
      {:else if $githubStatus?.github_login}
        <a class="gh-expired" href="/auth" title="GitHub token expired — click to reconnect">@{$githubStatus.github_login} (reconnect)</a>
      {:else}
        <a class="gh-connect" href="/auth">Connect GitHub</a>
      {/if}
      {#if $agentState}
        <span class="agent-dot" data-status={$agentState.status} title="Agent {$agentState.status}"></span>
      {/if}
    </div>
  </nav>

  <main>
    <slot />
  </main>
</div>

<style>
  .shell { display: flex; flex-direction: column; min-height: 100vh; }
  nav {
    display: flex;
    align-items: center;
    gap: 24px;
    padding: 0 24px;
    height: 52px;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
    position: sticky; top: 0; z-index: 10;
  }
  .brand { font-weight: 700; font-size: 16px; color: var(--color-text); }
  .nav-links { display: flex; gap: 4px; }
  .nav-links a {
    padding: 5px 10px; border-radius: 6px;
    color: var(--color-text-muted); font-size: 13px;
  }
  .nav-links a.active, .nav-links a:hover {
    color: var(--color-text); background: var(--color-border); text-decoration: none;
  }
  .nav-end { margin-left: auto; display: flex; align-items: center; gap: 12px; }
  .gh-user, .gh-connect, .gh-expired { font-size: 12px; }
  .gh-connect { color: var(--color-warning); }
  .gh-expired { color: var(--color-error); }
  .agent-dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--color-text-muted);
  }
  .agent-dot[data-status="running"] { background: var(--color-info); animation: pulse 1.5s infinite; }
  .agent-dot[data-status="idle"] { background: var(--color-success); }
  .agent-dot[data-status="sleeping"] { background: var(--color-warning); }
  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.4; } }
  main { flex: 1; padding: 24px; max-width: 1100px; margin: 0 auto; width: 100%; }
</style>
