import { create } from 'zustand';
import { detectLocalModels, type LocalServer } from '../lib/tauri';

interface LocalServersState {
  /** Running servers with their installed models; empty when none answer. */
  servers: LocalServer[];
  detecting: boolean;
  /** Probe Ollama / LM Studio on this machine. Safe to call repeatedly. */
  detect: () => Promise<void>;
}

export const useLocalServersStore = create<LocalServersState>((set, get) => ({
  servers: [],
  detecting: false,

  detect: async () => {
    if (get().detecting) return;
    set({ detecting: true });
    try {
      set({ servers: (await detectLocalModels()) ?? [] });
    } catch {
      set({ servers: [] });
    } finally {
      set({ detecting: false });
    }
  },
}));

/** First server that has at least one model installed. */
export function firstUsableServer(servers: LocalServer[]): LocalServer | undefined {
  return servers.find((s) => s.models.length > 0);
}
