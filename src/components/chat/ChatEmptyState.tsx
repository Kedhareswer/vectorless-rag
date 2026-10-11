import { Check, FileText, LoaderCircle, MessageSquareText, Plus, Server } from 'lucide-react';
import clsx from 'clsx';
import styles from './ChatEmptyState.module.css';

interface ChatEmptyStateProps {
  hasProvider: boolean;
  docNames: string[];
  isIngesting: boolean;
  onOpenSettings: () => void;
  onAddDocument: () => void;
  onFocusInput: () => void;
  /** Send a starter question. */
  onAsk: (question: string) => void;
}

/** Starter questions that work on any document set. */
export function starterQuestions(docCount: number): string[] {
  const questions = [
    'Summarize the key points',
    'List the important dates and deadlines',
    'What are the main risks or open issues?',
  ];
  if (docCount > 1) questions.push('Compare these documents — where do they differ?');
  return questions;
}

/**
 * What an empty chat shows: a setup checklist until a provider and a document
 * are in place, then starter questions about the attached documents.
 */
export function ChatEmptyState({
  hasProvider,
  docNames,
  isIngesting,
  onOpenSettings,
  onAddDocument,
  onFocusInput,
  onAsk,
}: ChatEmptyStateProps) {
  const hasDocs = docNames.length > 0;

  if (hasProvider && hasDocs) {
    const subject = docNames.length === 1 ? docNames[0] : `${docNames.length} documents`;
    return (
      <div className={styles.wrap}>
        <h2 className={styles.title}>Ask about {subject}</h2>
        <p className={styles.subtitle}>
          Answers are grounded in the document structure — every step is shown as it happens.
        </p>
        <div className={styles.starters}>
          {starterQuestions(docNames.length).map((q) => (
            <button key={q} type="button" className={styles.starter} onClick={() => onAsk(q)}>
              <MessageSquareText size={14} />
              <span>{q}</span>
            </button>
          ))}
        </div>
      </div>
    );
  }

  const steps = [
    {
      done: hasProvider,
      icon: Server,
      title: 'Connect a model provider',
      detail: 'Anthropic, OpenAI, Gemini, Groq, Ollama and more. Keys stay in your OS keychain.',
      action: { label: 'Open settings', onClick: onOpenSettings },
    },
    {
      done: hasDocs,
      icon: FileText,
      title: 'Add a document',
      detail: 'PDF, Word, Markdown, spreadsheets or code. You can also drop files here.',
      action: { label: isIngesting ? 'Adding…' : 'Add document', onClick: onAddDocument, busy: isIngesting },
    },
    {
      done: false,
      icon: MessageSquareText,
      title: 'Ask a question',
      detail: 'TGG reads the right sections and answers with one model call.',
      action: { label: 'Start typing', onClick: onFocusInput },
    },
  ];
  const next = steps.findIndex((s) => !s.done);

  return (
    <div className={styles.wrap}>
      <h2 className={styles.title}>Get started</h2>
      <p className={styles.subtitle}>Three steps to your first grounded answer.</p>
      <ol className={styles.checklist}>
        {steps.map((step, i) => {
          const Icon = step.icon;
          const current = i === next;
          const locked = i > next;
          return (
            <li key={step.title} className={clsx(styles.item, step.done && styles.done, current && styles.current)}>
              <span className={styles.marker}>{step.done ? <Check size={14} /> : <Icon size={14} />}</span>
              <div className={styles.text}>
                <span className={styles.itemTitle}>{step.title}</span>
                <span className={styles.itemDetail}>{step.detail}</span>
              </div>
              {!step.done && (
                <button
                  type="button"
                  className={clsx(styles.action, current && styles.actionPrimary)}
                  onClick={step.action.onClick}
                  disabled={locked || ('busy' in step.action && step.action.busy)}
                >
                  {'busy' in step.action && step.action.busy ? (
                    <LoaderCircle size={13} className={styles.spin} />
                  ) : i === 1 ? (
                    <Plus size={13} />
                  ) : null}
                  {step.action.label}
                </button>
              )}
            </li>
          );
        })}
      </ol>
    </div>
  );
}
