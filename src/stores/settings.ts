import { create } from 'zustand';
import {
  getProviders,
  saveProvider as saveProviderIPC,
  deleteProvider as deleteProviderIPC,
  type ProviderConfig as TauriProviderConfig,
} from '../lib/tauri';

export interface ProviderConfig {
  id: string;
  name: string;      // "ollama" | "groq" | "google" | "openrouter"
  apiKey: string;     // API key (empty for ollama)
  baseUrl: string;    // Provider base URL
  model: string;      // Model name
  isActive: boolean;  // Whether this is the active provider
}

// Convert from Tauri snake_case to frontend camelCase
function fromTauri(p: TauriProviderConfig): ProviderConfig {
  return {
    id: p.id,
    name: p.name,
    apiKey: p.api_key ?? '',
    baseUrl: p.base_url,
    model: p.model,
    isActive: p.is_active,
  };
}

// Convert from frontend camelCase to Tauri snake_case
function toTauri(p: ProviderConfig): TauriProviderConfig {
  return {
    id: p.id,
    name: p.name,
    api_key: p.apiKey || null,
    base_url: p.baseUrl,
    model: p.model,
    is_active: p.isActive,
  };
}

interface SettingsState {
  providers: ProviderConfig[];
  activeProviderId: string | null;
  maxExplorationSteps: number;
  isLoading: boolean;
  error: string | null;

  setProviders: (providers: ProviderConfig[]) => void;
  addProvider: (provider: ProviderConfig) => void;
  updateProvider: (id: string, updates: Partial<ProviderConfig>) => void;
  removeProvider: (id: string) => void;
  setActiveProvider: (id: string | null) => void;
  setMaxSteps: (steps: number) => void;
  loadProviders: () => Promise<void>;
  saveProviderToBackend: (provider: ProviderConfig) => Promise<void>;
  deleteProviderFromBackend: (id: string) => Promise<void>;
  clearError: () => void;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  providers: [],
  activeProviderId: null,
  maxExplorationSteps: 10,
  isLoading: false,
  error: null,

  setProviders: (providers: ProviderConfig[]) => {
    const active = providers.find((p) => p.isActive);
    set({ providers, activeProviderId: active?.id ?? null });
  },

  addProvider: (provider: ProviderConfig) => {
    set((state) => ({
      providers: [...state.providers, provider],
    }));
  },

  updateProvider: (id: string, updates: Partial<ProviderConfig>) => {
    set((state) => ({
      providers: state.providers.map((p) =>
        p.id === id ? { ...p, ...updates } : p
      ),
    }));
  },

  removeProvider: (id: string) => {
    set((state) => ({
      providers: state.providers.filter((p) => p.id !== id),
      activeProviderId: state.activeProviderId === id ? null : state.activeProviderId,
    }));
  },

  setActiveProvider: (id: string | null) => {
    set((state) => ({
      activeProviderId: id,
      providers: state.providers.map((p) => ({
        ...p,
        isActive: p.id === id,
      })),
    }));
  },

  setMaxSteps: (steps: number) => {
    set({ maxExplorationSteps: steps });
  },

  loadProviders: async () => {
    set({ isLoading: true, error: null });
    try {
      const tauriProviders = await getProviders();
      const providers = tauriProviders.map(fromTauri);
      const active = providers.find((p) => p.isActive);
      set({ providers, activeProviderId: active?.id ?? null, isLoading: false });
    } catch (err) {
      console.warn('Failed to load providers from backend:', err);
      set({ isLoading: false, error: String(err) });
    }
  },

  saveProviderToBackend: async (provider: ProviderConfig) => {
    set({ error: null });
    try {
      await saveProviderIPC(toTauri(provider));
      // Update local state
      const { providers } = get();
      const exists = providers.find((p) => p.id === provider.id);
      if (exists) {
        set((state) => ({
          providers: state.providers.map((p) =>
            p.id === provider.id ? provider : p
          ),
        }));
      } else {
        set((state) => ({
          providers: [...state.providers, provider],
        }));
      }
      if (provider.isActive) {
        set({ activeProviderId: provider.id });
      }
    } catch (err) {
      console.warn('Failed to save provider to backend:', err);
      set({ error: String(err) });
    }
  },

  deleteProviderFromBackend: async (id: string) => {
    set({ error: null });
    try {
      await deleteProviderIPC(id);
      set((state) => ({
        providers: state.providers.filter((p) => p.id !== id),
        activeProviderId: state.activeProviderId === id ? null : state.activeProviderId,
      }));
    } catch (err) {
      console.warn('Failed to delete provider from backend:', err);
      set({ error: String(err) });
    }
  },

  clearError: () => {
    set({ error: null });
  },
}));
