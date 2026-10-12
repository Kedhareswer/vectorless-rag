import type { ProviderConfig } from '../stores/settings';

/** Provider types the app can configure; each maps to a backend provider name. */
export type ProviderPreset =
  | 'local'
  | 'lmstudio'
  | 'ollama'
  | 'groq'
  | 'google'
  | 'openrouter'
  | 'agentrouter'
  | 'anthropic'
  | 'openai'
  | 'deepseek'
  | 'xai'
  | 'qwen'
  | 'openai-compat';

/** Suggested model IDs per provider. Any other ID can be typed in. */
export const PROVIDER_MODELS: Partial<Record<ProviderPreset, string[]>> = {
  google: [
    'gemini-2.5-flash',
    'gemini-2.5-pro',
    'gemini-2.0-flash',
    'gemini-2.0-pro',
  ],
  anthropic: [
    'claude-sonnet-5-5',
    'claude-opus-5-5',
    'claude-haiku-5-5',
    'claude-haiku-4-5-20251001',
    'claude-sonnet-4-6',
    'claude-opus-4-6',
  ],
  openai: [
    'gpt-4o-mini',
    'gpt-4o',
    'gpt-4.1-mini',
    'gpt-4.1',
    'o3-mini',
    'o4-mini',
  ],
  deepseek: [
    'deepseek-chat',
    'deepseek-reasoner',
  ],
  xai: [
    'grok-3-mini',
    'grok-3',
    'grok-4-fast-non-reasoning',
    'grok-4-0709',
  ],
  qwen: [
    'qwen-turbo',
    'qwen-plus',
    'qwen-max',
    'qwq-plus',
    'qwen3-coder-plus',
  ],
};

export const PROVIDER_PRESETS: Record<ProviderPreset, Omit<ProviderConfig, 'id' | 'isActive'>> = {
  ollama: {
    name: 'ollama',
    apiKey: '',
    baseUrl: 'http://localhost:11434',
    model: 'llama3.2',
  },
  groq: {
    name: 'groq',
    apiKey: '',
    baseUrl: 'https://api.groq.com/openai/v1',
    model: 'llama-3.3-70b-versatile',
  },
  google: {
    name: 'google',
    apiKey: '',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta',
    model: 'gemini-2.5-flash',
  },
  openrouter: {
    name: 'openrouter',
    apiKey: '',
    baseUrl: 'https://openrouter.ai/api/v1',
    model: 'anthropic/claude-sonnet-4-6',
  },
  agentrouter: {
    name: 'agentrouter',
    apiKey: '',
    baseUrl: 'https://agentrouter.org/v1',
    model: 'claude-sonnet-4-5-20250514',
  },
  anthropic: {
    name: 'anthropic',
    apiKey: '',
    baseUrl: 'https://api.anthropic.com/v1',
    model: 'claude-haiku-4-5-20251001',
  },
  openai: {
    name: 'openai',
    apiKey: '',
    baseUrl: 'https://api.openai.com/v1',
    model: 'gpt-4o-mini',
  },
  deepseek: {
    name: 'deepseek',
    apiKey: '',
    baseUrl: 'https://api.deepseek.com/v1',
    model: 'deepseek-chat',
  },
  xai: {
    name: 'xai',
    apiKey: '',
    baseUrl: 'https://api.x.ai/v1',
    model: 'grok-3-mini',
  },
  qwen: {
    name: 'qwen',
    apiKey: '',
    baseUrl: 'https://dashscope-intl.aliyuncs.com/compatible-mode/v1',
    model: 'qwen-turbo',
  },
  'openai-compat': {
    name: 'openai-compat',
    apiKey: '',
    baseUrl: '',
    model: '',
  },
  lmstudio: {
    name: 'lmstudio',
    apiKey: '',
    baseUrl: 'http://localhost:1234/v1',
    model: '',
  },
  local: {
    name: 'local',
    apiKey: '',
    baseUrl: '',
    model: 'on-device',
  },
};
