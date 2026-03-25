<!-- web/src/routes/+page.svelte -->
<script lang="ts">
  import { agentState } from '$lib/stores';
  import AgentStatusCard from '$lib/components/AgentStatusCard.svelte';
  import TaskCard from '$lib/components/TaskCard.svelte';
  import { tasks, agent } from '$lib/api';
  import type { Task } from '$lib/types';
  import { onMount } from 'svelte';

  let recentTasks: Task[] = [];
  let stats = { total: 0, pr_created: 0, needs_human: 0, failed: 0 };
  let loading = true;
  let running = false;
  let runError = '';

  onMount(async () => {
    const all = await tasks.list();
    recentTasks = all.slice(0, 10);
    stats = {
      total: all.length,
      pr_created: all.filter(t => t.status === 'pr_created').length,
      needs_human: all.filter(t => t.status === 'needs_human').length,
      failed: all.filter(t => t.status === 'failed').length,
    };
    loading = false;
  });

  async function runNow() {
    runError = '';
    running = true;
    try {
      await agent.run();
    } catch (e: any) {
      runError = e.message;
    } finally {
      running = false;
    }
  }
</script>

<h1>Dashboard</h1>

<div class="grid">
  <div class="section">
    <div class="section-header">
      <h2>Agent</h2>
      <button
        class="primary"
        on:click={runNow}
        disabled={running || $agentState?.status === 'running'}
      >
        {running ? 'Starting…' : 'Run now'}
      </button>
    </div>
    {#if runError}<p class="run-error">{runError}</p>{/if}
    {#if $agentState}
      <AgentStatusCard state={$agentState} />
    {:else}
      <p class="muted">Loading…</p>
    {/if}
  </div>

  <div class="section">
    <h2>Stats</h2>
    <div class="stats-grid">
      <div class="stat"><span class="num">{stats.total}</span><span class="lbl">Total tasks</span></div>
      <div class="stat"><span class="num success">{stats.pr_created}</span><span class="lbl">PRs created</span></div>
      <div class="stat"><span class="num warning">{stats.needs_human}</span><span class="lbl">Needs human</span></div>
      <div class="stat"><span class="num error">{stats.failed}</span><span class="lbl">Failed</span></div>
    </div>
  </div>
</div>

<div class="section" style="margin-top: 24px">
  <div class="section-header">
    <h2>Recent Tasks</h2>
    <a href="/tasks">View all</a>
  </div>
  {#if loading}
    <p class="muted">Loading…</p>
  {:else if recentTasks.length === 0}
    <p class="muted">No tasks yet. Add a repo and wait for the agent to run.</p>
  {:else}
    <div class="task-list">
      {#each recentTasks as task}
        <TaskCard {task} />
      {/each}
    </div>
  {/if}
</div>

<style>
  h1 { margin: 0 0 24px; font-size: 20px; }
  h2 { margin: 0 0 12px; font-size: 14px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-text-muted); }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 20px; }
  @media (max-width: 600px) { .grid { grid-template-columns: 1fr; } }
  .section-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
  .section-header h2 { margin: 0; }
  .stats-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .stat {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 16px;
    display: flex; flex-direction: column; gap: 4px;
  }
  .num { font-size: 28px; font-weight: 700; }
  .num.success { color: var(--color-success); }
  .num.warning { color: var(--color-warning); }
  .num.error { color: var(--color-error); }
  .lbl { font-size: 11px; color: var(--color-text-muted); text-transform: uppercase; }
  .task-list { display: flex; flex-direction: column; gap: 8px; }
  .muted { color: var(--color-text-muted); font-size: 13px; }
  .run-error { color: var(--color-error); font-size: 12px; margin: 4px 0 8px; }
</style>
