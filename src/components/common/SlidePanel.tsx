import { useEffect, useCallback, type ReactNode } from 'react';
import { X } from 'lucide-react';
import styles from './SlidePanel.module.css';

interface SlidePanelProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
}

export function SlidePanel({ title, onClose, children }: SlidePanelProps) {
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    },
    [onClose],
  );

  useEffect(() => {
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  return (
    <>
      <div className={styles.backdrop} onClick={onClose} />
      <div className={styles.panel}>
        <div className={styles.header}>
          <span className={styles.headerTitle}>{title}</span>
          <button className={styles.closeBtn} onClick={onClose} title="Close" type="button">
            <X size={16} />
          </button>
        </div>
        <div className={styles.body}>{children}</div>
      </div>
    </>
  );
}
