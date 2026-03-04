import { useState, useCallback, useEffect } from 'react';
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
} from 'lucide-react';
import clsx from 'clsx';
import { useSettingsStore, type ProviderConfig } from '../../stores/settings';
import { useThemeStore } from '../../stores/theme';
import styles from './SettingsModal.module.css';

interface SettingsModalProps {
  onClose: () => void;
}

type ProviderPreset = 'ollama' | 'groq' | 'google' | 'openrouter';

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
    model: 'gemini-2.0-flash',
  },
  openrouter: {
    name: 'openrouter',
    apiKey: '',
    baseUrl: 'https://openrouter.ai/api/v1',
    model: 'anthropic/claude-sonnet-4',
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

  // Sync from store if it changes externally
  useEffect(() => {
    setDraftProviders(savedProviders.map((p) => ({ ...p })));
  }, [savedProviders]);

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
      prev.map((p) => (p.id === id ? { ...p, ...updates } : p))
    );
  };

  const removeDraftProvider = (id: string) => {
    setDraftProviders((prev) => prev.filter((p) => p.id !== id));
  };

  const setDraftActive = (id: string) => {
    setDraftProviders((prev) =>
      prev.map((p) => ({ ...p, isActive: p.id === id }))
    );
  };

  const handlePresetChange = (id: string, preset: ProviderPreset) => {
    const presetData = PROVIDER_PRESETS[preset];
    updateDraftProvider(id, {
      name: presetData.name,
      baseUrl: presetData.baseUrl,
      model: presetData.model,
      apiKey: preset === 'ollama' ? '' : draftProviders.find((p) => p.id === id)?.apiKey ?? '',
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
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div
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
                      <label className={styles.fieldLabel}>Provider Type</label>
                      <select
                        className={styles.fieldSelect}
                        value={provider.name}
                        onChange={(e) =>
                          handlePresetChange(provider.id, e.target.value as ProviderPreset)
                        }
                      >
                        <option value="ollama">Ollama (Local)</option>
                        <option value="groq">Groq</option>
                        <option value="google">Google AI Studio</option>
                        <option value="openrouter">OpenRouter</option>
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
                      <input
                        type="text"
                        className={styles.fieldInput}
                        value={provider.model}
                        onChange={(e) =>
                          updateDraftProvider(provider.id, { model: e.target.value })
                        }
                        placeholder="Model name"
                      />
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
                          title={provider.isActive ? 'Active' : 'Set as active'}
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
                />
                <span className={styles.sliderValue}>{localSteps}</span>
              </div>
            </div>
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
    </div>
  );
}
