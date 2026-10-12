/** "640ms" under a second, otherwise "3.1s". */
export function formatLatency(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

/** Dollar cost with enough precision for sub-cent amounts. */
export function formatCost(cost: number): string {
  if (cost > 0 && cost < 0.01) return `$${cost.toFixed(4)}`;
  return `$${cost.toFixed(2)}`;
}

/** Thousands-separated integer, e.g. 6,210. */
export function formatCount(n: number): string {
  return Math.round(n).toLocaleString('en-US');
}
