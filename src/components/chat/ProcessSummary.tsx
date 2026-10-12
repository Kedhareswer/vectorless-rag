import { useEffect, useState } from 'react';
import { Check, ChevronRight, LoaderCircle } from 'lucide-react';
import clsx from 'clsx';
import type { ExplorationStep } from '../../stores/chat';
import { getStepMeta } from '../../lib/pipelineSteps';
import { formatCost, formatLatency } from '../../lib/format';
import { ThinkingBlock } from './ThinkingBlock';
import styles from './ProcessSummary.module.css';

interface ProcessSummaryProps {
  steps: ExplorationStep[];
  /** True while the query is still running. */
  active: boolean;
}

/** Totals across steps, for the collapsed summary line. */
export function summarizeSteps(steps: ExplorationStep[]) {
  return steps.reduce(
    (acc, s) => ({ latencyMs: acc.latencyMs + s.latencyMs, cost: acc.cost + s.cost }),
    { latencyMs: 0, cost: 0 },
  );
}

/**
 * How the answer was produced: open while the pipeline runs, collapsed to a
 * single line once it finishes (the user can reopen it).
 */
export function ProcessSummary({ steps, active }: ProcessSummaryProps) {
  const [open, setOpen] = useState(active);

  // Follow the pipeline: open while running, fold away when done.
  useEffect(() => {
    setOpen(active);
  }, [active]);

  if (steps.length === 0) return null;

  const current = [...steps].reverse().find((s) => s.status === 'running');
  const { latencyMs, cost } = summarizeSteps(steps);
  const title = active
    ? `${getStepMeta(current?.tool ?? steps[steps.length - 1].tool).activeLabel}…`
    : `Worked through ${steps.length} step${steps.length === 1 ? '' : 's'}`;
  const meta = [latencyMs > 0 && formatLatency(latencyMs), cost > 0 && formatCost(cost)]
    .filter(Boolean)
    .join(' · ');

  return (
    <div className={clsx(styles.wrap, open && styles.open)}>
      <button
        type="button"
        className={styles.toggle}
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        {active ? (
          <LoaderCircle size={14} className={clsx(styles.statusIcon, styles.spin)} />
        ) : (
          <Check size={14} className={styles.statusIcon} />
        )}
        <span className={styles.title}>{title}</span>
        {!active && meta && <span className={styles.meta}>{meta}</span>}
        <ChevronRight size={14} className={clsx(styles.chevron, open && styles.chevronOpen)} />
      </button>

      {open && (
        <div className={styles.list}>
          {steps.map((step) => (
            <ThinkingBlock key={step.stepNumber} step={step} />
          ))}
        </div>
      )}
    </div>
  );
}
