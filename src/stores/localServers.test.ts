import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useLocalServersStore, firstUsableServer } from './localServers';
import * as tauri from '../lib/tauri';

vi.mock('../lib/tauri', () => ({ detectLocalModels: vi.fn() }));

const ollama = { provider: 'ollama', label: 'Ollama', base_url: 'http://localhost:11434', models: ['llama3.2'] };

describe('localServers store', () => {
  beforeEach(() => useLocalServersStore.setState({ servers: [], detecting: false }));

  it('stores detected servers', async () => {
    vi.mocked(tauri.detectLocalModels).mockResolvedValue([ollama]);
    await useLocalServersStore.getState().detect();
    expect(useLocalServersStore.getState().servers).toEqual([ollama]);
    expect(useLocalServersStore.getState().detecting).toBe(false);
  });

  it('treats a failed probe as no servers', async () => {
    vi.mocked(tauri.detectLocalModels).mockRejectedValue(new Error('boom'));
    await useLocalServersStore.getState().detect();
    expect(useLocalServersStore.getState().servers).toEqual([]);
  });

  it('firstUsableServer skips servers without models', () => {
    const empty = { ...ollama, provider: 'lmstudio', models: [] };
    expect(firstUsableServer([empty, ollama])).toBe(ollama);
    expect(firstUsableServer([empty])).toBeUndefined();
  });
});
