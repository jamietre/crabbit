<script lang="ts">
  import { repos as reposApi, sync as syncApi } from '$lib/api';
  import type { Repo } from '$lib/types';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import { onMount } from 'svelte';

  let repoList: Repo[] = [];
  let addInput = '';   // "owner/name" format
  let addError = '';
  let deleteTarget: Repo | null = null;
  let syncingAll = false;
  let syncResult = '';

  // Per-repo editing state
  let editing: Record<number, {
    labelsRequire: string;
    labelsIgnore: string;
    labelsPrioritize: string;
    completionPrompt: string;
    saving: boolean;
  }> = {};

  onMount(async () => { repoList = await reposApi.list(); });

  async function addRepo() {
    addError = '';
    const parts = addInput.trim().split('/');
    if (parts.length !== 2 || !parts[0] || !parts[1]) {
      addError = 'Enter as owner/repo, e.g. "acme/api"';
      return;
    }
    try {
      const r = await reposApi.create(parts[0], parts[1]);
      repoList = [...repoList, r];
      addInput = '';
    } catch (e: any) {
      addError = e.message;
    }
  }

  async function toggleEnabled(repo: Repo) {
    const updated = await reposApi.update(repo.id, { enabled: !repo.enabled });
    repoList = repoList.map(r => r.id === repo.id ? updated : r);
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    await reposApi.delete(deleteTarget.id);
    repoList = repoList.filter(r => r.id !== deleteTarget!.id);
    deleteTarget = null;
  }

  function startEditing(repo: Repo) {
    editing = {
      ...editing,
      [repo.id]: {
        labelsRequire: repo.labels_require.join(', '),
        labelsIgnore: repo.labels_ignore.join(', '),
        labelsPrioritize: repo.labels_prioritize.join(', '),
        completionPrompt: repo.completion_prompt ?? '',
        saving: false,
      },
    };
  }

  function cancelEditing(id: number) {
    const { [id]: _, ...rest } = editing;
    editing = rest;
  }

  function parseTags(s: string): string[] {
    return s.split(',').map(t => t.trim()).filter(Boolean);
  }

  async function saveLabels(repo: Repo) {
    const e = editing[repo.id];
    if (!e) return;
    editing = { ...editing, [repo.id]: { ...e, saving: true } };
    const updated = await reposApi.update(repo.id, {
      labels_require: parseTags(e.labelsRequire),
      labels_ignore: parseTags(e.labelsIgnore),
      labels_prioritize: parseTags(e.labelsPrioritize),
      completion_prompt: e.completionPrompt.trim() || null,
    });
    repoList = repoList.map(r => r.id === repo.id ? updated : r);
    cancelEditing(repo.id);
  }

  async function syncAll() {
    syncingAll = true;
    syncResult = '';
    try {
      const r = await syncApi.all();
      syncResult = `Sync complete: ${r.created} created, ${r.updated} updated, ${r.closed} closed`;
    } catch (e: any) {
      syncResult = `Sync failed: ${e.message}`;
    } finally {
      syncingAll = false;
      repoList = await reposApi.list();
    }
  }

  async function syncOne(repo: Repo) {
    try {
      const r = await syncApi.repo(repo.id);
      syncResult = `Synced ${repo.owner}/${repo.name}: ${r.created} created, ${r.updated} updated, ${r.closed} closed`;
    } catch (e: any) {
      syncResult = `Sync failed: ${e.message}`;
    }
  }
</script>

<div class="page-header">
  <h1>Repos</h1>
  <div class="header-actions">
    {#if syncResult}<span class="sync-result">{syncResult}</span>{/if}
    <button class="primary" on:click={syncAll} disabled={syncingAll}>
      {syncingAll ? 'Syncing…' : 'Sync All'}
    </button>
  </div>
</div>

<div class="add-form">
  <h2>Add Repository</h2>
  <div class="form-row">
    <input bind:value={addInput} placeholder="owner/repo" />
    <button class="primary" on:click={addRepo}>Add</button>
  </div>
  {#if addError}<p class="error">{addError}</p>{/if}
</div>

{#if repoList.length === 0}
  <p class="muted">No repos configured yet.</p>
{:else}
  <div class="repo-list">
    {#each repoList as repo}
      <div class="repo-card">
        <div class="repo-row">
          <div class="repo-name">
            <a href="https://github.com/{repo.owner}/{repo.name}" target="_blank" rel="noopener">
              {repo.owner}/{repo.name}
            </a>
          </div>
          <div class="repo-actions">
            <label class="toggle">
              <input type="checkbox" checked={repo.enabled} on:change={() => toggleEnabled(repo)} />
              <span class="slider"></span>
            </label>
            <button class="small" on:click={() => syncOne(repo)}>Sync</button>
            {#if !editing[repo.id]}
              <button class="small" on:click={() => startEditing(repo)}>Edit Labels</button>
            {/if}
            <button class="danger small" on:click={() => deleteTarget = repo}>Delete</button>
          </div>
        </div>

        {#if !editing[repo.id]}
          <div class="label-summary">
            {#if repo.labels_require.length}
              <span class="label-group"><span class="label-key">require:</span> {repo.labels_require.join(', ')}</span>
            {/if}
            {#if repo.labels_ignore.length}
              <span class="label-group"><span class="label-key">ignore:</span> {repo.labels_ignore.join(', ')}</span>
            {/if}
            {#if repo.labels_prioritize.length}
              <span class="label-group"><span class="label-key">prioritize:</span> {repo.labels_prioritize.join(', ')}</span>
            {/if}
            {#if !repo.labels_require.length && !repo.labels_ignore.length && !repo.labels_prioritize.length}
              <span class="muted">No label filters</span>
            {/if}
          </div>
        {:else}
          {@const e = editing[repo.id]}
          <div class="edit-form">
            <div class="field-row">
              <label for="labels-require-{repo.id}">Require labels <span class="hint">(comma-separated)</span></label>
              <input id="labels-require-{repo.id}" bind:value={e.labelsRequire} placeholder="e.g. crabbit" />
            </div>
            <div class="field-row">
              <label for="labels-ignore-{repo.id}">Ignore labels</label>
              <input id="labels-ignore-{repo.id}" bind:value={e.labelsIgnore} placeholder="e.g. human, wip" />
            </div>
            <div class="field-row">
              <label for="labels-prioritize-{repo.id}">Prioritize labels</label>
              <input id="labels-prioritize-{repo.id}" bind:value={e.labelsPrioritize} placeholder="e.g. urgent" />
            </div>
            <div class="field-row">
              <label for="completion-prompt-{repo.id}">Completion prompt <span class="hint">(appended to system prompt)</span></label>
              <textarea id="completion-prompt-{repo.id}" bind:value={e.completionPrompt} rows={3} placeholder="Leave blank to use default (create PR + comment on issue)"></textarea>
            </div>
            <div class="edit-actions">
              <button class="primary small" on:click={() => saveLabels(repo)} disabled={e.saving}>
                {e.saving ? 'Saving…' : 'Save'}
              </button>
              <button class="small" on:click={() => cancelEditing(repo.id)}>Cancel</button>
            </div>
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<ConfirmDialog
  open={!!deleteTarget}
  title="Delete repo?"
  message="This will also delete all tasks for {deleteTarget?.owner}/{deleteTarget?.name}."
  confirmLabel="Delete"
  onConfirm={confirmDelete}
  onCancel={() => deleteTarget = null}
/>

<style>
  .page-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 24px; }
  h1 { margin: 0; font-size: 20px; }
  h2 { margin: 0 0 12px; font-size: 13px; color: var(--color-text-muted); text-transform: uppercase; }
  .header-actions { display: flex; align-items: center; gap: 12px; }
  .sync-result { font-size: 12px; color: var(--color-text-muted); }
  .add-form {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 24px;
  }
  .form-row { display: flex; gap: 8px; align-items: center; }
  .error { color: var(--color-error); font-size: 12px; margin: 6px 0 0; }
  .repo-list { display: flex; flex-direction: column; gap: 8px; }
  .repo-card {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 12px 16px;
  }
  .repo-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .repo-name { font-size: 14px; }
  .repo-name a { color: var(--color-text); text-decoration: none; }
  .repo-name a:hover { color: var(--color-accent); }
  .repo-actions { display: flex; align-items: center; gap: 8px; }
  .label-summary { margin-top: 8px; display: flex; flex-wrap: wrap; gap: 12px; font-size: 12px; color: var(--color-text-muted); }
  .label-group { display: flex; gap: 4px; }
  .label-key { color: var(--color-accent); font-weight: 600; }
  .edit-form { margin-top: 12px; display: flex; flex-direction: column; gap: 10px; }
  .field-row { display: flex; flex-direction: column; gap: 4px; }
  .field-row label { font-size: 12px; color: var(--color-text-muted); }
  .hint { font-weight: 400; opacity: 0.7; }
  .field-row input, .field-row textarea { font-size: 13px; }
  textarea { resize: vertical; }
  .edit-actions { display: flex; gap: 8px; }
  button.small { padding: 3px 8px; font-size: 11px; }
  .muted { color: var(--color-text-muted); }
  /* Toggle switch */
  .toggle { position: relative; display: inline-block; width: 36px; height: 20px; }
  .toggle input { opacity: 0; width: 0; height: 0; }
  .slider {
    position: absolute; cursor: pointer; inset: 0;
    background: var(--color-border); border-radius: 20px; transition: 0.2s;
  }
  .slider::before {
    content: ''; position: absolute;
    width: 14px; height: 14px; left: 3px; bottom: 3px;
    background: white; border-radius: 50%; transition: 0.2s;
  }
  input:checked + .slider { background: var(--color-accent); }
  input:checked + .slider::before { transform: translateX(16px); }
</style>
