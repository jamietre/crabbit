<!-- web/src/routes/auth/+page.svelte -->
<script lang="ts">
  import { githubStatus } from '$lib/stores';
  import { auth as authApi } from '$lib/api';

  let connecting = false;
  let connectError = '';

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

  async function disconnect() {
    await authApi.disconnect();
    githubStatus.set({ connected: false, github_login: null, token_scopes: null, connected_at: null });
  }

  function formatDate(ts: number | null) {
    if (!ts) return '';
    return new Date(ts * 1000).toLocaleDateString();
  }
</script>

<h1>GitHub Connection</h1>

{#if $githubStatus?.connected}
  <div class="connected-card">
    <div class="gh-icon">✓</div>
    <div class="gh-info">
      <div class="gh-login">@{$githubStatus.github_login}</div>
      <div class="gh-meta">
        Connected {formatDate($githubStatus.connected_at)} · Scopes: {$githubStatus.token_scopes}
      </div>
    </div>
    <button class="danger" on:click={disconnect}>Disconnect</button>
  </div>
  <p class="hint">
    The agent uses this GitHub account to read issues, create PRs, and post comments.
    Disconnecting will pause the agent until you reconnect.
  </p>
{:else}
  <div class="connect-card">
    <h2>Connect your GitHub account</h2>
    <p>Crabbit needs access to your GitHub repos to read issues and create pull requests.</p>
    <p><strong>Required scopes:</strong> <code>repo</code>, <code>read:user</code></p>
    <button class="primary" on:click={connectGitHub} disabled={connecting}>
      {connecting ? 'Redirecting…' : 'Connect GitHub'}
    </button>
    {#if connectError}<p class="error">{connectError}</p>{/if}
  </div>
{/if}

<style>
  h1 { margin: 0 0 24px; font-size: 20px; }
  .connected-card {
    display: flex; align-items: center; gap: 16px;
    background: var(--color-surface);
    border: 1px solid var(--color-success);
    border-radius: 8px; padding: 20px;
    max-width: 520px; margin-bottom: 16px;
  }
  .gh-icon {
    width: 40px; height: 40px; border-radius: 50%;
    background: var(--color-success);
    display: flex; align-items: center; justify-content: center;
    font-size: 20px; flex-shrink: 0;
  }
  .gh-info { flex: 1; }
  .gh-login { font-size: 16px; font-weight: 600; }
  .gh-meta { font-size: 12px; color: var(--color-text-muted); margin-top: 2px; }
  .hint { font-size: 12px; color: var(--color-text-muted); max-width: 520px; }
  .connect-card {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px; padding: 24px;
    max-width: 480px;
  }
  .connect-card h2 { margin: 0 0 12px; font-size: 16px; }
  .connect-card p { font-size: 13px; color: var(--color-text-muted); margin: 0 0 12px; }
  code { background: var(--color-border); padding: 2px 5px; border-radius: 3px; font-size: 12px; }
  .error { color: var(--color-error); font-size: 12px; margin: 8px 0 0; }
</style>
