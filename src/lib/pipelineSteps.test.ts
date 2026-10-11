import { describe, it, expect } from 'vitest';
import { getStepMeta, PIPELINE_STEPS } from './pipelineSteps';
import { formatCost, formatLatency, formatCount } from './format';

describe('getStepMeta', () => {
  it('has a label for every step the backend emits', () => {
    for (const key of ['query_rewrite', 'hyde', 'stepback', 'tree_overview', 'search', 'expand', 'scan_lists', 'llm_call']) {
      expect(PIPELINE_STEPS[key]).toBeDefined();
    }
    expect(getStepMeta('llm_call').label).toBe('Generated answer');
    expect(getStepMeta('search').phase).toBe('fetch');
  });

  it('makes unknown step keys readable', () => {
    expect(getStepMeta('rerank_nodes').label).toBe('Rerank nodes');
  });
});

describe('format helpers', () => {
  it('formats latency, cost and counts', () => {
    expect(formatLatency(640)).toBe('640ms');
    expect(formatLatency(3100)).toBe('3.1s');
    expect(formatCost(0.0049)).toBe('$0.0049');
    expect(formatCost(0.0224)).toBe('$0.02');
    expect(formatCost(1.5)).toBe('$1.50');
    expect(formatCost(0)).toBe('$0.00');
    expect(formatCount(6210)).toBe('6,210');
  });
});
