import { Check, Cloud, FileText, HardDrive, LoaderCircle, MessageSquareText, Plus, Server } from 'lucide-react';
import clsx from 'clsx';
import styles from './ChatEmptyState.module.css';

interface ChatEmptyStateProps {
  hasProvider: boolean;
  /** The Local (offline) provider is selected but its model isn't downloaded. */
  localModelMissing?: boolean;
  docNames: string[];
  isIngesting: boolean;
  onOpenSettings: () => void;
  /** Answer with the offline model (adds the provider, offers the download). */
  onUseOffline: () => void;
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
  localModelMissing = false,
  docNames,
  isIngesting,
  onOpenSettings,
  onUseOffline,
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

  interface StepAction {
    label: string;
    onClick: () => void;
    icon?: typeof Plus;
    busy?: boolean;
  }
  const steps: { done: boolean; icon: typeof Plus; title: string; detail: string; actions: StepAction[] }[] = [
    {
      done: hasProvider,
      icon: Server,
      title: localModelMissing ? 'Download the offline model' : 'Choose how to answer',
      detail: localModelMissing
        ? 'One ~700 MB download, then everything runs on this computer.'
        : 'Offline on this computer (one ~700 MB download, private and free), or a cloud provider like Anthropic or OpenAI for the strongest answers.',
      actions: localModelMissing
        ? [{ label: 'Download', onClick: onUseOffline, icon: HardDrive }]
        : [
            { label: 'Use offline model', onClick: onUseOffline, icon: HardDrive },
            { label: 'Cloud provider', onClick: onOpenSettings, icon: Cloud },
          ],
    },
    {
      done: hasDocs,
      icon: FileText,
      title: 'Add a document',
      detail: 'PDF, Word, Markdown, spreadsheets or code. You can also drop files here.',
      actions: [{ label: isIngesting ? 'Adding…' : 'Add document', onClick: onAddDocument, icon: Plus, busy: isIngesting }],
    },
    {
      done: false,
      icon: MessageSquareText,
      title: 'Ask a question',
      detail: 'TGG reads the right sections and answers with one model call.',
      actions: [{ label: 'Start typing', onClick: onFocusInput }],
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
                <div className={styles.actions}>
                  {step.actions.map((action, a) => {
                    const ActionIcon = action.icon;
                    return (
                      <button
                        key={action.label}
                        type="button"
                        className={clsx(styles.action, current && a === 0 && styles.actionPrimary)}
                        onClick={action.onClick}
                        disabled={locked || action.busy}
                      >
                        {action.busy ? (
                          <LoaderCircle size={13} className={styles.spin} />
                        ) : ActionIcon ? (
                          <ActionIcon size={13} />
                        ) : null}
                        {action.label}
                      </button>
                    );
                  })}
                </div>
              )}
            </li>
          );
        })}
      </ol>
    </div>
  );
}
