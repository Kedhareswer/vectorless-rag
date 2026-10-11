import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useDictationStore } from './dictation';
import * as tauri from '../lib/tauri';
import * as audio from '../lib/audio';

vi.mock('../lib/tauri', () => ({
  checkDictationModel: vi.fn(),
  getDictationOptions: vi.fn(),
  downloadDictationModel: vi.fn(),
  transcribeAudio: vi.fn(),
}));

vi.mock('../lib/audio', () => ({
  startMicRecording: vi.fn(),
}));

const tinyOption = { id: 'whisper-tiny-en-q8', name: 'Whisper tiny', description: '', size_bytes: 1, size_label: '~42 MB' };

describe('DictationStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useDictationStore.setState({
      status: null,
      options: [],
      phase: 'idle',
      isDownloading: false,
      progress: null,
      error: null,
    });
  });

  it('refresh loads status and options', async () => {
    vi.mocked(tauri.checkDictationModel).mockResolvedValue({ downloaded: false, model_id: null, loaded: false });
    vi.mocked(tauri.getDictationOptions).mockResolvedValue([tinyOption]);
    await useDictationStore.getState().refresh();
    const s = useDictationStore.getState();
    expect(s.status?.downloaded).toBe(false);
    expect(s.options[0].id).toBe('whisper-tiny-en-q8');
  });

  it('download defaults to the first (recommended) option and refreshes status', async () => {
    useDictationStore.setState({ options: [tinyOption] });
    vi.mocked(tauri.downloadDictationModel).mockResolvedValue(undefined);
    vi.mocked(tauri.checkDictationModel).mockResolvedValue({ downloaded: true, model_id: 'whisper-tiny-en-q8', loaded: false });
    await useDictationStore.getState().download();
    expect(tauri.downloadDictationModel).toHaveBeenCalledWith('whisper-tiny-en-q8', expect.any(Function));
    expect(useDictationStore.getState().status?.downloaded).toBe(true);
    expect(useDictationStore.getState().isDownloading).toBe(false);
  });

  it('records, then transcribes the captured audio', async () => {
    const pcm = new Float32Array([0.1, 0.2]);
    const stop = vi.fn().mockResolvedValue(pcm);
    vi.mocked(audio.startMicRecording).mockResolvedValue({ stop, cancel: vi.fn() });
    vi.mocked(tauri.transcribeAudio).mockResolvedValue('  hello world ');

    await useDictationStore.getState().startRecording();
    expect(useDictationStore.getState().phase).toBe('recording');

    const text = await useDictationStore.getState().stopAndTranscribe();
    expect(stop).toHaveBeenCalled();
    expect(tauri.transcribeAudio).toHaveBeenCalledWith(pcm);
    expect(text).toBe('hello world');
    expect(useDictationStore.getState().phase).toBe('idle');
  });

  it('reports microphone errors without leaving the recording state', async () => {
    vi.mocked(audio.startMicRecording).mockRejectedValue(new Error('denied'));
    await useDictationStore.getState().startRecording();
    const s = useDictationStore.getState();
    expect(s.phase).toBe('idle');
    expect(s.error).toContain('denied');
  });

  it('surfaces transcription errors and returns empty text', async () => {
    vi.mocked(audio.startMicRecording).mockResolvedValue({ stop: vi.fn().mockResolvedValue(new Float32Array()), cancel: vi.fn() });
    vi.mocked(tauri.transcribeAudio).mockRejectedValue('model missing');
    await useDictationStore.getState().startRecording();
    expect(await useDictationStore.getState().stopAndTranscribe()).toBe('');
    expect(useDictationStore.getState().error).toContain('model missing');
  });

  it('cancelRecording discards audio', async () => {
    const cancel = vi.fn();
    vi.mocked(audio.startMicRecording).mockResolvedValue({ stop: vi.fn(), cancel });
    await useDictationStore.getState().startRecording();
    useDictationStore.getState().cancelRecording();
    expect(cancel).toHaveBeenCalled();
    expect(useDictationStore.getState().phase).toBe('idle');
    expect(tauri.transcribeAudio).not.toHaveBeenCalled();
  });
});
