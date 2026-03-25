<!-- web/src/lib/components/AgentStatusCard.svelte -->
<script lang="ts">
  import type { AgentState } from '$lib/types';
  import StatusBadge from './StatusBadge.svelte';

  export let state: AgentState;

  function formatWakeAt(ts: number | null): string {
    if (!ts) return '';
    const date = new Date(ts * 1000);
    const now = Date.now();
    const diff = date.getTime() - now;
    if (diff <= 0) return 'soon';
    const mins = Math.ceil(diff / 60_000);
    if (mins < 60) return `${mins}m`;
    const hrs = Math.floor(mins / 60);
    const rem = mins % 60;
    return rem > 0 ? `${hrs}h ${rem}m` : `${hrs}h`;
  }

  function formatLastRun(ts: number | null): string {
    if (!ts) return 'Never';
    return new Date(ts * 1000).toLocaleString();
  }
</script>

<div class="card">
  <div class="row">
    <span class="label">Agent</span>
    <StatusBadge status={state.status} />
  </div>

  {#if state.status === 'sleeping' && state.wake_at}
    <div class="row">
      <span class="label">Wakes in</span>
      <span class="value">{formatWakeAt(state.wake_at)}</span>
    </div>
    {#if state.usage_note}
      <div class="note">{state.usage_note}</div>
    {/if}
  {/if}

  {#if state.status === 'running' && state.current_task_id}
    <div class="row">
      <span class="label">Working on</span>
      <a href="/tasks/{state.current_task_id}">Task #{state.current_task_id}</a>
    </div>
  {/if}

  <div class="row">
    <span class="label">Last run</span>
    <span class="value muted">{formatLastRun(state.last_run_at)}</span>
  </div>
</div>

<style>
  .card {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .label {
    color: var(--color-text-muted);
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .value { font-size: 13px; }
  .muted { color: var(--color-text-muted); }
  .note {
    font-size: 12px;
    color: var(--color-warning);
    background: rgba(245, 158, 11, 0.1);
    border-radius: 4px;
    padding: 6px 8px;
  }
</style>
