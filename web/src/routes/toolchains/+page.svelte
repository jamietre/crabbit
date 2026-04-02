<script lang="ts">
  import { toolchains as toolchainsApi } from '$lib/api';
  import type { Toolchain } from '$lib/types';
  import { onMount, onDestroy } from 'svelte';

  let items: Toolchain[] = [];
  let error = '';
  let showAddForm = false;
  let newName = '';
  let newDisplayName = '';
  let newStepsText = '';
  let generateDescription = '';
  let generating = false;
  let acting: Record<string, boolean> = {};
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  const STATUS_LABELS: Record<string, string> = {
    not_pulled: 'Not pulled',
    pulling: 'Pulling…',
    available: 'Available',
    pull_failed: 'Pull failed',
    pending: 'Not built',
    building: 'Building…',
    build_failed: 'Build failed',
  };

  function isActive(tc: Toolchain) {
    return tc.image_status === 'pulling' || tc.image_status === 'building';
  }

  onMount(async () => {
    await load();
    pollInterval = setInterval(async () => {
      if (items.some(isActive)) await load();
    }, 3000);
  });

  onDestroy(() => { if (pollInterval) clearInterval(pollInterval); });

  async function load() {
    try {
      items = await toolchainsApi.list();
      error = '';
    } catch (e) {
      error = String(e);
    }
  }

  async function pull(name: string) {
    acting[name] = true;
    try {
      await toolchainsApi.pull(name);
      items = items.map(t => t.name === name ? { ...t, image_status: 'pulling' } : t);
    } catch (e) {
      error = String(e);
    } finally {
      acting[name] = false;
    }
  }

  async function build(name: string) {
    acting[name] = true;
    try {
      await toolchainsApi.build(name);
      items = items.map(t => t.name === name ? { ...t, image_status: 'building', build_log: '' } : t);
    } catch (e) {
      error = String(e);
    } finally {
      acting[name] = false;
    }
  }

  async function remove(name: string) {
    if (!confirm(`Delete toolchain "${name}"? This cannot be undone.`)) return;
    acting[name] = true;
    try {
      await toolchainsApi.delete(name);
      items = items.filter(t => t.name !== name);
    } catch (e) {
      error = String(e);
    } finally {
      acting[name] = false;
    }
  }

  async function generateSteps() {
    if (!generateDescription.trim()) return;
    generating = true;
    error = '';
    try {
      const res = await toolchainsApi.generateSteps(generateDescription);
      newStepsText = res.steps.join('\n');
    } catch (e) {
      error = String(e);
    } finally {
      generating = false;
    }
  }

  async function addToolchain() {
    const steps = newStepsText.split('\n').map(s => s.trim()).filter(Boolean);
    try {
      const tc = await toolchainsApi.create(newName.trim(), newDisplayName.trim(), steps);
      items = [...items, tc].sort((a, b) => a.name.localeCompare(b.name));
      showAddForm = false;
      newName = '';
      newDisplayName = '';
      newStepsText = '';
      generateDescription = '';
      error = '';
    } catch (e) {
      error = String(e);
    }
  }

  function resetAddForm() {
    showAddForm = false;
    newName = '';
    newDisplayName = '';
    newStepsText = '';
    generateDescription = '';
  }
</script>

<div class="page">
  <div class="page-header">
    <h1>Toolchains</h1>
    <p>Manage Docker images used to run agent tasks. Built-in images are pulled from GitHub Container Registry; custom images are built locally.</p>
  </div>

  {#if error}
    <div class="error-banner">{error}</div>
  {/if}

  <div class="toolchain-list">
    {#each items as tc}
      <div class="toolchain-card" class:available={tc.image_status === 'available'}>
        <div class="tc-header">
          <div class="tc-title">
            <span class="tc-name">{tc.display_name}</span>
            {#if tc.builtin}
              <span class="badge builtin">Built-in</span>
            {:else}
              <span class="badge custom">Custom</span>
            {/if}
            <span class="badge status-{tc.image_status}">{STATUS_LABELS[tc.image_status] ?? tc.image_status}</span>
          </div>
          <div class="tc-actions">
            {#if tc.builtin && tc.image_status !== 'available'}
              <button
                on:click={() => pull(tc.name)}
                disabled={acting[tc.name] || isActive(tc)}
                class="btn-primary"
              >
                {tc.image_status === 'pull_failed' ? 'Retry Pull' : 'Pull'}
              </button>
            {:else if !tc.builtin}
              <button
                on:click={() => build(tc.name)}
                disabled={acting[tc.name] || isActive(tc)}
                class="btn-primary"
              >
                {tc.image_status === 'build_failed' ? 'Rebuild' : tc.image_status === 'available' ? 'Rebuild' : 'Build'}
              </button>
              <button
                on:click={() => remove(tc.name)}
                disabled={acting[tc.name] || isActive(tc)}
                class="btn-danger"
              >
                Delete
              </button>
            {/if}
          </div>
        </div>

        <div class="tc-image"><code>{tc.image}</code></div>

        {#if tc.detection_markers.length > 0}
          <div class="tc-markers">
            Detects: {#each tc.detection_markers as m}<code>{m}</code> {/each}
          </div>
        {/if}

        {#if tc.build_log}
          <pre class="build-log">{tc.build_log}</pre>
        {/if}
      </div>
    {/each}
  </div>

  <div class="add-section">
    {#if !showAddForm}
      <button on:click={() => showAddForm = true} class="btn-secondary">+ Add custom toolchain</button>
    {:else}
      <div class="add-form">
        <h2>Add Custom Toolchain</h2>

        <label>
          Name (identifier, e.g. "elixir")
          <input bind:value={newName} placeholder="elixir" />
        </label>

        <label>
          Display name
          <input bind:value={newDisplayName} placeholder="Elixir / Mix" />
        </label>

        <div class="generate-row">
          <input
            bind:value={generateDescription}
            placeholder="Describe what you need, e.g. 'Elixir 1.16 with Phoenix and Mix'"
          />
          <button on:click={generateSteps} disabled={generating || !generateDescription.trim()} class="btn-secondary">
            {generating ? 'Generating…' : 'Ask Claude'}
          </button>
        </div>

        <label>
          Install commands (one per line — will be RUN in a Dockerfile)
          <textarea
            bind:value={newStepsText}
            rows="6"
            placeholder="apt-get install -y elixir&#10;mix local.hex --force&#10;mix local.rebar --force"
          ></textarea>
        </label>

        <div class="form-actions">
          <button
            on:click={addToolchain}
            disabled={!newName.trim() || !newDisplayName.trim()}
            class="btn-primary"
          >
            Create
          </button>
          <button on:click={resetAddForm} class="btn-secondary">Cancel</button>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .page { max-width: 800px; margin: 0 auto; padding: 1.5rem; }
  .page-header { margin-bottom: 1.5rem; }
  .page-header h1 { margin: 0 0 0.5rem; }
  .page-header p { margin: 0; color: var(--text-muted, #888); }
  .error-banner { background: #fee; border: 1px solid #fcc; border-radius: 4px; padding: 0.75rem; margin-bottom: 1rem; color: #c00; }
  .toolchain-list { display: flex; flex-direction: column; gap: 0.75rem; margin-bottom: 1.5rem; }
  .toolchain-card { border: 1px solid var(--border, #ddd); border-radius: 6px; padding: 1rem; }
  .toolchain-card.available { border-color: #4caf50; }
  .tc-header { display: flex; align-items: center; justify-content: space-between; gap: 1rem; margin-bottom: 0.5rem; }
  .tc-title { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .tc-name { font-weight: 600; font-size: 1rem; }
  .tc-actions { display: flex; gap: 0.5rem; flex-shrink: 0; }
  .tc-image { font-size: 0.85rem; color: var(--text-muted, #888); margin-bottom: 0.25rem; }
  .tc-markers { font-size: 0.8rem; color: var(--text-muted, #888); }
  .tc-markers code { background: var(--code-bg, #f5f5f5); padding: 0 3px; border-radius: 3px; }
  .build-log { font-size: 0.75rem; background: #1e1e1e; color: #d4d4d4; padding: 0.75rem; border-radius: 4px; overflow-x: auto; max-height: 200px; overflow-y: auto; margin-top: 0.75rem; white-space: pre-wrap; }
  .badge { font-size: 0.7rem; padding: 2px 6px; border-radius: 3px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.03em; }
  .badge.builtin { background: #e3f2fd; color: #1565c0; }
  .badge.custom { background: #f3e5f5; color: #6a1b9a; }
  .badge.status-available { background: #e8f5e9; color: #2e7d32; }
  .badge.status-pulling, .badge.status-building { background: #fff3e0; color: #e65100; }
  .badge.status-pull_failed, .badge.status-build_failed { background: #ffebee; color: #c62828; }
  .badge.status-not_pulled, .badge.status-pending { background: #f5f5f5; color: #616161; }
  .add-section { margin-top: 1rem; }
  .add-form { border: 1px solid var(--border, #ddd); border-radius: 6px; padding: 1.25rem; }
  .add-form h2 { margin: 0 0 1rem; font-size: 1.1rem; }
  label { display: flex; flex-direction: column; gap: 0.3rem; margin-bottom: 0.75rem; font-size: 0.9rem; }
  input, textarea { padding: 0.4rem 0.6rem; border: 1px solid var(--border, #ddd); border-radius: 4px; font-size: 0.9rem; font-family: inherit; }
  textarea { resize: vertical; font-family: monospace; }
  .generate-row { display: flex; gap: 0.5rem; margin-bottom: 0.75rem; }
  .generate-row input { flex: 1; }
  .form-actions { display: flex; gap: 0.5rem; margin-top: 0.25rem; }
  .btn-primary { padding: 0.4rem 0.9rem; background: #1976d2; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 0.85rem; }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-secondary { padding: 0.4rem 0.9rem; background: transparent; border: 1px solid var(--border, #ddd); border-radius: 4px; cursor: pointer; font-size: 0.85rem; }
  .btn-danger { padding: 0.4rem 0.9rem; background: transparent; border: 1px solid #e57373; color: #c62828; border-radius: 4px; cursor: pointer; font-size: 0.85rem; }
  .btn-danger:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
