import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ProcessSummary, summarizeSteps } from './ProcessSummary';
import type { ExplorationStep } from '../../stores/chat';

const step = (n: number, tool: string, status: ExplorationStep['status'], extra: Partial<ExplorationStep> = {}): ExplorationStep => ({
  stepNumber: n, tool, inputSummary: 'in', outputSummary: 'out', tokensUsed: 0, latencyMs: 0, cost: 0, status, ...extra,
});

describe('ProcessSummary', () => {
  it('shows the running step with friendly labels while active', () => {
    render(<ProcessSummary active steps={[step(1, 'query_rewrite', 'complete'), step(2, 'search', 'running')]} />);
    expect(screen.getAllByRole('button', { name: /Searching content…/ })[0]).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('Rewrote query')).toBeInTheDocument();
    expect(screen.queryByText('query_rewrite')).not.toBeInTheDocument();
  });

  it('collapses to a one-line summary when done, and reopens on click', () => {
    const steps = [step(1, 'search', 'complete', { latencyMs: 900 }), step(2, 'llm_call', 'complete', { latencyMs: 2200, cost: 0.03 })];
    render(<ProcessSummary active={false} steps={steps} />);
    const toggle = screen.getByRole('button', { name: /Worked through 2 steps/ });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.getByText('3.1s · $0.03')).toBeInTheDocument();
    expect(screen.queryByText('Generated answer')).not.toBeInTheDocument();
    fireEvent.click(toggle);
    expect(screen.getByText('Generated answer')).toBeInTheDocument();
  });

  it('renders nothing without steps', () => {
    const { container } = render(<ProcessSummary active={false} steps={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('sums latency and cost', () => {
    expect(summarizeSteps([step(1, 'a', 'complete', { latencyMs: 5, cost: 0.1 }), step(2, 'b', 'complete', { latencyMs: 7, cost: 0.2 })]))
      .toEqual({ latencyMs: 12, cost: 0.30000000000000004 });
  });
});
