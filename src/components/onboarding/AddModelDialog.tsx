import { useEffect, useRef, useState } from 'react';
import { Check, Cloud, Copy, Cpu, ExternalLink, HardDrive, LoaderCircle, RefreshCw, X } from 'lucide-react';
import clsx from 'clsx';
import { CLOUD_KEY_PAGES, MODEL_GUIDES, MODEL_PATH_ORDER, type ModelPath } from '../../lib/modelGuides';
import { PROVIDER_MODELS, PROVIDER_PRESETS, type ProviderPreset } from '../../lib/providerPresets';
import { openExternal } from '../../lib/tauri';
import { useLocalServersStore } from '../../stores/localServers';
import { useLocalModelStore } from '../../stores/localModel';
import { useSettingsStore } from '../../stores/settings';
import { GuideVideo } from './GuideVideo';
import styles from './Onboarding.module.css';

const PATH_ICONS: Record<ModelPath, typeof Cpu> = { installed: Cpu, offline: HardDrive, cloud: Cloud };
const PULL_COMMAND = 'ollama pull llama3.2:3b';

interface AddModelDialogProps {
  /** Option to show first; defaults to installed models when one is found. */
  initialPath: ModelPath | null;
  onClose: () => void;
  /** Select the built-in offline model and start its download if needed. */
  onSetupOffline: () => void;
}

/** "Add a model": the three options, each with a screen recording and steps, and the action itself. */
export function AddModelDialog({ initialPath, onClose, onSetupOffline }: AddModelDialogProps) {
  const { servers, detecting, detect } = useLocalServersStore();
  const offlineReady = useLocalModelStore((s) => s.status?.downloaded ?? false);
  const [path, setPath] = useState<ModelPath>(
    () => initialPath ?? (servers.some((s) => s.models.length > 0) ? 'installed' : 'offline'),
  );
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    dialogRef.current?.focus();
    void detect();
  }, [detect]);

  const guide = MODEL_GUIDES[path];

  return (
    <div
      className={styles.overlay}
      role="presentation"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        ref={dialogRef}
        className={clsx(styles.dialog, styles.addModel)}
        role="dialog"
        aria-modal="true"
        aria-labelledby="add-model-title"
        tabIndex={-1}
        onKeyDown={(e) => e.key === 'Escape' && onClose()}
      >
        <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
          <X size={16} />
        </button>
        <h2 id="add-model-title" className={styles.title}>Add a model</h2>
        <p className={styles.subtitle}>Pick how answers are written. You can add more and switch any time.</p>

        <div className={styles.addLayout}>
          <div className={styles.options} role="tablist" aria-label="Ways to add a model">
            {MODEL_PATH_ORDER.map((id) => {
              const g = MODEL_GUIDES[id];
              const Icon = PATH_ICONS[id];
              const found = id === 'installed' ? servers.filter((s) => s.models.length > 0) : [];
              return (
                <button
                  key={id}
                  type="button"
                  role="tab"
                  aria-selected={path === id}
                  className={clsx(styles.option, path === id && styles.optionActive)}
                  onClick={() => setPath(id)}
                >
                  <Icon size={16} className={styles.pathIcon} />
                  <span className={styles.optionText}>
                    <strong>{g.title}</strong>
                    <span>{g.summary}</span>
                    <span className={styles.facts}>
                      {g.facts.map((f) => <span key={f} className={styles.fact}>{f}</span>)}
                    </span>
                    {found.length > 0 && (
                      <span className={styles.found}>Found {found.map((s) => s.label).join(' and ')}</span>
                    )}
                  </span>
                </button>
              );
            })}
          </div>

          <div className={styles.guide} role="tabpanel" aria-label={guide.title}>
            <GuideVideo src={guide.video} label={guide.videoLabel} />
            <ol className={styles.steps}>
              {guide.steps.map((s) => <li key={s}>{s}</li>)}
            </ol>
            <div className={styles.action}>
              {path === 'installed' && (
                <InstalledAction detecting={detecting} onCheck={() => void detect()} onDone={onClose} />
              )}
              {path === 'offline' && (
                <button
                  type="button"
                  className={styles.primary}
                  onClick={() => {
                    onSetupOffline();
                    onClose();
                  }}
                >
                  <HardDrive size={14} />
                  {offlineReady ? 'Use the offline model' : 'Download and use (~700 MB)'}
                </button>
              )}
              {path === 'cloud' && <CloudAction onDone={onClose} />}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function InstalledAction({ detecting, onCheck, onDone }: { detecting: boolean; onCheck: () => void; onDone: () => void }) {
  const servers = useLocalServersStore((s) => s.servers);
  const selectServerModel = useSettingsStore((s) => s.selectServerModel);
  const [copied, setCopied] = useState(false);
  const models = servers.flatMap((server) => server.models.map((model) => ({ server, model })));

  if (models.length > 0) {
    return (
      <div className={styles.modelList}>
        <span className={styles.actionLabel}>Installed on this computer</span>
        {models.slice(0, 8).map(({ server, model }) => (
          <button
            key={`${server.provider}:${model}`}
            type="button"
            className={styles.modelButton}
            onClick={async () => {
              await selectServerModel(server.provider, model, server.base_url);
              if (!useSettingsStore.getState().error) onDone();
            }}
          >
            <Cpu size={13} />
            <span className={styles.modelName}>{model}</span>
            <span className={styles.modelServer}>{server.label}</span>
          </button>
        ))}
      </div>
    );
  }

  return (
    <div className={styles.modelList}>
      <span className={styles.actionLabel}>
        {detecting ? 'Looking for Ollama and LM Studio…' : 'No running Ollama or LM Studio found yet.'}
      </span>
      <div className={styles.command}>
        <code>{PULL_COMMAND}</code>
        <button
          type="button"
          className={styles.iconButton}
          aria-label="Copy command"
          onClick={() => {
            void navigator.clipboard?.writeText(PULL_COMMAND).then(() => setCopied(true));
          }}
        >
          {copied ? <Check size={13} /> : <Copy size={13} />}
        </button>
      </div>
      <div className={styles.row}>
        <button type="button" className={styles.secondary} onClick={() => void openExternal('https://ollama.com/download')}>
          Get Ollama <ExternalLink size={12} />
        </button>
        <button type="button" className={styles.secondary} onClick={() => void openExternal('https://lmstudio.ai')}>
          Get LM Studio <ExternalLink size={12} />
        </button>
        <button type="button" className={styles.primary} onClick={onCheck} disabled={detecting}>
          {detecting ? <LoaderCircle size={14} className={styles.spin} /> : <RefreshCw size={14} />}
          Check again
        </button>
      </div>
    </div>
  );
}

function CloudAction({ onDone }: { onDone: () => void }) {
  const saveProviderToBackend = useSettingsStore((s) => s.saveProviderToBackend);
  const activateProvider = useSettingsStore((s) => s.activateProvider);
  const [preset, setPreset] = useState(CLOUD_KEY_PAGES[0].preset as ProviderPreset);
  const [apiKey, setApiKey] = useState('');
  const [model, setModel] = useState(PROVIDER_PRESETS[preset].model);
  const [saving, setSaving] = useState(false);
  const page = CLOUD_KEY_PAGES.find((p) => p.preset === preset) ?? CLOUD_KEY_PAGES[0];

  const save = async () => {
    setSaving(true);
    const config = {
      ...PROVIDER_PRESETS[preset],
      id: `${preset}-${Date.now()}`,
      apiKey: apiKey.trim(),
      model: model.trim(),
      isActive: false,
    };
    await saveProviderToBackend(config);
    if (!useSettingsStore.getState().error) {
      await activateProvider(config.id);
      onDone();
    }
    setSaving(false);
  };

  return (
    <form
      className={styles.cloudForm}
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <label className={styles.field}>
        <span>Provider</span>
        <select
          value={preset}
          onChange={(e) => {
            const next = e.target.value as ProviderPreset;
            setPreset(next);
            setModel(PROVIDER_PRESETS[next].model);
          }}
        >
          {CLOUD_KEY_PAGES.map((p) => <option key={p.preset} value={p.preset}>{p.label}</option>)}
        </select>
      </label>
      <button type="button" className={clsx(styles.secondary, styles.keyLink)} onClick={() => void openExternal(page.url)}>
        Get a key <ExternalLink size={12} />
      </button>
      <label className={clsx(styles.field, styles.fieldWide)}>
        <span>API key</span>
        <input
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="Paste your API key"
          autoComplete="off"
        />
      </label>
      <label className={clsx(styles.field, styles.fieldWide)}>
        <span>Model</span>
        <input value={model} onChange={(e) => setModel(e.target.value)} list="add-model-suggestions" spellCheck={false} />
        <datalist id="add-model-suggestions">
          {(PROVIDER_MODELS[preset] ?? []).map((m) => <option key={m} value={m} />)}
        </datalist>
      </label>
      <button type="submit" className={clsx(styles.primary, styles.fieldWide)} disabled={saving || !apiKey.trim() || !model.trim()}>
        {saving ? <LoaderCircle size={14} className={styles.spin} /> : <Check size={14} />}
        Save and use
      </button>
    </form>
  );
}
