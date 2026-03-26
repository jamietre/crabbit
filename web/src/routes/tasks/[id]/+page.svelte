<script lang="ts">
  import { page } from '$app/stores';
  import { tasks as tasksApi } from '$lib/api';
  import type { TaskWithEvents, TaskEvent } from '$lib/types';
  import StatusBadge from '$lib/components/StatusBadge.svelte';
  import ScreenshotViewer from '$lib/components/ScreenshotViewer.svelte';
  import { onMount, onDestroy } from 'svelte';

  let task: TaskWithEvents | null = null;
  let error = '';
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  const TERMINAL = new Set(['pr_created', 'needs_human', 'failed', 'skipped']);

  onMount(async () => {
    try {
      task = await tasksApi.get(Number($page.params.id));
      if (task && !TERMINAL.has(task.status)) startPolling();
    } catch (e: any) {
      error = e.message;
    }
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
  });

  function startPolling() {
    pollInterval = setInterval(async () => {
      try {
        const updated = await tasksApi.get(Number($page.params.id));
        task = updated;
        if (TERMINAL.has(updated.status)) {
          clearInterval(pollInterval!);
          pollInterval = null;
        }
      } catch {}
    }, 2000);
  }

  async function retryTask() {
    if (!task) return;
    await tasksApi.updateStatus(task.id, 'pending');
    task = { ...task, status: 'pending' };
  }

  function formatTime(ts: number) {
    return new Date(ts * 1000).toLocaleString();
  }

  type LogEntry = {
    ts: number;
    label: string;
    preview: string;
    detail?: string;
    detailKind?: 'text' | 'image' | 'json';
    imageBase64?: string;
    imageFilename?: string;
  };

  function toLogEntries(events: TaskEvent[]): LogEntry[] {
    const entries: LogEntry[] = [];

    for (const event of events) {
      const ts = event.created_at;

      if (event.event_type === 'orchestrator_log') {
        entries.push({ ts, label: 'Log', preview: event.payload.message as string });

      } else if (event.event_type === 'claude_output') {
        const line = event.payload.line as any;
        if (!line) continue;

        if (line.type === 'assistant') {
          for (const block of line.message?.content ?? []) {
            if (block.type === 'thinking' && block.thinking?.trim()) {
              const text: string = block.thinking.trim();
              const firstLine = text.split('\n')[0].slice(0, 120);
              entries.push({
                ts, label: 'Thinking',
                preview: firstLine + (text.length > firstLine.length ? '…' : ''),
                detail: text, detailKind: 'text',
              });
            } else if (block.type === 'text' && block.text?.trim()) {
              const text: string = block.text.trim();
              const firstLine = text.split('\n')[0].slice(0, 120);
              const hasMore = text.length > firstLine.length;
              entries.push({
                ts, label: 'Claude',
                preview: firstLine + (hasMore ? '…' : ''),
                detail: hasMore ? text : undefined,
                detailKind: 'text',
              });
            }
            // tool_use, system, tool results — skip
          }
        } else if (line.type === 'result') {
          const cost: number = line.cost_usd ?? 0;
          const ok = line.subtype === 'success';
          entries.push({ ts, label: 'Done', preview: `${ok ? '✓' : '✗'} Session complete · $${cost.toFixed(4)}` });
        }

      } else if (event.event_type === 'browser_screenshot') {
        entries.push({
          ts, label: 'Screenshot',
          preview: event.payload.filename as string ?? 'screenshot.png',
          detailKind: 'image',
          imageBase64: event.payload.base64 as string,
          imageFilename: event.payload.filename as string ?? 'screenshot.png',
        });

      } else {
        const json = JSON.stringify(event.payload, null, 2);
        const preview = json.slice(0, 100).replace(/\n/g, ' ');
        entries.push({
          ts,
          label: event.event_type.replace(/_/g, ' '),
          preview: preview + (json.length > 100 ? '…' : ''),
          detail: json.length > 100 ? json : undefined,
          detailKind: 'json',
        });
      }
    }

    return entries;
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
    {:else if task.status === 'in_progress'}
      <button on:click={retryTask}>Reset to pending</button>
    {/if}
  </div>

  {#if task.error_message}
    <div class="error-box">{task.error_message}</div>
  {/if}

  <details class="issue-body">
    <summary>Issue body</summary>
    <pre>{task.issue_body}</pre>
  </details>

  <h2>Log</h2>
  {#if task.events.length === 0}
    <p class="muted">{task.status === 'in_progress' ? 'Waiting for agent…' : 'No events.'}</p>
  {:else}
    <div class="log">
      {#each toLogEntries(task.events) as entry}
        {#if entry.detail || entry.detailKind === 'image'}
          <details class="log-entry">
            <summary>
              <span class="ts">{formatTime(entry.ts)}</span>
              <span class="tag">[{entry.label}]</span>
              <span class="preview">{entry.preview}</span>
            </summary>
            <div class="log-body">
              {#if entry.detailKind === 'image'}
                <ScreenshotViewer base64={entry.imageBase64!} filename={entry.imageFilename!} />
              {:else}
                <pre class="log-detail">{entry.detail}</pre>
              {/if}
            </div>
          </details>
        {:else}
          <div class="log-entry plain">
            <span class="ts">{formatTime(entry.ts)}</span>
            <span class="tag">[{entry.label}]</span>
            <span class="preview">{entry.preview}</span>
          </div>
        {/if}
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

  /* ── Log ── */
  .log { font-family: monospace; font-size: 12px; display: flex; flex-direction: column; gap: 1px; }

  .log-entry { display: block; }

  /* expandable row */
  details.log-entry > summary {
    display: flex; align-items: baseline; gap: 8px;
    padding: 2px 0; cursor: pointer;
    list-style: none;
  }
  details.log-entry > summary::-webkit-details-marker { display: none; }
  details.log-entry > summary::before {
    content: '▶';
    font-size: 9px;
    flex-shrink: 0;
    color: var(--color-text-muted);
    display: inline-block;
    transition: transform 0.15s;
    width: 12px;
  }
  details[open].log-entry > summary::before {
    transform: rotate(90deg);
  }

  /* plain (non-expandable) row — indent to match triangle width */
  .log-entry.plain {
    display: flex; align-items: baseline; gap: 8px;
    padding: 2px 0;
    padding-left: 20px; /* 12px triangle + 8px gap */
  }

  .ts { color: var(--color-text-muted); flex-shrink: 0; }
  .tag { color: var(--color-text-muted); flex-shrink: 0; }
  .preview { color: var(--color-text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .log-body { padding: 6px 0 6px 20px; }
  .log-detail { margin: 0; }

  .muted { color: var(--color-text-muted); }
  .error { color: var(--color-error); }
</style>
