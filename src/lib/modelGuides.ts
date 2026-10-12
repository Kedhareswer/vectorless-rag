import installedVideo from '../assets/guides/installed.webm';
import offlineVideo from '../assets/guides/offline.webm';
import cloudVideo from '../assets/guides/cloud.webm';

/** The three ways to give the app a model that writes answers. */
export type ModelPath = 'installed' | 'offline' | 'cloud';

export interface ModelGuide {
  id: ModelPath;
  title: string;
  /** One line shown on the choice card. */
  summary: string;
  /** Short facts shown as tags: cost, privacy, speed. */
  facts: string[];
  /** Numbered how-to steps shown beside the recording. */
  steps: string[];
  /** Screen recording of the steps (regenerate with scripts/guides). */
  video: string;
  videoLabel: string;
}

/**
 * How-to guides for adding a model, shown when the user first needs one.
 * Recordings live in src/assets/guides and are made by scripts/guides/record.mjs.
 */
export const MODEL_GUIDES: Record<ModelPath, ModelGuide> = {
  installed: {
    id: 'installed',
    title: 'A model you already have',
    summary: 'Use a model from Ollama or LM Studio on this computer.',
    facts: ['Free', 'Private', 'No extra download'],
    steps: [
      'Install Ollama (ollama.com) or LM Studio (lmstudio.ai).',
      'Get a model, e.g. run: ollama pull llama3.2:3b — in LM Studio, load a model and start the local server.',
      'Come back here and click Check again. Pick the model and you are done.',
    ],
    video: installedVideo,
    videoLabel: 'Recording: picking a model that Ollama already has installed',
  },
  offline: {
    id: 'offline',
    title: 'Built-in offline model',
    summary: 'LFM2.5 1.2B runs inside the app. One ~700 MB download.',
    facts: ['Free', 'Private', 'Slower on CPU'],
    steps: [
      'Click Download and use. The download runs in the background.',
      'When it finishes, answers are written on this computer — no internet needed.',
      'The same model also improves search for every provider.',
    ],
    video: offlineVideo,
    videoLabel: 'Recording: downloading the built-in offline model',
  },
  cloud: {
    id: 'cloud',
    title: 'Cloud provider',
    summary: 'Anthropic, OpenAI, Google and others. Best answers, needs an API key.',
    facts: ['Pay per use', 'Fastest', 'Sends passages online'],
    steps: [
      'Pick a provider and open its key page.',
      'Create an API key there and paste it here. Keys stay in your OS keychain.',
      'Click Save and use. You can switch models later from the chat box.',
    ],
    video: cloudVideo,
    videoLabel: 'Recording: adding an Anthropic API key',
  },
};

export const MODEL_PATH_ORDER: ModelPath[] = ['installed', 'offline', 'cloud'];

/** Cloud providers offered in the add-model guide, with where to get a key. */
export const CLOUD_KEY_PAGES: { preset: string; label: string; url: string }[] = [
  { preset: 'anthropic', label: 'Anthropic (Claude)', url: 'https://console.anthropic.com/settings/keys' },
  { preset: 'openai', label: 'OpenAI', url: 'https://platform.openai.com/api-keys' },
  { preset: 'google', label: 'Google AI Studio', url: 'https://aistudio.google.com/apikey' },
  { preset: 'groq', label: 'Groq', url: 'https://console.groq.com/keys' },
  { preset: 'openrouter', label: 'OpenRouter', url: 'https://openrouter.ai/settings/keys' },
  { preset: 'deepseek', label: 'DeepSeek', url: 'https://platform.deepseek.com/api_keys' },
];
