// web/src/lib/stores.ts
import { writable } from 'svelte/store';
import type { AgentState, ClaudeAuthStatus, GitHubAuthStatus } from './types';

export const agentState = writable<AgentState | null>(null);
export const githubStatus = writable<GitHubAuthStatus | null>(null);
export const claudeAuthStatus = writable<ClaudeAuthStatus | null>(null);

// Refresh agent state every 10 seconds when page is visible
export function startPolling(fetchFn: () => Promise<void>) {
  let interval: ReturnType<typeof setInterval>;
  const start = () => { interval = setInterval(fetchFn, 10_000); };
  const stop = () => clearInterval(interval);

  if (typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', () => {
      document.hidden ? stop() : start();
    });
    start();
  }

  return stop;
}
