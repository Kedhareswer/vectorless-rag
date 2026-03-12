import { useState } from 'react';
import { Wrench, Clock, Zap, ChevronDown, ChevronRight } from 'lucide-react';
import clsx from 'clsx';
import type { ExplorationStep } from '../../stores/chat';
import styles from './ThinkingBlock.module.css';

interface ThinkingBlockProps {
  step: ExplorationStep;
}

export function ThinkingBlock({ step }: ThinkingBlockProps) {
  const [expanded, setExpanded] = useState(false);

  if (step.status === 'running') {
    return (
      <div className={styles.running}>
        <div className={styles.pulseBar} />
        <div className={styles.runningContent}>
          <Wrench size={14} className={styles.toolIcon} />
          <span className={styles.toolName}>{step.tool}</span>
          <span className={styles.runningLabel}>Running...</span>
        </div>
      </div>
    );
  }

  return (
    <div className={clsx(styles.complete, expanded && styles.expanded)}>
      <button
        className={styles.completeHeader}
        onClick={() => setExpanded(!expanded)}
        type="button"
      >
        {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <Wrench size={14} className={styles.toolIcon} />
        <span className={styles.toolName}>{step.tool}</span>
        <span className={styles.summary}>
          {step.outputSummary.length > 60
            ? `${step.outputSummary.slice(0, 60)}...`
            : step.outputSummary}
        </span>
        <div className={styles.badges}>
          <span className={styles.tokenBadge}>
            <Zap size={10} />
            {step.tokensUsed}
          </span>
          <span className={styles.latencyBadge}>
            <Clock size={10} />
            {step.latencyMs}ms
          </span>
        </div>
      </button>

      {expanded && (
        <div className={styles.details}>
          <div className={styles.detailRow}>
            <span className={styles.detailLabel}>Input</span>
            <pre className={styles.detailValue}>{step.inputSummary}</pre>
          </div>
          <div className={styles.detailRow}>
            <span className={styles.detailLabel}>Output</span>
            <pre className={styles.detailValue}>{step.outputSummary}</pre>
          </div>
        </div>
      )}
    </div>
  );
}
