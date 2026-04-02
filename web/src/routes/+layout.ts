// web/src/routes/+layout.ts
import { agent, auth, claudeAuth } from '$lib/api';

export const prerender = false;
export const ssr = false;

export async function load() {
  const [agentState, githubStatus, claudeAuthStatus] = await Promise.all([
    agent.getState().catch(() => null),
    auth.status().catch(() => null),
    claudeAuth.status().catch(() => null),
  ]);
  return { agentState, githubStatus, claudeAuthStatus };
}
