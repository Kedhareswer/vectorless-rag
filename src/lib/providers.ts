import { LOCAL_PROVIDER } from '../stores/settings';

const DISPLAY_NAMES: Record<string, string> = {
  [LOCAL_PROVIDER]: 'Local (offline)',
  openai: 'OpenAI',
  'openai-compat': 'OpenAI compatible',
  xai: 'xAI',
  openrouter: 'OpenRouter',
  agentrouter: 'AgentRouter',
  deepseek: 'DeepSeek',
};

/** Human name for a provider type, e.g. "anthropic" → "Anthropic". */
export function providerDisplayName(name: string): string {
  return DISPLAY_NAMES[name] ?? name.charAt(0).toUpperCase() + name.slice(1);
}

/** What answers: the model ID, or "Offline model" for the local provider. */
export function providerModelLabel(p: { name: string; model: string }): string {
  if (p.name === LOCAL_PROVIDER) return 'Offline model';
  return p.model || providerDisplayName(p.name);
}

export const isLocalProvider = (p: { name: string } | undefined | null): boolean =>
  p?.name === LOCAL_PROVIDER;
