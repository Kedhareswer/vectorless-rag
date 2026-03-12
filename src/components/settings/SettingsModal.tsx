import { useState, useCallback, useEffect, useRef } from 'react';
import {
  X,
  Plus,
  Trash2,
  Sun,
  Moon,
  Monitor,
  AlertCircle,
  Server,
  Palette,
  SlidersHorizontal,
  Cpu,
  TriangleAlert,
} from 'lucide-react';
import clsx from 'clsx';
import { useSettingsStore, type ProviderConfig } from '../../stores/settings';
import { useThemeStore } from '../../stores/theme';
import { ModelDownloadDialog } from '../common/ModelDownloadDialog';
import { clearAppData } from '../../lib/tauri';
import styles from './SettingsModal.module.css';

interface SettingsModalProps {
  onClose: () => void;
}

type ProviderPreset =
  | 'ollama'
  | 'groq'
  | 'google'
  | 'openrouter'
  | 'agentrouter'
  | 'anthropic'
  | 'openai'
  | 'deepseek'
  | 'xai'
  | 'qwen'
  | 'openai-compat';

/** Models that have a fixed dropdown (provider -> model list) */
const PROVIDER_MODELS: Partial<Record<ProviderPreset, string[]>> = {
  google: [
    'gemini-2.5-flash',
    'gemini-2.5-pro',
    'gemini-2.0-flash',
    'gemini-2.0-pro',
  ],
  anthropic: [
    'claude-haiku-4-5-20251001',
    'claude-sonnet-4-6',
    'claude-opus-4-6',
  ],
  openai: [
    'gpt-4o-mini',
    'gpt-4o',
    'gpt-4.1-mini',
    'gpt-4.1',
    'o3-mini',
    'o4-mini',
  ],
  deepseek: [
    'deepseek-chat',
    'deepseek-reasoner',
  ],
  xai: [
    'grok-3-mini',
    'grok-3',
    'grok-4-fast-non-reasoning',
    'grok-4-0709',
  ],
  qwen: [
    'qwen-turbo',
    'qwen-plus',
    'qwen-max',
    'qwq-plus',
    'qwen3-coder-plus',
  ],
};

const PROVIDER_PRESETS: Record<ProviderPreset, Omit<ProviderConfig, 'id' | 'isActive'>> = {
  ollama: {
    name: 'ollama',
    apiKey: '',
    baseUrl: 'http://localhost:11434',
    model: 'llama3.2',
  },
  groq: {
    name: 'groq',
    apiKey: '',
    baseUrl: 'https://api.groq.com/openai/v1',
    model: 'llama-3.3-70b-versatile',
  },
  google: {
    name: 'google',
    apiKey: '',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta',
    model: 'gemini-2.5-flash',
  },
  openrouter: {
    name: 'openrouter',
    apiKey: '',
    baseUrl: 'https://openrouter.ai/api/v1',
    model: 'anthropic/claude-sonnet-4-6',
  },
  agentrouter: {
    name: 'agentrouter',
    apiKey: '',
    baseUrl: 'https://agentrouter.org/v1',
    model: 'claude-sonnet-4-5-20250514',
  },
  anthropic: {
    name: 'anthropic',
    apiKey: '',
    baseUrl: 'https://api.anthropic.com/v1',
    model: 'claude-haiku-4-5-20251001',
  },
  openai: {
    name: 'openai',
    apiKey: '',
    baseUrl: 'https://api.openai.com/v1',
    model: 'gpt-4o-mini',
  },
  deepseek: {
    name: 'deepseek',
    apiKey: '',
    baseUrl: 'https://api.deepseek.com/v1',
    model: 'deepseek-chat',
  },
  xai: {
    name: 'xai',
    apiKey: '',
    baseUrl: 'https://api.x.ai/v1',
    model: 'grok-3-mini',
  },
  qwen: {
    name: 'qwen',
    apiKey: '',
    baseUrl: 'https://dashscope-intl.aliyuncs.com/compatible-mode/v1',
    model: 'qwen-turbo',
  },
  'openai-compat': {
    name: 'openai-compat',
    apiKey: '',
    baseUrl: '',
    model: '',
  },
};

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

export function SettingsModal({ onClose }: SettingsModalProps) {
  const {
    providers: savedProviders,
    maxExplorationSteps,
    error,
    setMaxSteps,
    saveProviderToBackend,
    deleteProviderFromBackend,
    setActiveProvider,
    setProviders,
    clearError,
  } = useSettingsStore();

  const { theme, setTheme } = useThemeStore();

  // Local draft state for editing
  const [draftProviders, setDraftProviders] = useState<ProviderConfig[]>(() =>
    savedProviders.map((p) => ({ ...p }))
  );
  const [isSaving, setIsSaving] = useState(false);
  const [localSteps, setLocalSteps] = useState(maxExplorationSteps);
  const [showModelDialog, setShowModelDialog] = useState(false);
  const [isClearing, setIsClearing] = useState(false);
  const [clearConfirm, setClearConfirm] = useState(false);

  // Cache API keys per provider type so switching types remembers previous keys
  const [keyCache, setKeyCache] = useState<Record<string, string>>(() => {
    const cache: Record<string, string> = {};
    for (const p of savedProviders) {
      if (p.apiKey) {
        cache[p.name] = p.apiKey;
      }
    }
    return cache;
  });

  const overlayRef = useRef<HTMLDivElement>(null);

  // Sync from store if it changes externally
  useEffect(() => {
    setDraftProviders(savedProviders.map((p) => ({ ...p })));
  }, [savedProviders]);

  // Focus dialog on mount for keyboard accessibility
  useEffect(() => {
    overlayRef.current?.focus();
  }, []);

  const handleOverlayClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (e.target === e.currentTarget) {
        onClose();
      }
    },
    [onClose]
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    },
    [onClose]
  );

  const addProvider = (preset: ProviderPreset) => {
    const presetData = PROVIDER_PRESETS[preset];
    const newProvider: ProviderConfig = {
      ...presetData,
      id: generateId(),
      isActive: draftProviders.length === 0,
    };
    setDraftProviders((prev) => [...prev, newProvider]);
  };

  const updateDraftProvider = (id: string, updates: Partial<ProviderConfig>) => {
    setDraftProviders((prev) =>
      prev.map((p) => {
        if (p.id !== id) return p;
        const updated = { ...p, ...updates };
        // Keep key cache in sync when user types an API key
        if (updates.apiKey !== undefined && updated.name !== 'ollama') {
          setKeyCache((c) => ({ ...c, [updated.name]: updates.apiKey! }));
        }
        return updated;
      })
    );
  };

  const removeDraftProvider = (id: string) => {
    setDraftProviders((prev) => {
      const remaining = prev.filter((p) => p.id !== id);
      // If we removed the active provider, promote the first remaining one
      if (remaining.length > 0 && !remaining.some((p) => p.isActive)) {
        remaining[0] = { ...remaining[0], isActive: true };
      }
      return remaining;
    });
  };

  const setDraftActive = (id: string) => {
    setDraftProviders((prev) =>
      prev.map((p) => ({ ...p, isActive: p.id === id }))
    );
  };

  const handlePresetChange = (id: string, preset: ProviderPreset) => {
    const presetData = PROVIDER_PRESETS[preset];
    const currentProvider = draftProviders.find((p) => p.id === id);

    // Save current provider's key to cache before switching
    if (currentProvider && currentProvider.apiKey && currentProvider.name !== 'ollama') {
      setKeyCache((prev) => ({ ...prev, [currentProvider.name]: currentProvider.apiKey }));
    }

    // Restore cached key for the new provider type, or empty
    const cachedKey = preset === 'ollama' ? '' : (keyCache[preset] ?? '');

    updateDraftProvider(id, {
      name: presetData.name,
      baseUrl: presetData.baseUrl,
      model: presetData.model,
      apiKey: cachedKey,
    });
  };

  const handleSave = async () => {
    setIsSaving(true);
    clearError();
    try {
      // Save all draft providers to backend
      for (const provider of draftProviders) {
        await saveProviderToBackend(provider);
      }

      // Delete providers that were removed from drafts
      const draftIds = new Set(draftProviders.map((p) => p.id));
      for (const existing of savedProviders) {
        if (!draftIds.has(existing.id)) {
          await deleteProviderFromBackend(existing.id);
        }
      }

      // Update local store with draft state
      setProviders(draftProviders);

      const activeProvider = draftProviders.find((p) => p.isActive);
      if (activeProvider) {
        setActiveProvider(activeProvider.id);
      }

      setMaxSteps(localSteps);
      onClose();
    } catch (err) {
      console.warn('Failed to save settings:', err);
      // Don't close — let the user see the error banner
    } finally {
      setIsSaving(false);
    }
  };

  const handleClearData = async () => {
    if (!clearConfirm) {
      setClearConfirm(true);
      return;
    }
    setIsClearing(true);
    try {
      await clearAppData();
      // Reload the window — backend DB is gone, frontend state must reset
      window.location.reload();
    } catch (err) {
      console.error('Failed to clear app data:', err);
    } finally {
      setIsClearing(false);
      setClearConfirm(false);
    }
  };

  return (
    <div
      ref={overlayRef}
      className={styles.overlay}
      onClick={handleOverlayClick}
      onKeyDown={handleKeyDown}
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      tabIndex={-1}
    >
      <div className={styles.modal}>
        {/* Header */}
        <div className={styles.header}>
          <span className={styles.title}>Settings</span>
          <button
            className={styles.closeButton}
            onClick={onClose}
            title="Close"
            type="button"
          >
            <X size={16} />
          </button>
        </div>

        {/* Body */}
        <div className={styles.body}>
          {error && (
            <div className={styles.errorBanner}>
              <AlertCircle size={14} />
              <span>{error}</span>
            </div>
          )}

          {/* Providers Section */}
          <div className={styles.section}>
            <div className={styles.sectionTitle}>
              <Server size={14} className={styles.sectionIcon} />
              <span>LLM Providers</span>
            </div>

            <div className={styles.providerCards}>
              {draftProviders.map((provider) => (
                <div key={provider.id} className={styles.providerCard}>
                  <div className={styles.providerCardHeader}>
                    <span className={styles.providerCardName}>
                      {provider.name.charAt(0).toUpperCase() + provider.name.slice(1)}
                    </span>
                    <div className={styles.providerCardActions}>
                      <button
                        className={styles.deleteButton}
                        onClick={() => removeDraftProvider(provider.id)}
                        title="Remove provider"
                        type="button"
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>

                  <div className={styles.fieldGrid}>
                    <div className={clsx(styles.field, styles.fieldFull)}>
                      <label className={styles.fieldLabel} htmlFor={`provider-type-${provider.id}`}>Provider Type</label>
                      <select
                        id={`provider-type-${provider.id}`}
                        className={styles.fieldSelect}
                        value={provider.name}
                        title="Select provider type"
                        onChange={(e) =>
                          handlePresetChange(provider.id, e.target.value as ProviderPreset)
                        }
                      >
                        <optgroup label="Cloud — Direct">
                          <option value="anthropic">Anthropic (Claude)</option>
                          <option value="openai">OpenAI</option>
                          <option value="google">Google AI Studio</option>
                          <option value="deepseek">DeepSeek</option>
                          <option value="xai">xAI (Grok)</option>
                          <option value="qwen">Alibaba Qwen</option>
                          <option value="groq">Groq</option>
                        </optgroup>
                        <optgroup label="Routers / Aggregators">
                          <option value="openrouter">OpenRouter</option>
                          <option value="agentrouter">AgentRouter</option>
                        </optgroup>
                        <optgroup label="Local">
                          <option value="ollama">Ollama (Local)</option>
                        </optgroup>
                        <optgroup label="Custom">
                          <option value="openai-compat">OpenAI Compatible</option>
                        </optgroup>
                      </select>
                    </div>

                    {provider.name !== 'ollama' && (
                      <div className={clsx(styles.field, styles.fieldFull)}>
                        <label className={styles.fieldLabel}>API Key</label>
                        <input
                          type="password"
                          className={styles.fieldInput}
                          value={provider.apiKey}
                          onChange={(e) =>
                            updateDraftProvider(provider.id, { apiKey: e.target.value })
                          }
                          placeholder="Enter API key..."
                          autoComplete="off"
                        />
                      </div>
                    )}

                    <div className={styles.field}>
                      <label className={styles.fieldLabel}>Base URL</label>
                      <input
                        type="text"
                        className={styles.fieldInput}
                        value={provider.baseUrl}
                        onChange={(e) =>
                          updateDraftProvider(provider.id, { baseUrl: e.target.value })
                        }
                        placeholder="https://..."
                      />
                    </div>

                    <div className={styles.field}>
                      <label className={styles.fieldLabel}>Model</label>
                      {PROVIDER_MODELS[provider.name as ProviderPreset] ? (
                        <select
                          className={styles.fieldSelect}
                          value={provider.model}
                          onChange={(e) =>
                            updateDraftProvider(provider.id, { model: e.target.value })
                          }
                          title="Select model"
                        >
                          {PROVIDER_MODELS[provider.name as ProviderPreset]!.map((m) => (
                            <option key={m} value={m}>{m}</option>
                          ))}
                        </select>
                      ) : (
                        <input
                          type="text"
                          className={styles.fieldInput}
                          value={provider.model}
                          onChange={(e) =>
                            updateDraftProvider(provider.id, { model: e.target.value })
                          }
                          placeholder="Model name"
                        />
                      )}
                    </div>

                    <div className={clsx(styles.field, styles.fieldFull)}>
                      <div className={styles.toggleRow}>
                        <span className={styles.toggleLabel}>Active Provider</span>
                        <button
                          type="button"
                          className={clsx(
                            styles.toggle,
                            provider.isActive && styles.toggleActive
                          )}
                          onClick={() => setDraftActive(provider.id)}
                          aria-label={provider.isActive ? 'Active provider' : 'Set as active provider'}
                          aria-pressed={provider.isActive}
                        />
                      </div>
                    </div>
                  </div>
                </div>
              ))}

              <button
                className={styles.addProviderButton}
                onClick={() => addProvider('ollama')}
                type="button"
              >
                <Plus size={16} />
                <span>Add Provider</span>
              </button>
            </div>
          </div>

          {/* Theme Section */}
          <div className={styles.section}>
            <div className={styles.sectionTitle}>
              <Palette size={14} className={styles.sectionIcon} />
              <span>Appearance</span>
            </div>

            <div className={styles.themeGroup}>
              <button
                type="button"
                className={clsx(
                  styles.themeOption,
                  theme === 'light' && styles.themeOptionActive
                )}
                onClick={() => setTheme('light')}
              >
                <Sun size={14} />
                <span>Light</span>
              </button>
              <button
                type="button"
                className={clsx(
                  styles.themeOption,
                  theme === 'dark' && styles.themeOptionActive
                )}
                onClick={() => setTheme('dark')}
              >
                <Moon size={14} />
                <span>Dark</span>
              </button>
              <button
                type="button"
                className={clsx(
                  styles.themeOption,
                  theme === 'system' && styles.themeOptionActive
                )}
                onClick={() => setTheme('system')}
              >
                <Monitor size={14} />
                <span>System</span>
              </button>
            </div>
          </div>

          {/* Local Model Section */}
          <div className={styles.section}>
            <div className={styles.sectionTitle}>
              <Cpu size={14} className={styles.sectionIcon} />
              <span>Document Enrichment Model</span>
            </div>
            <p className={styles.sectionDesc}>
              A small local AI model that generates summaries and metadata for document sections.
            </p>
            <button
              type="button"
              className={styles.addProviderButton}
              onClick={() => setShowModelDialog(true)}
            >
              <Cpu size={16} />
              <span>Manage Local Model</span>
            </button>
          </div>

          {/* Exploration Section */}
          <div className={styles.section}>
            <div className={styles.sectionTitle}>
              <SlidersHorizontal size={14} className={styles.sectionIcon} />
              <span>Exploration</span>
            </div>

            <div className={styles.field}>
              <label className={styles.fieldLabel}>
                Max Exploration Steps
              </label>
              <div className={styles.sliderRow}>
                <input
                  type="range"
                  className={styles.slider}
                  min={1}
                  max={20}
                  value={localSteps}
                  onChange={(e) => setLocalSteps(Number(e.target.value))}
                  aria-label="Max exploration steps"
                  title="Max exploration steps"
                />
                <span className={styles.sliderValue}>{localSteps}</span>
              </div>
            </div>
          </div>

          {/* Danger Zone */}
          <div className={styles.section}>
            <div className={styles.sectionTitle}>
              <TriangleAlert size={14} className={styles.sectionIcon} style={{ color: 'var(--accent)' }} />
              <span>Danger Zone</span>
            </div>
            <p className={styles.sectionDesc}>
              Deletes all conversations, documents, providers, and the local model.
              The app returns to a clean first-run state. This cannot be undone.
            </p>
            <button
              type="button"
              className={styles.dangerButton}
              onClick={handleClearData}
              disabled={isClearing}
            >
              <Trash2 size={14} />
              <span>
                {isClearing
                  ? 'Clearing...'
                  : clearConfirm
                  ? 'Click again to confirm — this is irreversible'
                  : 'Clear All App Data'}
              </span>
            </button>
          </div>
        </div>

        {/* Footer */}
        <div className={styles.footer}>
          <button
            className={styles.cancelButton}
            onClick={onClose}
            type="button"
          >
            Cancel
          </button>
          <button
            className={styles.saveButton}
            onClick={handleSave}
            disabled={isSaving}
            type="button"
          >
            {isSaving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </div>

      {showModelDialog && (
        <ModelDownloadDialog onClose={() => setShowModelDialog(false)} />
      )}
    </div>
  );
}
