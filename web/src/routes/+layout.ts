// web/src/routes/+layout.ts
import { agent, auth } from '$lib/api';

export const prerender = false;
export const ssr = false;

export async function load() {
  const [agentState, githubStatus] = await Promise.all([
    agent.getState().catch(() => null),
    auth.status().catch(() => null),
  ]);
  return { agentState, githubStatus };
}
