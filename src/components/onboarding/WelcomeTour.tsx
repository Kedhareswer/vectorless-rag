import { useEffect, useRef, useState } from 'react';
import { ArrowLeft, ArrowRight, BookOpenText, Cloud, Cpu, HardDrive, Search, Sparkles, X } from 'lucide-react';
import clsx from 'clsx';
import { MODEL_GUIDES, MODEL_PATH_ORDER, type ModelPath } from '../../lib/modelGuides';
import { useLocalServersStore, firstUsableServer } from '../../stores/localServers';
import styles from './Onboarding.module.css';

const PATH_ICONS: Record<ModelPath, typeof Cpu> = { installed: Cpu, offline: HardDrive, cloud: Cloud };

interface WelcomeTourProps {
  onClose: () => void;
}

/**
 * First-run tour: what the app does, why it needs a model, and the ways to get one.
 * It only explains — the how-to is shown later, when the user first adds a model.
 */
export function WelcomeTour({ onClose }: WelcomeTourProps) {
  const [step, setStep] = useState(0);
  const server = firstUsableServer(useLocalServersStore((s) => s.servers));
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    dialogRef.current?.focus();
  }, [step]);

  const slides = [
    {
      title: 'Ask questions about your documents',
      body: (
        <>
          <p className={styles.lead}>
            Add PDFs, Word files, notes or code. TGG reads their structure — sections, tables and
            pages — and answers from the exact passages, showing every step it takes.
          </p>
          <ul className={styles.points}>
            <li><BookOpenText size={15} /> Answers point to the sections they came from.</li>
            <li><Search size={15} /> No embeddings or vector index — documents are read like a person would.</li>
            <li><HardDrive size={15} /> Your files and chats stay on this computer.</li>
          </ul>
        </>
      ),
    },
    {
      title: 'Why does it need a model?',
      body: (
        <>
          <p className={styles.lead}>
            The app finds the right passages on its own. A language model is only needed to turn
            them into an answer in plain words.
          </p>
          <div className={styles.roles}>
            <div className={styles.role}>
              <span className={styles.roleTag}>Required</span>
              <strong>Answer writer</strong>
              <span>Reads the passages and writes the answer. Runs on this computer or in the cloud — your choice.</span>
            </div>
            <div className={styles.role}>
              <span className={clsx(styles.roleTag, styles.roleTagSoft)}>Optional</span>
              <strong>Search helper</strong>
              <span>The built-in LFM2.5 model rephrases your question to find more matches. Free and offline.</span>
            </div>
          </div>
        </>
      ),
    },
    {
      title: 'Three ways to answer',
      body: (
        <>
          <div className={styles.paths}>
            {MODEL_PATH_ORDER.map((id) => {
              const guide = MODEL_GUIDES[id];
              const Icon = PATH_ICONS[id];
              return (
                <div key={id} className={styles.pathCard}>
                  <Icon size={18} className={styles.pathIcon} />
                  <strong>{guide.title}</strong>
                  <span className={styles.pathSummary}>{guide.summary}</span>
                  <span className={styles.facts}>
                    {guide.facts.map((f) => <span key={f} className={styles.fact}>{f}</span>)}
                  </span>
                  {id === 'installed' && server && (
                    <span className={styles.found}>
                      Found {server.label} · {server.models.length} model{server.models.length === 1 ? '' : 's'}
                    </span>
                  )}
                </div>
              );
            })}
          </div>
          <p className={styles.note}>
            <Sparkles size={14} /> No need to decide now. When you first need a model, we’ll show you
            how to add it, step by step.
          </p>
        </>
      ),
    },
  ];

  const last = step === slides.length - 1;
  const slide = slides[step];

  return (
    <div className={styles.overlay} role="presentation">
      <div
        ref={dialogRef}
        className={clsx(styles.dialog, styles.tour)}
        role="dialog"
        aria-modal="true"
        aria-labelledby="tour-title"
        tabIndex={-1}
        onKeyDown={(e) => {
          if (e.key === 'Escape') onClose();
          if (e.key === 'ArrowRight' && !last) setStep(step + 1);
          if (e.key === 'ArrowLeft' && step > 0) setStep(step - 1);
        }}
      >
        <button type="button" className={styles.close} onClick={onClose} aria-label="Skip tour">
          <X size={16} />
        </button>
        <span className={styles.kicker}>Welcome · {step + 1} of {slides.length}</span>
        <h2 id="tour-title" className={styles.title}>{slide.title}</h2>
        <div className={styles.slide}>{slide.body}</div>

        <div className={styles.footer}>
          <div className={styles.dots} aria-hidden="true">
            {slides.map((s, i) => (
              <span key={s.title} className={clsx(styles.dot, i === step && styles.dotActive)} />
            ))}
          </div>
          {step > 0 ? (
            <button type="button" className={styles.secondary} onClick={() => setStep(step - 1)}>
              <ArrowLeft size={14} /> Back
            </button>
          ) : (
            <button type="button" className={styles.secondary} onClick={onClose}>
              Skip
            </button>
          )}
          <button
            type="button"
            className={styles.primary}
            onClick={() => (last ? onClose() : setStep(step + 1))}
          >
            {last ? 'Get started' : 'Next'} {!last && <ArrowRight size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}
