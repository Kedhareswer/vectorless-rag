import { useCallback, useEffect, useState } from 'react';
import { ChevronDown, ChevronRight, Loader, RefreshCw, Send } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeSanitize from 'rehype-sanitize';
import { useChatStore } from '../../stores/chat';
import { useSettingsStore } from '../../stores/settings';
import {
  listQuestionBatches,
  refreshQuestionBatch,
  submitQuestionBatch,
  type BatchJobDetail,
} from '../../lib/tauri';
import { relativeDay } from '../../lib/relativeTime';
import styles from './BatchPanel.module.css';

/** Mirrors MAX_BATCH_QUESTIONS in agent/batch.rs. */
const MAX_QUESTIONS = 50;

function statusLabel({ job, items }: BatchJobDetail): string {
  if (job.status !== 'ended') return job.status === 'canceling' ? 'Canceling…' : 'Processing…';
  const failed = items.filter((i) => i.error).length;
  return failed > 0 ? `Done · ${failed} failed` : 'Done';
}

/**
 * Ask many questions over this chat's documents at once. Each question runs
 * the normal enrich → fetch pipeline; the answer calls go to the provider's
 * Batch API (half price, finished within 24 h, usually minutes).
 */
export function BatchPanel() {
  const activeConversationId = useChatStore((s) => s.activeConversationId);
  const conversationDocIds = useChatStore((s) => s.conversationDocIds);
  const provider = useSettingsStore((s) => s.providers.find((p) => p.id === s.activeProviderId));

  const [draft, setDraft] = useState('');
  const [jobs, setJobs] = useState<BatchJobDetail[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [refreshing, setRefreshing] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const questions = draft.split('\n').map((q) => q.trim()).filter(Boolean);
  const supported = provider?.name.toLowerCase() === 'anthropic';
  const blocker = !activeConversationId
    ? 'Open a chat first.'
    : conversationDocIds.length === 0
      ? 'Add a document to this chat first.'
      : !provider
        ? 'Add a model first.'
        : !supported
          ? 'Batch questions use the Anthropic Batch API. Switch the model to an Anthropic provider.'
          : questions.length > MAX_QUESTIONS
            ? `A batch holds at most ${MAX_QUESTIONS} questions.`
            : null;

  useEffect(() => {
    if (!activeConversationId) {
      setJobs([]);
      return;
    }
    listQuestionBatches(activeConversationId)
      .then(setJobs)
      .catch((e) => setError(String(e)));
  }, [activeConversationId]);

  const replaceJob = (detail: BatchJobDetail) =>
    setJobs((prev) => [detail, ...prev.filter((j) => j.job.id !== detail.job.id)].sort(
      (a, b) => b.job.created_at.localeCompare(a.job.created_at),
    ));

  const handleSubmit = useCallback(async () => {
    if (!provider || blocker || questions.length === 0) return;
    setSubmitting(true);
    setError(null);
    try {
      const detail = await submitQuestionBatch(activeConversationId, conversationDocIds, provider.id, questions);
      replaceJob(detail);
      setExpanded(detail.job.id);
      setDraft('');
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  }, [provider, blocker, questions, activeConversationId, conversationDocIds]);

  const handleRefresh = useCallback(async (jobId: string) => {
    setRefreshing(jobId);
    setError(null);
    try {
      replaceJob(await refreshQuestionBatch(jobId));
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(null);
    }
  }, []);

  return (
    <div className={styles.panel}>
      <section className={styles.section}>
        <label className={styles.sectionTitle} htmlFor="batch-questions">
          Questions
        </label>
        <textarea
          id="batch-questions"
          className={styles.textarea}
          placeholder={'One question per line, e.g.\nWhat is the contract term?\nWho are the parties?'}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          rows={6}
          disabled={submitting}
        />
        <div className={styles.footer}>
          <span className={styles.hint}>
            {questions.length > 0 ? `${questions.length} question${questions.length === 1 ? '' : 's'}` : 'Half price · results within 24 h'}
          </span>
          <button
            type="button"
            className={styles.primaryBtn}
            onClick={() => void handleSubmit()}
            disabled={submitting || !!blocker || questions.length === 0}
          >
            {submitting ? <Loader size={13} className={styles.spin} /> : <Send size={13} />}
            <span>{submitting ? 'Preparing…' : 'Submit batch'}</span>
          </button>
        </div>
        {blocker && <p className={styles.note}>{blocker}</p>}
        {error && <p className={styles.error}>{error}</p>}
      </section>

      <section className={styles.section}>
        <span className={styles.sectionTitle}>Batches in this chat</span>
        {jobs.length === 0 ? (
          <p className={styles.empty}>No batches yet.</p>
        ) : (
          <ul className={styles.jobs}>
            {jobs.map((detail) => {
              const { job, items } = detail;
              const open = expanded === job.id;
              return (
                <li key={job.id} className={styles.job}>
                  <div className={styles.jobHeader}>
                    <button
                      type="button"
                      className={styles.jobToggle}
                      onClick={() => setExpanded(open ? null : job.id)}
                      aria-expanded={open}
                    >
                      {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                      <span>{items.length} question{items.length === 1 ? '' : 's'}</span>
                      <span className={styles.meta}>{relativeDay(job.created_at)}</span>
                    </button>
                    <span className={job.status === 'ended' ? styles.statusDone : styles.statusPending}>
                      {statusLabel(detail)}
                    </span>
                    {!job.results_fetched && (
                      <button
                        type="button"
                        className={styles.iconBtn}
                        onClick={() => void handleRefresh(job.id)}
                        disabled={refreshing === job.id}
                        title="Check for results"
                        aria-label="Check for results"
                      >
                        <RefreshCw size={12} className={refreshing === job.id ? styles.spin : undefined} />
                      </button>
                    )}
                  </div>
                  {open && (
                    <ol className={styles.items}>
                      {items.map((item) => (
                        <li key={item.custom_id} className={styles.item}>
                          <p className={styles.question}>{item.question}</p>
                          {item.answer ? (
                            <div className={styles.answer}>
                              <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeSanitize]}>
                                {item.answer}
                              </ReactMarkdown>
                            </div>
                          ) : item.error ? (
                            <p className={styles.error}>{item.error}</p>
                          ) : (
                            <p className={styles.pending}>Waiting for the provider…</p>
                          )}
                        </li>
                      ))}
                    </ol>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </div>
  );
}
