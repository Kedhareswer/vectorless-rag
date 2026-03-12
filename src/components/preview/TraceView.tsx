import { useState, useMemo } from 'react';
import {
  Activity,
  Zap,
  Clock,
  Check,
  BookOpen,
  Search,
  FileText,
  List,
  Cpu,
} from 'lucide-react';
import clsx from 'clsx';
import { useChatStore } from '../../stores/chat';
import type { ExplorationStep } from '../../stores/chat';
import styles from './TraceView.module.css';

/** Map deterministic fetch operation names to human-friendly labels */
function getStepLabel(tool: string): string {
  switch (tool) {
    case 'tree_overview': return 'Reading structure';
    case 'search': return 'Searching content';
    case 'expand': return 'Reading section';
    case 'scan_lists': return 'Scanning lists & tables';
    case 'llm_call': return 'Generating answer';
    default: return tool;
  }
}

function getStepIcon(tool: string) {
  switch (tool) {
    case 'tree_overview': return FileText;
    case 'search': return Search;
    case 'expand': return BookOpen;
    case 'scan_lists': return List;
    case 'llm_call': return Cpu;
    default: return FileText;
  }
}

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
          {(() => { const Icon = getStepIcon(step.tool); return <Icon size={12} />; })()}
          <span className={styles.toolName}>{getStepLabel(step.tool)}</span>
          {step.status === 'running' ? (
            <span className={styles.statusRunning} title="Running" />
          ) : (
            <Check size={14} className={styles.statusComplete} />
          )}
        </div>

        <div className={styles.badgeRow}>
          {step.latencyMs > 0 && (
            <span className={styles.badge}>
              <Clock size={10} />
              {formatLatency(step.latencyMs)}
            </span>
          )}
          {step.tokensUsed > 0 && (
            <span className={styles.badge}>
              <Zap size={10} />
              {step.tokensUsed.toLocaleString()} tokens
            </span>
          )}
          {step.tokensUsed === 0 && step.latencyMs === 0 && (
            <span className={styles.badge}>local</span>
          )}
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

type ViewScope = 'query' | 'session';

export function TraceView() {
  const explorationSteps = useChatStore((s) => s.explorationSteps);
  const sessionTotals = useChatStore((s) => s.sessionTotals);
  const sessionSteps = useChatStore((s) => s.sessionSteps);
  const isLoadingSession = useChatStore((s) => s.isLoadingSession);
  const [viewScope, setViewScope] = useState<ViewScope>('session');

  const queryTotals = useMemo(() => {
    let tokens = 0;
    let cost = 0;
    let latency = 0;

    for (const step of explorationSteps) {
      tokens += step.tokensUsed;
      latency += step.latencyMs;
      cost += step.cost;
    }

    return { tokens, cost, latency, steps: explorationSteps.length };
  }, [explorationSteps]);

  const displayTotals = viewScope === 'session'
    ? {
        tokens: sessionTotals.tokens + queryTotals.tokens,
        cost: sessionTotals.cost + queryTotals.cost,
        latency: sessionTotals.latency + queryTotals.latency,
        steps: sessionTotals.steps + queryTotals.steps,
      }
    : queryTotals;

  // Empty state — compact, no bloated placeholder (skip while loading)
  if (explorationSteps.length === 0 && sessionTotals.steps === 0 && !isLoadingSession) {
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
        <div className={styles.emptyState}>
          <Activity size={16} className={styles.emptyIcon} />
          <span>Run a query to see traces</span>
        </div>
      </div>
    );
  }

  return (
    <div className={styles.container}>
      <div className={styles.scopeToggle}>
        <button
          type="button"
          className={clsx(styles.scopeBtn, viewScope === 'query' && styles.scopeBtnActive)}
          onClick={() => setViewScope('query')}
        >
          This query
        </button>
        <button
          type="button"
          className={clsx(styles.scopeBtn, viewScope === 'session' && styles.scopeBtnActive)}
          onClick={() => setViewScope('session')}
        >
          Session
        </button>
      </div>

      <div className={styles.header}>
        <div className={styles.statCard}>
          <span className={styles.statLabel}>Tokens</span>
          <span className={styles.statValue}>
            {displayTotals.tokens.toLocaleString()}
          </span>
        </div>
        <div className={styles.statCard}>
          <span className={styles.statLabel}>Cost</span>
          <span className={styles.statValue}>
            {formatCost(displayTotals.cost)}
          </span>
        </div>
        <div className={styles.statCard}>
          <span className={styles.statLabel}>Latency</span>
          <span className={styles.statValue}>
            {formatLatency(displayTotals.latency)}
          </span>
        </div>
        <div className={styles.statCard}>
          <span className={styles.statLabel}>Steps</span>
          <span className={styles.statValue}>{displayTotals.steps}</span>
        </div>
      </div>

      <div className={styles.timeline}>
        {viewScope === 'session' && sessionSteps.map((step) => (
          <StepItem key={`s-${step.stepNumber}`} step={step} />
        ))}
        {explorationSteps.map((step) => (
          <StepItem key={`q-${step.stepNumber}`} step={step} />
        ))}
      </div>
    </div>
  );
}
