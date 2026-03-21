import { create } from 'zustand';
import {
  checkLocalModel,
  downloadLocalModel,
  deleteLocalModel,
  type LocalModelStatus,
  type DownloadProgress,
} from '../lib/tauri';

interface LocalModelState {
  /** Whether the model + binary are downloaded */
  status: LocalModelStatus | null;
  /** True while a download is in progress (survives dialog close) */
  isDownloading: boolean;
  /** Current download progress */
  progress: DownloadProgress | null;
  /** Download or status error */
  error: string | null;

  /** Load current model status from backend */
  refreshStatus: () => Promise<void>;
  /** Start downloading a model. Runs in background — caller can unmount safely. */
  startDownload: (modelId: string) => Promise<void>;
  /** Delete model + clear settings */
  removeModel: () => Promise<void>;
  clearError: () => void;
}

export const useLocalModelStore = create<LocalModelState>((set, get) => ({
  status: null,
  isDownloading: false,
  progress: null,
  error: null,

  refreshStatus: async () => {
    try {
      const status = await checkLocalModel();
      set({ status });
    } catch {
      set({ status: null });
    }
  },

  startDownload: async (modelId: string) => {
    if (get().isDownloading) return; // prevent double-download

    set({ isDownloading: true, error: null, progress: null });

    try {
      await downloadLocalModel(modelId, (p: DownloadProgress) => {
        set({ progress: p });
        if (p.error) {
          set({ error: p.error, isDownloading: false });
        }
      });
      // Download completed — refresh status
      const status = await checkLocalModel();
      set({ status, isDownloading: false });
    } catch (err) {
      set({ error: String(err), isDownloading: false });
    }
  },

  removeModel: async () => {
    try {
      await deleteLocalModel();
      set({
        status: {
          downloaded: false,
          model_id: null,
          model_path: null,
          size_bytes: null,
          tokenizer_ready: false,
        },
        progress: null,
      });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  clearError: () => set({ error: null }),
}));
