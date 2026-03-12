import { Sun, Moon, Settings, FileText, Activity, Menu } from 'lucide-react';
import clsx from 'clsx';
import { useThemeStore } from '../../stores/theme';
import styles from './TopBar.module.css';

interface TopBarProps {
  chatTitle: string | null;
  onMenuClick: () => void;
  onSettingsClick: () => void;
  docsPanelOpen: boolean;
  onDocsToggle: () => void;
  tracePanelOpen: boolean;
  onTraceToggle: () => void;
}

export function TopBar({
  chatTitle,
  onMenuClick,
  onSettingsClick,
  docsPanelOpen,
  onDocsToggle,
  tracePanelOpen,
  onTraceToggle,
}: TopBarProps) {
  const { resolvedTheme, setTheme, theme } = useThemeStore();

  const cycleTheme = () => {
    if (theme === 'light') setTheme('dark');
    else if (theme === 'dark') setTheme('system');
    else setTheme('light');
  };

  return (
    <div className={styles.topBar}>
      <button
        className={styles.menuButton}
        onClick={onMenuClick}
        title="Conversations"
        type="button"
      >
        <Menu size={18} />
      </button>

      <span className={clsx(styles.chatTitle, !chatTitle && styles.chatTitleEmpty)}>
        {chatTitle || 'TGG'}
      </span>

      <div className={styles.actions}>
        <button
          className={styles.iconBtn}
          onClick={cycleTheme}
          title={`Theme: ${theme} (click to cycle)`}
          type="button"
        >
          {resolvedTheme === 'dark' ? <Moon size={16} /> : <Sun size={16} />}
        </button>

        <button
          className={styles.iconBtn}
          onClick={onSettingsClick}
          title="Settings"
          type="button"
        >
          <Settings size={16} />
        </button>

        <div className={styles.divider} />

        <button
          className={clsx(styles.iconBtn, docsPanelOpen && styles.iconBtnActive)}
          onClick={onDocsToggle}
          title="Documents"
          type="button"
        >
          <FileText size={16} />
        </button>

        <button
          className={clsx(styles.iconBtn, tracePanelOpen && styles.iconBtnActive)}
          onClick={onTraceToggle}
          title="Trace & Evaluation"
          type="button"
        >
          <Activity size={16} />
        </button>
      </div>
    </div>
  );
}
