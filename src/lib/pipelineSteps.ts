import {
  ArrowUpRight,
  BookOpen,
  Cpu,
  FileText,
  List,
  PenLine,
  Search,
  Sparkles,
  Workflow,
  type LucideIcon,
} from 'lucide-react';

/** Which part of the pipeline a step belongs to. */
export type StepPhase = 'enrich' | 'fetch' | 'generate';

export interface StepMeta {
  /** Short label, e.g. "Searching content". */
  label: string;
  /** Present-tense label shown while the step runs. */
  activeLabel: string;
  icon: LucideIcon;
  phase: StepPhase;
}

/**
 * Display metadata for every pipeline step the backend emits (the `tool` key on
 * step events). Adding a pipeline step means adding one entry here; unknown
 * keys fall back to a readable version of the key.
 */
export const PIPELINE_STEPS: Record<string, StepMeta> = {
  query_rewrite: { label: 'Rewrote query', activeLabel: 'Rewriting query', icon: PenLine, phase: 'enrich' },
  hyde: { label: 'Hypothetical answer (HyDE)', activeLabel: 'Drafting a hypothetical answer', icon: Sparkles, phase: 'enrich' },
  stepback: { label: 'Broadened question (StepBack)', activeLabel: 'Broadening the question', icon: ArrowUpRight, phase: 'enrich' },
  tree_overview: { label: 'Read structure', activeLabel: 'Reading structure', icon: FileText, phase: 'fetch' },
  search: { label: 'Searched content', activeLabel: 'Searching content', icon: Search, phase: 'fetch' },
  expand: { label: 'Read section', activeLabel: 'Reading section', icon: BookOpen, phase: 'fetch' },
  scan_lists: { label: 'Scanned lists & tables', activeLabel: 'Scanning lists & tables', icon: List, phase: 'fetch' },
  llm_call: { label: 'Generated answer', activeLabel: 'Generating answer', icon: Cpu, phase: 'generate' },
};

export function getStepMeta(tool: string): StepMeta {
  const known = PIPELINE_STEPS[tool];
  if (known) return known;
  const readable = tool.replace(/[_-]+/g, ' ').replace(/^\w/, (c) => c.toUpperCase());
  return { label: readable, activeLabel: readable, icon: Workflow, phase: 'fetch' };
}
