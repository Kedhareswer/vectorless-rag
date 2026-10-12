import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ModelPicker } from './ModelPicker';
import { useSettingsStore } from '../../stores/settings';
import * as tauri from '../../lib/tauri';
import { useLocalServersStore } from '../../stores/localServers';

vi.mock('../../lib/tauri', () => ({
  getProviders: vi.fn().mockResolvedValue([]),
  saveProvider: vi.fn().mockResolvedValue(undefined),
  deleteProvider: vi.fn().mockResolvedValue(undefined),
}));

const provider = (id: string, name: string, model: string, isActive: boolean) => ({
  id, name, model, isActive, apiKey: 'k', baseUrl: '',
});

describe('ModelPicker', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSettingsStore.setState({
      providers: [provider('a', 'anthropic', 'claude-sonnet-4-5', true), provider('g', 'groq', 'llama-3.3-70b', false)],
      activeProviderId: 'a',
      error: null,
    });
  });

  it('switches the active provider and persists both changed rows', async () => {
    render(<ModelPicker onOpenSettings={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: /claude-sonnet-4-5/ }));
    fireEvent.click(screen.getByRole('menuitemradio', { name: /llama-3.3-70b/ }));
    await vi.waitFor(() => expect(tauri.saveProvider).toHaveBeenCalledTimes(2));
    const s = useSettingsStore.getState();
    expect(s.activeProviderId).toBe('g');
    expect(s.providers.find((p) => p.id === 'a')?.isActive).toBe(false);
  });

  it('lists models installed in local servers and switches to one', async () => {
    useLocalServersStore.setState({
      servers: [{ provider: 'ollama', label: 'Ollama', base_url: 'http://localhost:11434', models: ['qwen3:8b'] }],
    });
    render(<ModelPicker onOpenSettings={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: /claude-sonnet-4-5/ }));
    fireEvent.click(screen.getByRole('menuitemradio', { name: /qwen3:8b/ }));
    await vi.waitFor(() => expect(useSettingsStore.getState().providers.find((p) => p.name === 'ollama')?.isActive).toBe(true));
    expect(tauri.saveProvider).toHaveBeenCalledWith(expect.objectContaining({ name: 'ollama', model: 'qwen3:8b', api_key: null }));
    useLocalServersStore.setState({ servers: [] });
  });

  it('links to provider settings', () => {
    const onOpenSettings = vi.fn();
    render(<ModelPicker onOpenSettings={onOpenSettings} />);
    fireEvent.click(screen.getByRole('button', { name: /claude-sonnet-4-5/ }));
    fireEvent.click(screen.getByRole('menuitem', { name: /Manage providers/ }));
    expect(onOpenSettings).toHaveBeenCalled();
  });

  it('renders nothing without an active provider', () => {
    useSettingsStore.setState({ providers: [], activeProviderId: null });
    const { container } = render(<ModelPicker onOpenSettings={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });
});
