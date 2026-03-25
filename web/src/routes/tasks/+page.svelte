<script lang="ts">
  import { tasks as tasksApi } from '$lib/api';
  import type { Task, TaskStatus } from '$lib/types';
  import TaskCard from '$lib/components/TaskCard.svelte';
  import { onMount } from 'svelte';

  let allTasks: Task[] = [];
  let filterStatus: TaskStatus | '' = '';

  onMount(async () => { allTasks = await tasksApi.list(); });

  $: filtered = filterStatus
    ? allTasks.filter(t => t.status === filterStatus)
    : allTasks;

  const statuses: Array<{ value: TaskStatus | ''; label: string }> = [
    { value: '', label: 'All' },
    { value: 'pending', label: 'Pending' },
    { value: 'in_progress', label: 'In Progress' },
    { value: 'pr_created', label: 'PR Created' },
    { value: 'needs_human', label: 'Needs Human' },
    { value: 'failed', label: 'Failed' },
    { value: 'skipped', label: 'Skipped' },
  ];
</script>

<div class="header">
  <h1>Tasks</h1>
  <select bind:value={filterStatus}>
    {#each statuses as s}
      <option value={s.value}>{s.label} {s.value ? `(${allTasks.filter(t => t.status === s.value).length})` : `(${allTasks.length})`}</option>
    {/each}
  </select>
</div>

{#if filtered.length === 0}
  <p class="muted">No tasks {filterStatus ? `with status "${filterStatus}"` : 'yet'}.</p>
{:else}
  <div class="list">
    {#each filtered as task}
      <TaskCard {task} />
    {/each}
  </div>
{/if}

<style>
  .header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 20px; }
  h1 { margin: 0; font-size: 20px; }
  select { max-width: 180px; }
  .list { display: flex; flex-direction: column; gap: 8px; }
  .muted { color: var(--color-text-muted); }
</style>
