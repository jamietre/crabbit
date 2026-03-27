<!-- web/src/lib/components/StatusBadge.svelte -->
<script lang="ts">
  import type { TaskStatus, AgentStatus } from '$lib/types';

  export let status: TaskStatus | AgentStatus;

  const labels: Record<string, string> = {
    pending: 'Pending',
    in_progress: 'In Progress',
    retrying: 'Retrying',
    pr_created: 'PR Created',
    needs_human: 'Needs Human',
    failed: 'Failed',
    skipped: 'Skipped',
    idle: 'Idle',
    running: 'Running',
    sleeping: 'Sleeping',
  };
</script>

<span class="badge" style="--badge-color: var(--status-{status}, var(--color-text-muted))">
  {labels[status] ?? status}
</span>

<style>
  .badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 9999px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: var(--badge-color);
    border: 1px solid var(--badge-color);
    opacity: 0.9;
  }
</style>
