import { useState } from 'react';
import { ChevronRight, LoaderCircle } from 'lucide-react';
import clsx from 'clsx';
import type { ExplorationStep } from '../../stores/chat';
import { getStepMeta } from '../../lib/pipelineSteps';
import { formatCount, formatLatency } from '../../lib/format';
import styles from './ThinkingBlock.module.css';

interface ThinkingBlockProps {
  step: ExplorationStep;
}

/** One pipeline step: icon, label, result preview; click to see input/output. */
export function ThinkingBlock({ step }: ThinkingBlockProps) {
  const [expanded, setExpanded] = useState(false);
  const meta = getStepMeta(step.tool);
  const Icon = meta.icon;
  const running = step.status === 'running';
  const hasDetails = !running && (step.inputSummary || step.outputSummary);

  return (
    <div className={clsx(styles.step, running && styles.running)}>
      <button
        type="button"
        className={styles.header}
        onClick={() => hasDetails && setExpanded((v) => !v)}
        aria-expanded={hasDetails ? expanded : undefined}
        disabled={!hasDetails}
      >
        <span className={styles.iconWrap}>
          {running ? <LoaderCircle size={13} className={styles.spin} /> : <Icon size={13} />}
        </span>
        <span className={styles.label}>{running ? `${meta.activeLabel}…` : meta.label}</span>
        {!running && step.outputSummary && (
          <span className={styles.summary}>{step.outputSummary}</span>
        )}
        <span className={styles.badges}>
          {step.tokensUsed > 0 && <span className={styles.badge}>{formatCount(step.tokensUsed)} tok</span>}
          {step.latencyMs > 0 && <span className={styles.badge}>{formatLatency(step.latencyMs)}</span>}
        </span>
        {hasDetails && (
          <ChevronRight size={13} className={clsx(styles.chevron, expanded && styles.chevronOpen)} />
        )}
      </button>

      {expanded && (
        <div className={styles.details}>
          {step.inputSummary && (
            <div className={styles.detailRow}>
              <span className={styles.detailLabel}>Input</span>
              <p className={styles.detailValue}>{step.inputSummary}</p>
            </div>
          )}
          {step.outputSummary && (
            <div className={styles.detailRow}>
              <span className={styles.detailLabel}>Result</span>
              <p className={styles.detailValue}>{step.outputSummary}</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
