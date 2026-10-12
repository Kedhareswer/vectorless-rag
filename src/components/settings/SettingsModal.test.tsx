import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { SettingsModal } from './SettingsModal';
import { useSettingsStore } from '../../stores/settings';
import * as tauri from '../../lib/tauri';

vi.mock('../../lib/tauri', () => ({
  getProviders: vi.fn().mockResolvedValue([]),
  saveProvider: vi.fn().mockResolvedValue(undefined),
  deleteProvider: vi.fn().mockResolvedValue(undefined),
  clearAppData: vi.fn().mockResolvedValue(undefined),
  checkLocalModel: vi.fn().mockResolvedValue({ downloaded: false, model_id: null, model_path: null, size_bytes: null, tokenizer_ready: false }),
  getModelOptions: vi.fn().mockResolvedValue([]),
  checkDictationModel: vi.fn().mockResolvedValue({ downloaded: false, model_id: null, loaded: false }),
  getDictationOptions: vi.fn().mockResolvedValue([{ id: 'whisper-tiny-en-q8', name: 'Whisper tiny (English)', description: '', size_bytes: 1, size_label: '~42 MB' }]),
}));

describe('SettingsModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSettingsStore.setState({
      providers: [{ id: 'a', name: 'anthropic', apiKey: 'k', baseUrl: '', model: 'claude-custom-preview', isActive: true }],
      activeProviderId: 'a',
      error: null,
    });
  });

  it('keeps a model ID that is not in the suggestion list', () => {
    render(<SettingsModal onClose={vi.fn()} />);
    expect(screen.getByPlaceholderText('Model ID')).toHaveValue('claude-custom-preview');
  });

  it('only enables Save after a change, then saves and closes', async () => {
    const onClose = vi.fn();
    render(<SettingsModal onClose={onClose} />);
    const save = screen.getByRole('button', { name: 'Save' });
    expect(save).toBeDisabled();

    fireEvent.change(screen.getByPlaceholderText('Model ID'), { target: { value: 'claude-opus-5-5' } });
    expect(screen.getByText('Unsaved provider changes')).toBeInTheDocument();
    fireEvent.click(save);

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(tauri.saveProvider).toHaveBeenCalledWith(expect.objectContaining({ model: 'claude-opus-5-5' }));
  });

  it('switches sections, including Local AI with dictation', async () => {
    render(<SettingsModal onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Local AI' }));
    expect(screen.getByRole('heading', { name: 'Local AI' })).toBeInTheDocument();
    expect(await screen.findByText('Whisper tiny (English)')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument();
  });

  it('opens on a requested section', () => {
    render(<SettingsModal onClose={vi.fn()} initialSection="appearance" />);
    expect(screen.getByRole('radiogroup', { name: 'Theme' })).toBeInTheDocument();
  });
});
