import { useEffect, useRef, useState } from 'react';
import { Check, ChevronDown, Server, Settings } from 'lucide-react';
import clsx from 'clsx';
import { useSettingsStore } from '../../stores/settings';
import styles from './ModelPicker.module.css';

interface ModelPickerProps {
  onOpenSettings: () => void;
}

/** Composer chip showing the answering model; switches between configured providers. */
export function ModelPicker({ onOpenSettings }: ModelPickerProps) {
  const providers = useSettingsStore((s) => s.providers);
  const activeProviderId = useSettingsStore((s) => s.activeProviderId);
  const activateProvider = useSettingsStore((s) => s.activateProvider);
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
        <span className={styles.chipText}>{active.model || active.name}</span>
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
                <span className={styles.itemModel}>{p.model || '(no model set)'}</span>
                <span className={styles.itemProvider}>{p.name}</span>
              </span>
              {p.id === activeProviderId && <Check size={13} className={styles.check} />}
            </button>
          ))}
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
