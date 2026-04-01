<script lang="ts">
  import { prompts as promptsApi } from '$lib/api';
  import type { Prompt } from '$lib/types';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import { onMount } from 'svelte';

  const CATEGORIES = ['triage', 'plan', 'code'];

  let promptList: Prompt[] = [];
  let loading = true;
  let error = '';

  // Edit/create modal state
  let editing: Prompt | null = null;
  let isNew = false;
  let form = { category: 'triage', label: '', name: '', content: '' };
  let saving = false;
  let saveError = '';

  // Delete confirm
  let deleteTarget: Prompt | null = null;

  onMount(async () => {
    try {
      promptList = await promptsApi.list();
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  });

  function openCreate() {
    isNew = true;
    editing = null;
    form = { category: 'triage', label: '', name: '', content: '' };
    saveError = '';
  }

  function openEdit(p: Prompt) {
    isNew = false;
    editing = p;
    form = { category: p.category, label: p.label, name: p.name, content: p.content };
    saveError = '';
  }

  function closeModal() {
    editing = null;
    isNew = false;
    saveError = '';
  }

  async function savePrompt() {
    if (!form.name.trim() || !form.content.trim()) {
      saveError = 'Name and content are required.';
      return;
    }
    saving = true;
    saveError = '';
    try {
      if (isNew) {
        const created = await promptsApi.create(form);
        promptList = [...promptList, created];
      } else if (editing) {
        const updated = await promptsApi.update(editing.id, form);
        promptList = promptList.map(p => p.id === editing!.id ? updated : p);
      }
      closeModal();
    } catch (e: any) {
      saveError = e.message;
    } finally {
      saving = false;
    }
  }

  async function toggleEnabled(p: Prompt) {
    try {
      const updated = await promptsApi.update(p.id, { enabled: !p.enabled });
      promptList = promptList.map(x => x.id === p.id ? updated : x);
    } catch (e: any) {
      error = e.message;
    }
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    try {
      await promptsApi.delete(deleteTarget.id);
      promptList = promptList.filter(p => p.id !== deleteTarget!.id);
    } catch (e: any) {
      error = e.message;
    } finally {
      deleteTarget = null;
    }
  }

  // Group prompts by category for display
  $: grouped = CATEGORIES.map(cat => ({
    category: cat,
    items: promptList.filter(p => p.category === cat),
  })).concat(
    [...new Set(promptList.map(p => p.category).filter(c => !CATEGORIES.includes(c)))]
      .map(cat => ({ category: cat, items: promptList.filter(p => p.category === cat) }))
  );
</script>

<div class="header-row">
  <h1>Prompts</h1>
  <button class="primary" on:click={openCreate}>+ New Prompt</button>
</div>

{#if loading}
  <p class="muted">Loading…</p>
{:else if error}
  <p class="error">{error}</p>
{:else if promptList.length === 0}
  <p class="muted">No prompts configured yet. Create one to get started.</p>
{:else}
  {#each grouped as group}
    {#if group.items.length > 0}
      <div class="group">
        <h2>{group.category}</h2>
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Label</th>
              <th>Enabled</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {#each group.items as p}
              <tr class:disabled={!p.enabled}>
                <td>
                  <button class="name-btn" on:click={() => openEdit(p)}>{p.name}</button>
                </td>
                <td class="label-cell">{p.label || '—'}</td>
                <td>
                  <label class="toggle">
                    <input type="checkbox" checked={p.enabled} on:change={() => toggleEnabled(p)} />
                    <span class="slider"></span>
                  </label>
                </td>
                <td>
                  <button class="danger small" on:click={() => deleteTarget = p}>Delete</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/each}
{/if}

{#if isNew || editing !== null}
  <div class="modal-overlay" on:click|self={closeModal} role="dialog" aria-modal="true">
    <div class="modal">
      <div class="modal-header">
        <h3>{isNew ? 'New Prompt' : 'Edit Prompt'}</h3>
        <button class="close-btn" on:click={closeModal}>×</button>
      </div>

      <div class="modal-body">
        <div class="field">
          <label for="pm-category">Category</label>
          <select id="pm-category" bind:value={form.category}>
            {#each CATEGORIES as cat}<option value={cat}>{cat}</option>{/each}
          </select>
        </div>

        <div class="field">
          <label for="pm-label">Label <span class="optional">(optional)</span></label>
          <input id="pm-label" type="text" bind:value={form.label} placeholder="e.g. rust, python, frontend" />
        </div>

        <div class="field">
          <label for="pm-name">Name</label>
          <input id="pm-name" type="text" bind:value={form.name} placeholder="Descriptive name for this prompt" />
        </div>

        <div class="field">
          <label for="pm-content">Content</label>
          <textarea id="pm-content" rows="10" bind:value={form.content} placeholder="Prompt content…"></textarea>
        </div>

        {#if saveError}<p class="error">{saveError}</p>{/if}
      </div>

      <div class="modal-footer">
        <button on:click={closeModal}>Cancel</button>
        <button class="primary" on:click={savePrompt} disabled={saving}>
          {saving ? 'Saving…' : 'Save'}
        </button>
      </div>
    </div>
  </div>
{/if}

<ConfirmDialog
  open={!!deleteTarget}
  title="Delete prompt?"
  message="Delete &quot;{deleteTarget?.name}&quot;? This cannot be undone."
  confirmLabel="Delete"
  onConfirm={confirmDelete}
  onCancel={() => deleteTarget = null}
/>

<style>
  .header-row { display: flex; align-items: center; justify-content: space-between; margin-bottom: 24px; }
  h1 { margin: 0; font-size: 20px; }
  h2 { margin: 0 0 8px; font-size: 12px; color: var(--color-text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .group { margin-bottom: 32px; }
  table { width: 100%; border-collapse: collapse; }
  th, td { text-align: left; padding: 10px 12px; border-bottom: 1px solid var(--color-border); font-size: 13px; }
  th { color: var(--color-text-muted); font-size: 11px; text-transform: uppercase; }
  tr.disabled td { opacity: 0.5; }
  .name-btn {
    background: none; border: none; cursor: pointer; font-size: 13px;
    color: var(--color-accent); padding: 0; text-align: left;
  }
  .name-btn:hover { text-decoration: underline; }
  .label-cell { color: var(--color-text-muted); }
  button.small { padding: 3px 8px; font-size: 11px; }
  .muted { color: var(--color-text-muted); }
  .error { color: var(--color-error); font-size: 13px; }

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

  /* Modal */
  .modal-overlay {
    position: fixed; inset: 0; background: rgba(0,0,0,0.5);
    display: flex; align-items: center; justify-content: center; z-index: 100;
  }
  .modal {
    background: var(--color-bg); border: 1px solid var(--color-border);
    border-radius: 10px; width: 600px; max-width: calc(100vw - 32px);
    max-height: calc(100vh - 64px); display: flex; flex-direction: column;
  }
  .modal-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 16px 20px; border-bottom: 1px solid var(--color-border);
  }
  .modal-header h3 { margin: 0; font-size: 15px; }
  .close-btn { background: none; border: none; cursor: pointer; font-size: 20px; color: var(--color-text-muted); padding: 0; line-height: 1; }
  .modal-body { padding: 20px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; flex: 1; }
  .modal-footer { padding: 16px 20px; border-top: 1px solid var(--color-border); display: flex; justify-content: flex-end; gap: 8px; }
  .field { display: flex; flex-direction: column; gap: 6px; }
  .field label { font-size: 12px; color: var(--color-text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .optional { font-size: 11px; text-transform: none; }
  textarea { resize: vertical; min-height: 120px; font-family: monospace; font-size: 12px; }
</style>
