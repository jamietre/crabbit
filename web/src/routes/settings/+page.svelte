<script lang="ts">
  import { settings as settingsApi, agent as agentApi } from '$lib/api';
  import type { ClaudeSettings, AgentState } from '$lib/types';
  import { onMount } from 'svelte';

  let s: ClaudeSettings | null = null;
  let agentState: AgentState | null = null;
  let saved = false;
  let error = '';

  const MODELS = ['claude-sonnet-4-6', 'claude-opus-4-6', 'claude-haiku-4-5-20251001'];
  const EFFORTS = ['low', 'medium', 'high', 'max'];

  onMount(async () => {
    [s, agentState] = await Promise.all([settingsApi.get(), agentApi.getState()]);
  });

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

  function usageColor(pct: number): string {
    if (pct >= 90) return 'var(--color-error)';
    if (pct >= 70) return 'var(--color-warning)';
    if (pct >= 50) return 'var(--color-text)';
    return 'var(--color-success)';
  }

  function formatResetAt(ts: number | null): string {
    if (!ts) return '';
    return new Date(ts * 1000).toLocaleString();
  }
</script>

<h1>Claude Settings</h1>

{#if !s}
  <p class="muted">Loading…</p>
{:else}
  {#if agentState?.usage_pct_7d != null}
    <div class="usage-banner">
      <span class="usage-label">7-day Claude Pro usage</span>
      <span class="usage-bar-wrap">
        <span class="usage-bar" style="width: {agentState.usage_pct_7d}%; background: {usageColor(agentState.usage_pct_7d)}"></span>
      </span>
      <span class="usage-pct" style="color: {usageColor(agentState.usage_pct_7d)}">{Math.round(agentState.usage_pct_7d)}%</span>
      {#if agentState.usage_reset_at}
        <span class="usage-reset">resets {formatResetAt(agentState.usage_reset_at)}</span>
      {/if}
    </div>
  {/if}

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
      <label for="usage-limit">7-day usage limit (%)</label>
      <input id="usage-limit" type="number" step="1" min="0" max="100"
        value={s.usage_limit_pct ?? ''}
        on:input={e => s!.usage_limit_pct = e.currentTarget.value ? Number(e.currentTarget.value) : null}
        placeholder="No limit"
        style="max-width: 140px"
      />
      <p class="hint">Stop starting new tasks when 7-day Pro usage exceeds this percentage.</p>
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

  .usage-banner {
    display: flex; align-items: center; gap: 10px;
    background: var(--color-surface); border: 1px solid var(--color-border);
    border-radius: 8px; padding: 12px 16px;
    max-width: 520px; margin-bottom: 24px;
    font-size: 13px;
  }
  .usage-label { color: var(--color-text-muted); flex-shrink: 0; }
  .usage-bar-wrap {
    flex: 1; height: 6px; background: var(--color-border); border-radius: 3px; overflow: hidden;
  }
  .usage-bar { display: block; height: 100%; border-radius: 3px; transition: width 0.3s; }
  .usage-pct { font-weight: 600; flex-shrink: 0; min-width: 38px; text-align: right; }
  .usage-reset { color: var(--color-text-muted); font-size: 11px; flex-shrink: 0; }
</style>
