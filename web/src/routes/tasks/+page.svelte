<script lang="ts">
  import { tasks as tasksApi } from '$lib/api';
  import type { Task, TaskStatus } from '$lib/types';
  import TaskCard from '$lib/components/TaskCard.svelte';
  import { onMount } from 'svelte';

  let allTasks: Task[] = [];
  let filterStatus: TaskStatus | '' = '';
  let runningTask: number | null = null;
  let runError = '';

  onMount(async () => { allTasks = await tasksApi.list(); });

  $: filtered = filterStatus
    ? allTasks.filter(t => t.status === filterStatus)
    : allTasks;

  const statuses: Array<{ value: TaskStatus | ''; label: string }> = [
    { value: '', label: 'All' },
    { value: 'queued', label: 'Queued' },
    { value: 'pending', label: 'Pending' },
    { value: 'in_progress', label: 'In Progress' },
    { value: 'pr_created', label: 'PR Created' },
    { value: 'needs_human', label: 'Needs Human' },
    { value: 'failed', label: 'Failed' },
    { value: 'skipped', label: 'Skipped' },
  ];

  async function startTask(task: Task) {
    runError = '';
    runningTask = task.id;
    try {
      await tasksApi.run(task.id);
      allTasks = await tasksApi.list();
    } catch (e: any) {
      runError = e.message;
    } finally {
      runningTask = null;
    }
  }
</script>

<div class="header">
  <h1>Tasks</h1>
  <select bind:value={filterStatus}>
    {#each statuses as s}
      <option value={s.value}>{s.label} {s.value ? `(${allTasks.filter(t => t.status === s.value).length})` : `(${allTasks.length})`}</option>
    {/each}
  </select>
</div>

{#if runError}<p class="error">{runError}</p>{/if}

{#if filtered.length === 0}
  <p class="muted">No tasks {filterStatus ? `with status "${filterStatus}"` : 'yet'}.</p>
{:else}
  <div class="list">
    {#each filtered as task}
      <div class="task-row">
        <TaskCard {task} />
        {#if task.status === 'queued' || task.status === 'pending' || task.status === 'failed'}
          <button
            class="start-btn small"
            on:click={() => startTask(task)}
            disabled={runningTask === task.id}
          >
            {runningTask === task.id ? '…' : 'Start'}
          </button>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 20px; }
  h1 { margin: 0; font-size: 20px; }
  select { max-width: 180px; }
  .list { display: flex; flex-direction: column; gap: 8px; }
  .muted { color: var(--color-text-muted); }
  .error { color: var(--color-error); font-size: 12px; margin: 0 0 12px; }
  .task-row { display: flex; align-items: stretch; gap: 8px; }
  .task-row :global(.card) { flex: 1; }
  .start-btn {
    align-self: center;
    padding: 4px 10px;
    font-size: 11px;
    white-space: nowrap;
  }
</style>
