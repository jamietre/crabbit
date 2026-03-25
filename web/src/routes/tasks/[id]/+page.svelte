<script lang="ts">
  import { page } from '$app/stores';
  import { tasks as tasksApi } from '$lib/api';
  import type { TaskWithEvents, TaskEvent } from '$lib/types';
  import StatusBadge from '$lib/components/StatusBadge.svelte';
  import ScreenshotViewer from '$lib/components/ScreenshotViewer.svelte';
  import { onMount } from 'svelte';

  let task: TaskWithEvents | null = null;
  let error = '';

  onMount(async () => {
    try {
      task = await tasksApi.get(Number($page.params.id));
    } catch (e: any) {
      error = e.message;
    }
  });

  async function retryTask() {
    if (!task) return;
    await tasksApi.updateStatus(task.id, 'pending');
    task = { ...task, status: 'pending' };
  }

  function formatTime(ts: number) {
    return new Date(ts * 1000).toLocaleString();
  }

  function isScreenshot(event: TaskEvent) {
    return event.event_type === 'browser_screenshot';
  }

  function eventLabel(type: string): string {
    const labels: Record<string, string> = {
      claude_output: 'Claude output',
      status_change: 'Status change',
      comment_posted: 'Comment posted',
      pr_created: 'PR created',
      browser_screenshot: 'Screenshot',
      error: 'Error',
    };
    return labels[type] ?? type;
  }
</script>

{#if error}
  <p class="error">{error}</p>
{:else if !task}
  <p class="muted">Loading…</p>
{:else}
  <div class="breadcrumb">
    <a href="/tasks">Tasks</a> / #{task.issue_number}
  </div>

  <div class="task-header">
    <h1>{task.issue_title}</h1>
    <StatusBadge status={task.status} />
  </div>

  <div class="meta-row">
    <a href={task.issue_url} target="_blank" rel="noopener">View issue ↗</a>
    {#if task.pr_url}
      <a href={task.pr_url} target="_blank" rel="noopener" class="pr-link">PR #{task.pr_number} ↗</a>
    {/if}
    {#if task.status === 'failed' || task.status === 'needs_human'}
      <button on:click={retryTask}>Retry</button>
    {/if}
  </div>

  {#if task.error_message}
    <div class="error-box">{task.error_message}</div>
  {/if}

  <details class="issue-body">
    <summary>Issue body</summary>
    <pre>{task.issue_body}</pre>
  </details>

  <h2>Timeline</h2>
  {#if task.events.length === 0}
    <p class="muted">No events yet.</p>
  {:else}
    <div class="timeline">
      {#each task.events as event}
        <div class="event" data-type={event.event_type}>
          <div class="event-header">
            <span class="event-type">{eventLabel(event.event_type)}</span>
            <span class="event-time">{formatTime(event.created_at)}</span>
          </div>
          {#if isScreenshot(event)}
            <ScreenshotViewer
              base64={event.payload.base64 as string}
              filename={event.payload.filename as string ?? 'screenshot.png'}
            />
          {:else if event.event_type === 'claude_output'}
            <details>
              <summary>Show output</summary>
              <pre class="output">{typeof event.payload.text === 'string' ? event.payload.text : JSON.stringify(event.payload, null, 2)}</pre>
            </details>
          {:else}
            <pre class="payload">{JSON.stringify(event.payload, null, 2)}</pre>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .breadcrumb { font-size: 12px; color: var(--color-text-muted); margin-bottom: 16px; }
  .task-header { display: flex; align-items: flex-start; gap: 12px; margin-bottom: 12px; }
  h1 { margin: 0; font-size: 20px; flex: 1; }
  h2 { margin: 24px 0 12px; font-size: 14px; text-transform: uppercase; color: var(--color-text-muted); letter-spacing: 0.05em; }
  .meta-row { display: flex; gap: 12px; align-items: center; margin-bottom: 16px; font-size: 13px; }
  .pr-link { color: var(--color-success); }
  .error-box {
    background: rgba(239,68,68,0.1); border: 1px solid var(--color-error);
    border-radius: 6px; padding: 10px 14px; font-size: 13px;
    color: var(--color-error); margin-bottom: 16px;
  }
  .issue-body { margin-bottom: 24px; }
  .issue-body summary { cursor: pointer; font-size: 13px; color: var(--color-text-muted); }
  pre { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 6px; padding: 12px; font-size: 12px; overflow-x: auto; white-space: pre-wrap; }
  .timeline { display: flex; flex-direction: column; gap: 12px; }
  .event {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px; overflow: hidden;
  }
  .event[data-type="error"] { border-color: var(--color-error); }
  .event[data-type="pr_created"] { border-color: var(--color-success); }
  .event-header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 8px 12px; border-bottom: 1px solid var(--color-border);
    background: rgba(255,255,255,0.02);
  }
  .event-type { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; }
  .event-time { font-size: 11px; color: var(--color-text-muted); }
  .output, .payload { margin: 0; border: none; border-radius: 0; }
  .muted { color: var(--color-text-muted); }
  .error { color: var(--color-error); }
</style>
