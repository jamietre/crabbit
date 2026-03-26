<script lang="ts">
  import { settings as settingsApi, agent as agentApi, claudeAuth as claudeAuthApi } from '$lib/api';
  import type { ClaudeSettings, AgentState, ClaudeAuthStatus } from '$lib/types';
  import { onMount } from 'svelte';

  let s: ClaudeSettings | null = null;
  let agentState: AgentState | null = null;
  let claudeAuthStatus: ClaudeAuthStatus | null = null;
  let saved = false;
  let error = '';
  let clearingAuth = false;

  const MODELS = ['claude-sonnet-4-6', 'claude-opus-4-6', 'claude-haiku-4-5-20251001'];
  const EFFORTS = ['low', 'medium', 'high', 'max'];

  onMount(async () => {
    [s, agentState, claudeAuthStatus] = await Promise.all([
      settingsApi.get(),
      agentApi.getState(),
      claudeAuthApi.status(),
    ]);
  });

  async function clearClaudeAuth() {
    clearingAuth = true;
    try {
      await claudeAuthApi.clear();
      claudeAuthStatus = { configured: false, updated_at: null };
    } finally {
      clearingAuth = false;
    }
  }

  function formatTs(ts: number | null): string {
    if (!ts) return '';
    return new Date(ts * 1000).toLocaleString();
  }

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

</script>

<h1>Claude Settings</h1>

{#if !s}
  <p class="muted">Loading…</p>
{:else}
  {#if claudeAuthStatus !== null}
    <div class="claude-auth-card">
      <div class="claude-auth-row">
        <span class="claude-auth-label">Claude credentials</span>
        {#if claudeAuthStatus.configured}
          <span class="badge badge-ok">Synced</span>
          <span class="claude-auth-sub">Updated {formatTs(claudeAuthStatus.updated_at)}</span>
          <button class="btn-link danger" on:click={clearClaudeAuth} disabled={clearingAuth}>
            {clearingAuth ? 'Clearing…' : 'Clear'}
          </button>
        {:else}
          <span class="badge badge-warn">Not configured</span>
          <span class="claude-auth-sub">Run <code>install-desktop-sync.sh</code> on your desktop to push credentials.</span>
        {/if}
      </div>
    </div>
  {/if}

  {#if agentState?.usage_pct_7d != null}
    <div class="usage-banner">
      <span class="usage-label">7-day Claude Pro usage</span>
      <span class="usage-bar-wrap">
        <span class="usage-bar" style="width: {agentState.usage_pct_7d}%; background: {usageColor(agentState.usage_pct_7d)}"></span>
      </span>
      <span class="usage-pct" style="color: {usageColor(agentState.usage_pct_7d)}">{Math.round(agentState.usage_pct_7d)}%</span>
      {#if agentState.usage_reset_at}
        <span class="usage-reset">resets {formatTs(agentState.usage_reset_at)}</span>
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

  .claude-auth-card {
    background: var(--color-surface); border: 1px solid var(--color-border);
    border-radius: 8px; padding: 12px 16px;
    max-width: 520px; margin-bottom: 24px;
  }
  .claude-auth-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; font-size: 13px; }
  .claude-auth-label { color: var(--color-text-muted); flex-shrink: 0; }
  .claude-auth-sub { color: var(--color-text-muted); font-size: 12px; }
  .claude-auth-sub code { font-family: monospace; font-size: 11px; background: var(--color-border); padding: 1px 4px; border-radius: 3px; }
  .badge { font-size: 11px; font-weight: 600; padding: 2px 8px; border-radius: 10px; }
  .badge-ok { background: color-mix(in srgb, var(--color-success) 15%, transparent); color: var(--color-success); }
  .badge-warn { background: color-mix(in srgb, var(--color-warning) 15%, transparent); color: var(--color-warning); }
  .btn-link { background: none; border: none; cursor: pointer; font-size: 12px; padding: 0; }
  .btn-link.danger { color: var(--color-error); }
  .btn-link:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
