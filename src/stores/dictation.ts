import { create } from 'zustand';
import {
  checkDictationModel,
  deleteDictationModel,
  downloadDictationModel,
  getDictationOptions,
  transcribeAudio,
  type DictationModelOption,
  type DictationStatus,
  type DownloadProgress,
} from '../lib/tauri';
import { startMicRecording, type MicRecording } from '../lib/audio';

export type DictationPhase = 'idle' | 'recording' | 'transcribing';

interface DictationState {
  status: DictationStatus | null;
  options: DictationModelOption[];
  phase: DictationPhase;
  isDownloading: boolean;
  progress: DownloadProgress | null;
  error: string | null;

  /** Load model status and the downloadable options. */
  refresh: () => Promise<void>;
  /** Download a dictation model (defaults to the recommended one). */
  download: (modelId?: string) => Promise<void>;
  /** Delete downloaded dictation models. */
  remove: () => Promise<void>;
  /** Start recording from the microphone. */
  startRecording: () => Promise<void>;
  /** Stop recording and transcribe. Resolves to the text ('' on failure). */
  stopAndTranscribe: () => Promise<string>;
  /** Stop recording and discard the audio. */
  cancelRecording: () => void;
  clearError: () => void;
}

// The live recording isn't serialisable state, so it lives outside the store.
let recording: MicRecording | null = null;

export const useDictationStore = create<DictationState>((set, get) => ({
  status: null,
  options: [],
  phase: 'idle',
  isDownloading: false,
  progress: null,
  error: null,

  refresh: async () => {
    try {
      const [status, options] = await Promise.all([checkDictationModel(), getDictationOptions()]);
      set({ status, options: options ?? [] });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  download: async (modelId) => {
    if (get().isDownloading) return;
    const id = modelId ?? get().options[0]?.id;
    if (!id) {
      set({ error: 'No dictation model available.' });
      return;
    }
    set({ isDownloading: true, progress: null, error: null });
    try {
      await downloadDictationModel(id, (p) => {
        set({ progress: p });
        if (p.error) set({ error: p.error });
      });
      const status = await checkDictationModel();
      set({ status, isDownloading: false });
    } catch (err) {
      set({ error: String(err), isDownloading: false });
    }
  },

  remove: async () => {
    try {
      await deleteDictationModel();
      set({ status: await checkDictationModel(), progress: null });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  startRecording: async () => {
    if (get().phase !== 'idle') return;
    set({ error: null });
    try {
      recording = await startMicRecording();
      set({ phase: 'recording' });
    } catch (err) {
      recording = null;
      set({ error: `Microphone unavailable: ${String(err)}`, phase: 'idle' });
    }
  },

  stopAndTranscribe: async () => {
    const current = recording;
    if (get().phase !== 'recording' || !current) return '';
    recording = null;
    set({ phase: 'transcribing' });
    try {
      const pcm = await current.stop();
      const text = (await transcribeAudio(pcm)).trim();
      set({ phase: 'idle' });
      return text;
    } catch (err) {
      set({ phase: 'idle', error: String(err) });
      return '';
    }
  },

  cancelRecording: () => {
    recording?.cancel();
    recording = null;
    set({ phase: 'idle' });
  },

  clearError: () => set({ error: null }),
}));
