<!-- web/src/routes/auth/+page.svelte -->
<script lang="ts">
  import { githubStatus, claudeAuthStatus } from '$lib/stores';
  import { auth as authApi, claudeAuth as claudeAuthApi } from '$lib/api';
  import { onMount } from 'svelte';

  let connecting = false;
  let connectError = '';
  let checkingClaude = false;
  let clearingClaude = false;

  onMount(async () => {
    claudeAuthStatus.set(await claudeAuthApi.status().catch(() => null));
  });

  async function connectGitHub() {
    connecting = true;
    connectError = '';
    try {
      const { url } = await authApi.beginOAuth();
      window.location.href = url;
    } catch (e: any) {
      connectError = e.message;
      connecting = false;
    }
  }

  async function disconnectGitHub() {
    await authApi.disconnect();
    githubStatus.set({ connected: false, github_login: null, token_scopes: null, connected_at: null });
  }

  async function checkClaude() {
    checkingClaude = true;
    try {
      const result = await claudeAuthApi.check();
      claudeAuthStatus.update(s => s ? { ...s, check: result } : s);
    } finally {
      checkingClaude = false;
    }
  }

  async function clearClaude() {
    clearingClaude = true;
    try {
      await claudeAuthApi.clear();
      claudeAuthStatus.update(s => s ? { ...s, configured: false, updated_at: null, check: { status: 'unknown', checked_at: null, error: null } } : s);
    } finally {
      clearingClaude = false;
    }
  }

  function formatDate(ts: number | null) {
    if (!ts) return '';
    return new Date(ts * 1000).toLocaleString();
  }

  $: claudeOk      = $claudeAuthStatus?.check.status === 'ok';
  $: claudeExpired = $claudeAuthStatus?.check.status === 'expired';
</script>

<h1>Connections</h1>

<section>
  <h2>GitHub</h2>
  {#if $githubStatus?.connected}
    <div class="account-card connected">
      <div class="account-dot ok"></div>
      <div class="account-info">
        <div class="account-name">@{$githubStatus.github_login}</div>
        <div class="account-meta">
          Connected {formatDate($githubStatus.connected_at)} · Scopes: {$githubStatus.token_scopes}
        </div>
      </div>
      <button class="btn-danger" on:click={disconnectGitHub}>Disconnect</button>
    </div>
    <p class="hint">Used to read issues, create PRs, and post comments.</p>
  {:else}
    <div class="account-card disconnected">
      <div class="account-dot warn"></div>
      <div class="account-info">
        <div class="account-name">Not connected</div>
        <div class="account-meta">Required to read issues and create pull requests.</div>
      </div>
      <button class="btn-primary" on:click={connectGitHub} disabled={connecting}>
        {connecting ? 'Redirecting…' : 'Connect GitHub'}
      </button>
    </div>
    {#if connectError}<p class="error">{connectError}</p>{/if}
    <p class="hint"><strong>Required scopes:</strong> <code>repo</code>, <code>read:user</code></p>
  {/if}
</section>

<section>
  <h2>Claude</h2>
  {#if $claudeAuthStatus !== null}
    <div class="account-card" class:connected={claudeOk} class:disconnected={claudeExpired || !$claudeAuthStatus.configured}>
      <div class="account-dot" class:ok={claudeOk} class:error={claudeExpired} class:warn={!claudeOk && !claudeExpired}></div>
      <div class="account-info">
        {#if $claudeAuthStatus.configured}
          <div class="account-name">Credentials synced</div>
          <div class="account-meta">
            Updated {formatDate($claudeAuthStatus.updated_at)} ·
            Auth {$claudeAuthStatus.check.status}
            {#if $claudeAuthStatus.check.checked_at}· checked {formatDate($claudeAuthStatus.check.checked_at)}{/if}
          </div>
          {#if $claudeAuthStatus.check.error && !claudeOk}
            <div class="account-error">{$claudeAuthStatus.check.error}</div>
          {/if}
        {:else}
          <div class="account-name">Not configured</div>
          <div class="account-meta">Run <code>install-desktop-sync.sh</code> on your desktop to push credentials.</div>
        {/if}
      </div>
      <div class="account-actions">
        <button class="btn-secondary" on:click={checkClaude} disabled={checkingClaude}>
          {checkingClaude ? 'Checking…' : 'Check now'}
        </button>
        {#if $claudeAuthStatus.configured}
          <button class="btn-danger" on:click={clearClaude} disabled={clearingClaude}>
            {clearingClaude ? 'Clearing…' : 'Clear'}
          </button>
        {/if}
      </div>
    </div>
    <p class="hint">Claude credentials are pushed from your desktop via the sync daemon and stored on the server.</p>
  {:else}
    <p class="muted">Loading…</p>
  {/if}
</section>

<style>
  h1 { margin: 0 0 28px; font-size: 20px; }
  section { margin-bottom: 32px; max-width: 560px; }
  h2 { margin: 0 0 12px; font-size: 13px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-text-muted); }

  .account-card {
    display: flex; align-items: center; gap: 14px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px; padding: 16px 20px;
    margin-bottom: 8px;
  }
  .account-card.connected { border-color: var(--color-success); }
  .account-card.disconnected { border-color: var(--color-border); }

  .account-dot {
    width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0;
    background: var(--color-text-muted);
  }
  .account-dot.ok    { background: var(--color-success); }
  .account-dot.warn  { background: var(--color-warning); }
  .account-dot.error { background: var(--color-error); }

  .account-info { flex: 1; }
  .account-name { font-size: 14px; font-weight: 600; }
  .account-meta { font-size: 12px; color: var(--color-text-muted); margin-top: 2px; }
  .account-error { font-size: 12px; color: var(--color-error); margin-top: 4px; }
  .account-actions { display: flex; gap: 8px; flex-shrink: 0; }

  .hint { font-size: 12px; color: var(--color-text-muted); margin: 4px 0 0; }
  .muted { color: var(--color-text-muted); font-size: 13px; }
  .error { color: var(--color-error); font-size: 12px; margin: 8px 0 0; }

  code { background: var(--color-border); padding: 2px 5px; border-radius: 3px; font-size: 11px; }

  button { font-size: 12px; padding: 5px 12px; border-radius: 6px; cursor: pointer; border: 1px solid; }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-primary { background: var(--color-info); color: #fff; border-color: var(--color-info); }
  .btn-secondary { background: none; color: var(--color-text-muted); border-color: var(--color-border); }
  .btn-danger { background: none; color: var(--color-error); border-color: var(--color-error); }
</style>
