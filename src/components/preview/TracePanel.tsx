import { useState } from 'react';
import { ChevronDown, Activity, Link2 } from 'lucide-react';
import clsx from 'clsx';
import { useChatStore } from '../../stores/chat';
import { TraceView } from './TraceView';
import { RelationsView } from './RelationsView';
import styles from './TracePanel.module.css';

export function TracePanel() {
  const [traceOpen, setTraceOpen] = useState(true);
  const [relationsOpen, setRelationsOpen] = useState(true);

  const explorationSteps = useChatStore((s) => s.explorationSteps);
  const isExploring = useChatStore((s) => s.isExploring);
  const hasTrace = explorationSteps.length > 0 || isExploring;

  return (
    <div className={styles.container}>
      {/* Exploration Trace */}
      <div className={styles.section}>
        <button
          type="button"
          className={styles.sectionHeader}
          onClick={() => setTraceOpen((v) => !v)}
          aria-expanded={traceOpen}
        >
          <ChevronDown
            size={14}
            className={clsx(styles.chevron, !traceOpen && styles.chevronClosed)}
          />
          <Activity size={14} className={styles.sectionIcon} />
          <span className={styles.sectionTitle}>Exploration Trace</span>
          {hasTrace && (
            <span className={styles.badge}>
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

      {/* Cross-Document Relations */}
      <div className={styles.section}>
        <button
          type="button"
          className={styles.sectionHeader}
          onClick={() => setRelationsOpen((v) => !v)}
          aria-expanded={relationsOpen}
        >
          <ChevronDown
            size={14}
            className={clsx(styles.chevron, !relationsOpen && styles.chevronClosed)}
          />
          <Link2 size={14} className={styles.sectionIcon} />
          <span className={styles.sectionTitle}>Relations</span>
        </button>
        {relationsOpen && (
          <div className={styles.sectionContent}>
            <RelationsView />
          </div>
        )}
      </div>
    </div>
  );
}
