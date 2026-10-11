import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { DictationButton } from './DictationButton';
import { useDictationStore } from '../../stores/dictation';

const actions = {
  refresh: vi.fn().mockResolvedValue(undefined),
  download: vi.fn().mockResolvedValue(undefined),
  startRecording: vi.fn().mockResolvedValue(undefined),
  stopAndTranscribe: vi.fn().mockResolvedValue('hello there'),
  cancelRecording: vi.fn(),
  clearError: vi.fn(),
};

const option = { id: 'whisper-tiny-en-q8', name: 'Whisper tiny (English)', description: '', size_bytes: 1, size_label: '~42 MB' };

function setStore(partial: Partial<ReturnType<typeof useDictationStore.getState>>) {
  useDictationStore.setState({
    status: null,
    options: [option],
    phase: 'idle',
    isDownloading: false,
    progress: null,
    error: null,
    ...actions,
    ...partial,
  });
}

describe('DictationButton', () => {
  beforeEach(() => vi.clearAllMocks());

  it('offers the model download when none is installed', () => {
    setStore({ status: { downloaded: false, model_id: null, loaded: false } });
    render(<DictationButton onTranscript={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up voice input' }));
    expect(screen.getByText(/Whisper tiny \(English\) \(~42 MB\)/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Download/ }));
    expect(actions.download).toHaveBeenCalled();
    expect(actions.startRecording).not.toHaveBeenCalled();
  });

  it('starts recording when the model is installed', () => {
    setStore({ status: { downloaded: true, model_id: 'whisper-tiny-en-q8', loaded: false } });
    render(<DictationButton onTranscript={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Voice input (offline)' }));
    expect(actions.startRecording).toHaveBeenCalled();
  });

  it('stops recording and hands over the transcript', async () => {
    const onTranscript = vi.fn();
    setStore({ status: { downloaded: true, model_id: 'whisper-tiny-en-q8', loaded: true }, phase: 'recording' });
    render(<DictationButton onTranscript={onTranscript} />);
    fireEvent.click(screen.getByRole('button', { name: 'Stop and transcribe' }));
    await waitFor(() => expect(onTranscript).toHaveBeenCalledWith('hello there'));
  });

  it('is disabled while transcribing', () => {
    setStore({ status: { downloaded: true, model_id: 'x', loaded: true }, phase: 'transcribing' });
    render(<DictationButton onTranscript={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Transcribing…' })).toBeDisabled();
  });
});
