import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { WelcomeTour } from './WelcomeTour';
import { AddModelDialog } from './AddModelDialog';
import { useLocalServersStore } from '../../stores/localServers';
import { useSettingsStore } from '../../stores/settings';
import * as tauri from '../../lib/tauri';

vi.mock('../../lib/tauri', () => ({
  detectLocalModels: vi.fn().mockResolvedValue([]),
  saveProvider: vi.fn().mockResolvedValue(undefined),
  getProviders: vi.fn().mockResolvedValue([]),
  deleteProvider: vi.fn().mockResolvedValue(undefined),
  openExternal: vi.fn().mockResolvedValue(undefined),
}));

const ollama = { provider: 'ollama', label: 'Ollama', base_url: 'http://localhost:11434', models: ['llama3.2:3b'] };

describe('WelcomeTour', () => {
  beforeEach(() => useLocalServersStore.setState({ servers: [], detecting: false }));

  it('walks through why a model is needed and the ways to get one', () => {
    useLocalServersStore.setState({ servers: [ollama] });
    const onClose = vi.fn();
    render(<WelcomeTour onClose={onClose} />);
    expect(screen.getByRole('heading', { name: 'Ask questions about your documents' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Next/ }));
    expect(screen.getByRole('heading', { name: 'Why does it need a model?' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Next/ }));
    expect(screen.getByText('Built-in offline model')).toBeInTheDocument();
    expect(screen.getByText('Found Ollama · 1 model')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Get started' }));
    expect(onClose).toHaveBeenCalled();
  });

  it('can be skipped', () => {
    const onClose = vi.fn();
    render(<WelcomeTour onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }));
    expect(onClose).toHaveBeenCalled();
  });
});

describe('AddModelDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useLocalServersStore.setState({ servers: [], detecting: false });
    useSettingsStore.setState({ providers: [], activeProviderId: null, error: null });
  });

  it('opens on installed models when one is found and uses the picked one', async () => {
    vi.mocked(tauri.detectLocalModels).mockResolvedValue([ollama]);
    useLocalServersStore.setState({ servers: [ollama] });
    const onClose = vi.fn();
    render(<AddModelDialog initialPath={null} onClose={onClose} onSetupOffline={vi.fn()} />);
    expect(screen.getByRole('tab', { name: /A model you already have/ })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByLabelText(/Recording: picking a model/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /llama3.2:3b/ }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(useSettingsStore.getState().providers[0]).toMatchObject({ name: 'ollama', model: 'llama3.2:3b', isActive: true });
  });

  it('explains how to install one when none is running', async () => {
    vi.mocked(tauri.detectLocalModels).mockResolvedValue([]);
    render(<AddModelDialog initialPath="installed" onClose={vi.fn()} onSetupOffline={vi.fn()} />);
    expect(await screen.findByText('No running Ollama or LM Studio found yet.')).toBeInTheDocument();
    expect(screen.getByText('ollama pull llama3.2:3b')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Get Ollama/ }));
    expect(tauri.openExternal).toHaveBeenCalledWith('https://ollama.com/download');
    fireEvent.click(screen.getByRole('button', { name: /Check again/ }));
    await waitFor(() => expect(tauri.detectLocalModels).toHaveBeenCalledTimes(2));
  });

  it('starts the offline model setup', () => {
    const onSetupOffline = vi.fn();
    render(<AddModelDialog initialPath="offline" onClose={vi.fn()} onSetupOffline={onSetupOffline} />);
    fireEvent.click(screen.getByRole('button', { name: /Download and use/ }));
    expect(onSetupOffline).toHaveBeenCalled();
  });

  it('saves a cloud key and makes it the active provider', async () => {
    const onClose = vi.fn();
    render(<AddModelDialog initialPath="cloud" onClose={onClose} onSetupOffline={vi.fn()} />);
    const save = screen.getByRole('button', { name: /Save and use/ });
    expect(save).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: /Get a key/ }));
    expect(tauri.openExternal).toHaveBeenCalledWith('https://console.anthropic.com/settings/keys');
    fireEvent.change(screen.getByPlaceholderText('Paste your API key'), { target: { value: ' sk-test ' } });
    fireEvent.click(save);
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(tauri.saveProvider).toHaveBeenCalledWith(expect.objectContaining({ name: 'anthropic', api_key: 'sk-test' }));
    expect(useSettingsStore.getState().providers[0]).toMatchObject({ name: 'anthropic', isActive: true });
  });
});
