<script lang="ts">
  import { repos as reposApi } from '$lib/api';
  import type { Repo } from '$lib/types';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import { onMount } from 'svelte';

  let repoList: Repo[] = [];
  let addInput = '';   // "owner/name" format
  let addLabel = '';
  let addError = '';
  let deleteTarget: Repo | null = null;

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
      addInput = ''; addLabel = '';
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
</script>

<h1>Repos</h1>

<div class="add-form">
  <h2>Add Repository</h2>
  <div class="form-row">
    <input bind:value={addInput} placeholder="owner/repo" />
    <input bind:value={addLabel} placeholder="Label filter (optional)" style="max-width: 200px" />
    <button class="primary" on:click={addRepo}>Add</button>
  </div>
  {#if addError}<p class="error">{addError}</p>{/if}
</div>

{#if repoList.length === 0}
  <p class="muted">No repos configured yet.</p>
{:else}
  <table>
    <thead>
      <tr><th>Repository</th><th>Label Filter</th><th>Enabled</th><th></th></tr>
    </thead>
    <tbody>
      {#each repoList as repo}
        <tr>
          <td>
            <a href="https://github.com/{repo.owner}/{repo.name}" target="_blank" rel="noopener">
              {repo.owner}/{repo.name}
            </a>
          </td>
          <td>{repo.label_filter ?? '—'}</td>
          <td>
            <label class="toggle">
              <input type="checkbox" checked={repo.enabled} on:change={() => toggleEnabled(repo)} />
              <span class="slider"></span>
            </label>
          </td>
          <td>
            <button class="danger small" on:click={() => deleteTarget = repo}>Delete</button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
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
  h1 { margin: 0 0 24px; font-size: 20px; }
  h2 { margin: 0 0 12px; font-size: 13px; color: var(--color-text-muted); text-transform: uppercase; }
  .add-form {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 24px;
  }
  .form-row { display: flex; gap: 8px; align-items: center; }
  .error { color: var(--color-error); font-size: 12px; margin: 6px 0 0; }
  table { width: 100%; border-collapse: collapse; }
  th, td { text-align: left; padding: 10px 12px; border-bottom: 1px solid var(--color-border); font-size: 13px; }
  th { color: var(--color-text-muted); font-size: 11px; text-transform: uppercase; }
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
