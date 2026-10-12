import { useEffect, useRef, useState } from 'react';
import { Check, ChevronDown, Server, Settings } from 'lucide-react';
import clsx from 'clsx';
import { useSettingsStore } from '../../stores/settings';
import { providerDisplayName, providerModelLabel } from '../../lib/providers';
import { useLocalServersStore } from '../../stores/localServers';
import styles from './ModelPicker.module.css';

interface ModelPickerProps {
  onOpenSettings: () => void;
}

/** Composer chip showing the answering model; switches between configured providers. */
export function ModelPicker({ onOpenSettings }: ModelPickerProps) {
  const providers = useSettingsStore((s) => s.providers);
  const activeProviderId = useSettingsStore((s) => s.activeProviderId);
  const activateProvider = useSettingsStore((s) => s.activateProvider);
  const selectServerModel = useSettingsStore((s) => s.selectServerModel);
  const servers = useLocalServersStore((s) => s.servers);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [open]);

  const active = providers.find((p) => p.id === activeProviderId);
  if (!active) return null;

  // Installed models on local servers that aren't already a configured choice.
  const serverModels = servers.flatMap((server) =>
    server.models
      .filter((m) => !providers.some((p) => p.name === server.provider && p.model === m))
      .map((model) => ({ server, model })),
  );

  return (
    <div className={styles.wrap} ref={ref} onKeyDown={(e) => e.key === 'Escape' && setOpen(false)}>
      <button
        type="button"
        className={clsx(styles.chip, open && styles.chipOpen)}
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="menu"
        aria-expanded={open}
        title="Model used to answer"
      >
        <Server size={11} />
        <span className={styles.chipText}>{providerModelLabel(active)}</span>
        <ChevronDown size={11} />
      </button>

      {open && (
        <div className={styles.menu} role="menu">
          <div className={styles.menuLabel}>Answer with</div>
          {providers.map((p) => (
            <button
              key={p.id}
              type="button"
              role="menuitemradio"
              aria-checked={p.id === activeProviderId}
              className={styles.item}
              onClick={() => {
                void activateProvider(p.id);
                setOpen(false);
              }}
            >
              <span className={styles.itemText}>
                <span className={styles.itemModel}>{providerModelLabel(p)}</span>
                <span className={styles.itemProvider}>{providerDisplayName(p.name)}</span>
              </span>
              {p.id === activeProviderId && <Check size={13} className={styles.check} />}
            </button>
          ))}
          {serverModels.length > 0 && (
            <>
              <div className={styles.menuLabel}>Installed on this computer</div>
              {serverModels.slice(0, 8).map(({ server, model }) => (
                <button
                  key={`${server.provider}:${model}`}
                  type="button"
                  role="menuitemradio"
                  aria-checked={false}
                  className={styles.item}
                  onClick={() => {
                    void selectServerModel(server.provider, model, server.base_url);
                    setOpen(false);
                  }}
                >
                  <span className={styles.itemText}>
                    <span className={styles.itemModel}>{model}</span>
                    <span className={styles.itemProvider}>{server.label}</span>
                  </span>
                </button>
              ))}
            </>
          )}
          <button
            type="button"
            role="menuitem"
            className={clsx(styles.item, styles.manage)}
            onClick={() => {
              setOpen(false);
              onOpenSettings();
            }}
          >
            <Settings size={13} />
            <span>Manage providers…</span>
          </button>
        </div>
      )}
    </div>
  );
}
