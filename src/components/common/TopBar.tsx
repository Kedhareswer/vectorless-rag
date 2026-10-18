import { ChevronDown, FileText, Activity, ListChecks, Settings, SquarePen } from 'lucide-react';
import clsx from 'clsx';
import { withShortcut } from '../../lib/shortcuts';
import styles from './TopBar.module.css';

interface TopBarProps {
  chatTitle: string | null;
  switcherOpen: boolean;
  onSwitcherToggle: () => void;
  onNewChat: () => void;
  docsCount: number;
  docsPanelOpen: boolean;
  onDocsToggle: () => void;
  tracePanelOpen: boolean;
  onTraceToggle: () => void;
  batchPanelOpen: boolean;
  onBatchToggle: () => void;
  onSettingsClick: () => void;
}

export function TopBar({
  chatTitle,
  switcherOpen,
  onSwitcherToggle,
  onNewChat,
  docsCount,
  docsPanelOpen,
  onDocsToggle,
  tracePanelOpen,
  onTraceToggle,
  batchPanelOpen,
  onBatchToggle,
  onSettingsClick,
}: TopBarProps) {
  return (
    <header className={styles.topBar}>
      <button
        type="button"
        className={clsx(styles.titleButton, switcherOpen && styles.titleButtonOpen)}
        onClick={onSwitcherToggle}
        title={withShortcut('Switch chat', 'switchChat')}
        aria-haspopup="dialog"
        aria-expanded={switcherOpen}
        aria-label={`Chats${chatTitle ? `: ${chatTitle}` : ''}`}
      >
        <span className={clsx(styles.chatTitle, !chatTitle && styles.chatTitleEmpty)}>
          {chatTitle || 'Select a chat'}
        </span>
        <ChevronDown size={14} className={styles.titleChevron} />
      </button>

      <button
        type="button"
        className={styles.iconBtn}
        onClick={onNewChat}
        title={withShortcut('New chat', 'newChat')}
        aria-label="New chat"
      >
        <SquarePen size={16} />
      </button>

      <div className={styles.spacer} />

      <nav className={styles.actions} aria-label="Panels">
        <button
          type="button"
          className={clsx(styles.labelBtn, docsPanelOpen && styles.btnActive)}
          onClick={onDocsToggle}
          title={withShortcut('Documents in this chat', 'documents')}
          aria-label="Documents"
          aria-pressed={docsPanelOpen}
        >
          <FileText size={15} />
          <span className={styles.btnText}>Documents</span>
          {docsCount > 0 && <span className={styles.countBadge}>{docsCount}</span>}
        </button>

        <button
          type="button"
          className={clsx(styles.labelBtn, tracePanelOpen && styles.btnActive)}
          onClick={onTraceToggle}
          title={withShortcut('Trace & evaluation', 'trace')}
          aria-label="Trace"
          aria-pressed={tracePanelOpen}
        >
          <Activity size={15} />
          <span className={styles.btnText}>Trace</span>
        </button>

        <button
          type="button"
          className={clsx(styles.labelBtn, batchPanelOpen && styles.btnActive)}
          onClick={onBatchToggle}
          title={withShortcut('Batch questions', 'batch')}
          aria-label="Batch"
          aria-pressed={batchPanelOpen}
        >
          <ListChecks size={15} />
          <span className={styles.btnText}>Batch</span>
        </button>

        <div className={styles.divider} />

        <button
          type="button"
          className={styles.iconBtn}
          onClick={onSettingsClick}
          title={withShortcut('Settings', 'settings')}
          aria-label="Settings"
        >
          <Settings size={16} />
        </button>
      </nav>
    </header>
  );
}
