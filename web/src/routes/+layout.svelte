<!-- web/src/routes/+layout.svelte -->
<script lang="ts">
  import '../app.css';
  import { page } from '$app/stores';
  import { agentState, githubStatus, claudeAuthStatus, startPolling } from '$lib/stores';
  import { agent, auth, claudeAuth } from '$lib/api';
  import { onMount } from 'svelte';

  export let data;

  $: agentState.set(data.agentState);
  $: githubStatus.set(data.githubStatus);
  $: claudeAuthStatus.set(data.claudeAuthStatus);

  onMount(() => {
    const stop = startPolling(async () => {
      const [a, g, c] = await Promise.all([
        agent.getState().catch(() => null),
        auth.status().catch(() => null),
        claudeAuth.status().catch(() => null),
      ]);
      agentState.set(a);
      githubStatus.set(g);
      claudeAuthStatus.set(c);
    });
    return stop;
  });

  const navItems = [
    { href: '/', label: 'Dashboard' },
    { href: '/tasks', label: 'Tasks' },
    { href: '/repos', label: 'Repos' },
    { href: '/prompts', label: 'Prompts' },
    { href: '/settings', label: 'Settings' },
  ];

  $: claudeCheckOk   = $claudeAuthStatus?.check.status === 'ok';
  $: claudeExpired   = $claudeAuthStatus?.check.status === 'expired';
  $: claudeConfigured = $claudeAuthStatus?.configured;
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
        <a class="conn-indicator" href="/auth" title="GitHub connected">
          <span class="dot dot-ok"></span>@{$githubStatus.github_login}
        </a>
      {:else if $githubStatus?.github_login}
        <a class="conn-indicator" href="/auth" title="GitHub token expired — click to reconnect">
          <span class="dot dot-error"></span>@{$githubStatus.github_login}
        </a>
      {:else}
        <a class="conn-indicator warn" href="/auth" title="GitHub not connected">
          <span class="dot dot-warn"></span>GitHub
        </a>
      {/if}
      {#if claudeConfigured}
        <a class="conn-indicator" href="/auth"
          title="Claude auth {$claudeAuthStatus?.check.status ?? 'unknown'}">
          <span class="dot" class:dot-ok={claudeCheckOk} class:dot-error={claudeExpired} class:dot-warn={!claudeCheckOk && !claudeExpired}></span>Claude
        </a>
      {:else}
        <a class="conn-indicator warn" href="/auth" title="Claude credentials not configured">
          <span class="dot dot-warn"></span>Claude
        </a>
      {/if}
      {#if $agentState}
        <span class="usage-pill"
          title="5h Claude Pro usage"
          data-warn={($agentState.usage_pct_5h ?? 0) >= 70}>
          5h {$agentState.usage_pct_5h != null ? Math.round($agentState.usage_pct_5h) + '%' : '—'}
        </span>
        <span class="usage-pill"
          title="7d Claude Pro usage"
          data-warn={($agentState.usage_pct_7d ?? 0) >= 70}>
          7d {$agentState.usage_pct_7d != null ? Math.round($agentState.usage_pct_7d) + '%' : '—'}
        </span>
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

  .conn-indicator {
    display: flex; align-items: center; gap: 5px;
    font-size: 12px; color: var(--color-text-muted);
  }
  .conn-indicator.warn { color: var(--color-warning); }
  .dot {
    width: 7px; height: 7px; border-radius: 50%;
    background: var(--color-text-muted); flex-shrink: 0;
  }
  .dot-ok    { background: var(--color-success); }
  .dot-warn  { background: var(--color-warning); }
  .dot-error { background: var(--color-error); }

  .agent-dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--color-text-muted);
  }
  .agent-dot[data-status="running"] { background: var(--color-info); animation: pulse 1.5s infinite; }
  .agent-dot[data-status="idle"] { background: var(--color-success); }
  .agent-dot[data-status="sleeping"] { background: var(--color-warning); }
  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.4; } }

  .usage-pill {
    font-size: 11px; color: var(--color-text-muted);
    background: var(--color-border); border-radius: 10px;
    padding: 2px 7px;
  }
  .usage-pill[data-warn="true"] { color: var(--color-warning); background: color-mix(in srgb, var(--color-warning) 15%, transparent); }
  main { flex: 1; padding: 24px; max-width: 1100px; margin: 0 auto; width: 100%; }
</style>
