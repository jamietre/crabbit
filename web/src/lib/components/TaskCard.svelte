<!-- web/src/lib/components/TaskCard.svelte -->
<script lang="ts">
  import type { Task } from '$lib/types';
  import StatusBadge from './StatusBadge.svelte';

  export let task: Task;

  function timeAgo(ts: number): string {
    const diff = Date.now() - ts * 1000;
    const mins = Math.floor(diff / 60_000);
    if (mins < 1) return 'just now';
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    return `${Math.floor(hrs / 24)}d ago`;
  }
</script>

<a class="card" href="/tasks/{task.id}">
  <div class="header">
    <span class="title">{task.issue_title}</span>
    <StatusBadge status={task.status} />
  </div>
  <div class="meta">
    <span class="issue-ref">#{task.issue_number}</span>
    {#if task.pr_number}
      <span class="pr-ref">→ PR #{task.pr_number}</span>
    {/if}
    <span class="time">{timeAgo(task.updated_at)}</span>
  </div>
</a>

<style>
  .card {
    display: block;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 12px 16px;
    text-decoration: none;
    color: inherit;
    transition: border-color 0.15s;
  }
  .card:hover { border-color: var(--color-accent); text-decoration: none; }
  .header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 6px;
  }
  .title { font-size: 14px; line-height: 1.4; flex: 1; }
  .meta {
    display: flex;
    gap: 10px;
    font-size: 12px;
    color: var(--color-text-muted);
  }
  .pr-ref { color: var(--color-success); }
</style>
