import { useState, useMemo } from 'react';
import {
  Activity,
  Zap,
  Clock,
  Check,
} from 'lucide-react';
import clsx from 'clsx';
import { useChatStore, PROVIDER_COST_RATES } from '../../stores/chat';
import { useSettingsStore } from '../../stores/settings';
import type { ExplorationStep } from '../../stores/chat';
import sharedStyles from './PreviewPanel.module.css';
import styles from './TraceView.module.css';

/** Format milliseconds into a human-friendly string */
function formatLatency(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

/** Format cost to dollars */
function formatCost(cost: number): string {
  if (cost < 0.01) return `$${cost.toFixed(4)}`;
  return `$${cost.toFixed(2)}`;
}

interface StepItemProps {
  step: ExplorationStep;
}

function StepItem({ step }: StepItemProps) {
  const [inputExpanded, setInputExpanded] = useState(false);
  const [outputExpanded, setOutputExpanded] = useState(false);

  return (
    <div className={styles.stepItem}>
      <div className={styles.stepNumber}>{step.stepNumber}</div>
      <div className={styles.stepBody}>
        <div className={styles.toolRow}>
          <span className={styles.toolName}>{step.tool}</span>
          {step.status === 'running' ? (
            <span className={styles.statusRunning} title="Running" />
          ) : (
            <Check size={14} className={styles.statusComplete} />
          )}
        </div>

        <div className={styles.badgeRow}>
          <span className={styles.badge}>
            <Clock size={10} />
            {formatLatency(step.latencyMs)}
          </span>
          <span className={styles.badge}>
            <Zap size={10} />
            {step.tokensUsed} tokens
          </span>
        </div>

        {step.inputSummary && (
          <>
            <span className={styles.summaryLabel}>Input</span>
            <div
              className={clsx(
                styles.summary,
                inputExpanded && styles.summaryExpanded,
              )}
              onClick={() => setInputExpanded((v) => !v)}
              title="Click to expand"
            >
              {step.inputSummary}
            </div>
          </>
        )}

        {step.outputSummary && (
          <>
            <span className={styles.summaryLabel}>Output</span>
            <div
              className={clsx(
                styles.summary,
                outputExpanded && styles.summaryExpanded,
              )}
              onClick={() => setOutputExpanded((v) => !v)}
              title="Click to expand"
            >
              {step.outputSummary}
            </div>
          </>
        )}
      </div>
    </div>
  );
}

export function TraceView() {
  const explorationSteps = useChatStore((s) => s.explorationSteps);
  const { providers, activeProviderId } = useSettingsStore();
  const activeProvider = providers.find((p) => p.id === activeProviderId);
  const providerName = activeProvider?.name.toLowerCase() ?? '';

  const totals = useMemo(() => {
    let tokens = 0;
    let cost = 0;
    let latency = 0;

    // Look up per-provider cost rate ($ per 1M tokens, blended)
    const ratePerMillion = PROVIDER_COST_RATES[providerName] ?? 0.10;

    for (const step of explorationSteps) {
      tokens += step.tokensUsed;
      latency += step.latencyMs;
      cost += (step.tokensUsed / 1_000_000) * ratePerMillion;
    }

    return { tokens, cost, latency, steps: explorationSteps.length, providerName };
  }, [explorationSteps, providerName]);

  // Empty state
  if (explorationSteps.length === 0) {
    return (
      <div className={styles.container}>
        <div className={styles.header}>
          <div className={styles.statCard}>
            <span className={styles.statLabel}>Tokens</span>
            <span className={styles.statValue}>0</span>
          </div>
          <div className={styles.statCard}>
            <span className={styles.statLabel}>Cost</span>
            <span className={styles.statValue}>$0.00</span>
          </div>
          <div className={styles.statCard}>
            <span className={styles.statLabel}>Latency</span>
            <span className={styles.statValue}>0ms</span>
          </div>
          <div className={styles.statCard}>
            <span className={styles.statLabel}>Steps</span>
            <span className={styles.statValue}>0</span>
          </div>
        </div>
        <div className={sharedStyles.placeholder}>
          <Activity size={32} className={sharedStyles.placeholderIcon} />
          <p className={sharedStyles.placeholderText}>
            Run a query to see exploration traces
          </p>
          <p className={sharedStyles.placeholderHint}>
            Exploration steps will be visualized as a timeline
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className={styles.container}>
      <div className={styles.header}>
        <div className={styles.statCard}>
          <span className={styles.statLabel}>Tokens</span>
          <span className={styles.statValue}>
            {totals.tokens.toLocaleString()}
          </span>
        </div>
        <div className={styles.statCard}>
          <span className={styles.statLabel}>Cost</span>
          <span className={styles.statValue}>
            {formatCost(totals.cost)}
          </span>
        </div>
        <div className={styles.statCard}>
          <span className={styles.statLabel}>Latency</span>
          <span className={styles.statValue}>
            {formatLatency(totals.latency)}
          </span>
        </div>
        <div className={styles.statCard}>
          <span className={styles.statLabel}>Steps</span>
          <span className={styles.statValue}>{totals.steps}</span>
        </div>
      </div>

      <div className={styles.timeline}>
        {explorationSteps.map((step) => (
          <StepItem key={step.stepNumber} step={step} />
        ))}
      </div>
    </div>
  );
}
