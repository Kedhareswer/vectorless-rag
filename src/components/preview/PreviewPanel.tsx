import { useState, useEffect, useRef } from 'react';
import {
  PanelRightClose,
  PanelRightOpen,
  ChevronDown,
  GitBranch,
  Activity,
  Network,
} from 'lucide-react';
import clsx from 'clsx';
import { useDocumentsStore } from '../../stores/documents';
import { useChatStore } from '../../stores/chat';
import { IconButton } from '../common/IconButton';
import { TreeView } from './TreeView';
import { CanvasView } from './CanvasView';
import { TraceView } from './TraceView';
import styles from './PreviewPanel.module.css';

type ViewMode = 'tree' | 'graph';

export function PreviewPanel() {
  const [collapsed, setCollapsed] = useState(true);
  const [structureOpen, setStructureOpen] = useState(true);
  const [traceOpen, setTraceOpen] = useState(true);
  const [viewMode, setViewMode] = useState<ViewMode>('tree');

  const activeDocumentId = useDocumentsStore((s) => s.activeDocumentId);
  const isExploring = useChatStore((s) => s.isExploring);
  const explorationSteps = useChatStore((s) => s.explorationSteps);

  const prevDocId = useRef(activeDocumentId);
  const prevExploring = useRef(isExploring);

  // Auto-expand when a document is selected
  useEffect(() => {
    if (activeDocumentId && activeDocumentId !== prevDocId.current) {
      setCollapsed(false);
      setStructureOpen(true);
    }
    prevDocId.current = activeDocumentId;
  }, [activeDocumentId]);

  // Auto-expand trace when exploration starts
  useEffect(() => {
    if (isExploring && !prevExploring.current) {
      setCollapsed(false);
      setTraceOpen(true);
    }
    prevExploring.current = isExploring;
  }, [isExploring]);

  const hasTrace = explorationSteps.length > 0 || isExploring;

  return (
    <div className={clsx(styles.panel, collapsed && styles.collapsed)}>
      <div className={styles.toggleBar}>
        <IconButton
          icon={collapsed ? PanelRightOpen : PanelRightClose}
          onClick={() => setCollapsed(!collapsed)}
          title={collapsed ? 'Expand panel' : 'Collapse panel'}
          size="sm"
        />
      </div>

      {!collapsed && (
        <div className={styles.sections}>
          {/* Document Structure Section */}
          <div className={clsx(styles.section, !structureOpen && styles.sectionCollapsed)}>
            <div className={styles.sectionHeaderRow}>
              <button
                type="button"
                className={clsx(styles.sectionHeader, structureOpen && styles.sectionHeaderOpen)}
                onClick={() => setStructureOpen((v) => !v)}
                aria-expanded={structureOpen}
              >
                <ChevronDown size={14} className={clsx(styles.chevron, !structureOpen && styles.chevronClosed)} />
                <GitBranch size={14} className={styles.sectionIcon} />
                <span className={styles.sectionTitle}>Document Structure</span>
              </button>
              <div className={styles.viewToggle}>
                <button
                  type="button"
                  className={clsx(styles.viewToggleBtn, viewMode === 'tree' && styles.viewToggleBtnActive)}
                  onClick={() => setViewMode('tree')}
                  title="Tree view"
                >
                  <GitBranch size={12} />
                </button>
                <button
                  type="button"
                  className={clsx(styles.viewToggleBtn, viewMode === 'graph' && styles.viewToggleBtnActive)}
                  onClick={() => setViewMode('graph')}
                  title="Graph view"
                >
                  <Network size={12} />
                </button>
              </div>
            </div>
            {structureOpen && (
              <div className={styles.sectionContent}>
                {viewMode === 'tree' ? <TreeView /> : <CanvasView />}
              </div>
            )}
          </div>

          {/* Exploration Trace Section */}
          <div className={clsx(styles.section, !traceOpen && styles.sectionCollapsed)}>
            <button
              type="button"
              className={clsx(styles.sectionHeader, traceOpen && styles.sectionHeaderOpen)}
              onClick={() => setTraceOpen((v) => !v)}
              aria-expanded={traceOpen}
            >
              <ChevronDown size={14} className={clsx(styles.chevron, !traceOpen && styles.chevronClosed)} />
              <Activity size={14} className={styles.sectionIcon} />
              <span className={styles.sectionTitle}>Exploration Trace</span>
              {hasTrace && (
                <span className={styles.sectionBadge}>
                  {explorationSteps.length} step{explorationSteps.length !== 1 ? 's' : ''}
                </span>
              )}
            </button>
            {traceOpen && (
              <div className={styles.sectionContent}>
                <TraceView />
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
