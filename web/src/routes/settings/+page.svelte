<script lang="ts">
  import { settings as settingsApi } from '$lib/api';
  import type { ClaudeSettings } from '$lib/types';
  import { onMount } from 'svelte';

  let s: ClaudeSettings | null = null;
  let saved = false;
  let error = '';

  const MODELS = ['claude-sonnet-4-6', 'claude-opus-4-6', 'claude-haiku-4-5-20251001'];
  const EFFORTS = ['low', 'medium', 'high', 'max'];

  onMount(async () => { s = await settingsApi.get(); });

  async function save() {
    if (!s) return;
    try {
      s = await settingsApi.update(s);
      saved = true;
      setTimeout(() => saved = false, 2000);
    } catch (e: any) {
      error = e.message;
    }
  }
</script>

<h1>Claude Settings</h1>

{#if !s}
  <p class="muted">Loading…</p>
{:else}
  <form on:submit|preventDefault={save}>
    <div class="field">
      <label for="model">Model</label>
      <select id="model" bind:value={s.model}>
        {#each MODELS as m}<option value={m}>{m}</option>{/each}
      </select>
    </div>

    <div class="field">
      <label for="effort">Effort level</label>
      <select id="effort" bind:value={s.effort_level}>
        {#each EFFORTS as e}<option value={e}>{e}</option>{/each}
      </select>
    </div>

    <div class="field">
      <label for="budget">Max budget (USD)</label>
      <input id="budget" type="number" step="0.01" min="0"
        value={s.max_budget_usd ?? ''}
        on:input={e => s!.max_budget_usd = e.currentTarget.value ? Number(e.currentTarget.value) : null}
        placeholder="No limit"
        style="max-width: 140px"
      />
    </div>

    <div class="field">
      <label for="prompt">System prompt append</label>
      <textarea id="prompt" rows="4" bind:value={s.system_prompt_append} placeholder="Additional instructions appended to every Claude session…"></textarea>
    </div>

    <div class="field checkbox">
      <label>
        <input type="checkbox" bind:checked={s.allow_browser_automation} />
        Allow browser automation (Playwright)
      </label>
      <p class="hint">When enabled, Claude can use Playwright to test frontend changes visually.</p>
    </div>

    <div class="actions">
      <button type="submit" class="primary">Save settings</button>
      {#if saved}<span class="saved">✓ Saved</span>{/if}
      {#if error}<span class="error">{error}</span>{/if}
    </div>
  </form>
{/if}

<style>
  h1 { margin: 0 0 24px; font-size: 20px; }
  form { max-width: 520px; display: flex; flex-direction: column; gap: 20px; }
  .field { display: flex; flex-direction: column; gap: 6px; }
  .field label { font-size: 12px; color: var(--color-text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .field.checkbox { flex-direction: row; align-items: flex-start; gap: 10px; }
  .field.checkbox label { text-transform: none; font-size: 13px; color: var(--color-text); display: flex; gap: 8px; align-items: center; cursor: pointer; }
  .hint { margin: 4px 0 0; font-size: 12px; color: var(--color-text-muted); }
  .actions { display: flex; align-items: center; gap: 12px; }
  .saved { color: var(--color-success); font-size: 13px; }
  .error { color: var(--color-error); font-size: 13px; }
  .muted { color: var(--color-text-muted); }
</style>
