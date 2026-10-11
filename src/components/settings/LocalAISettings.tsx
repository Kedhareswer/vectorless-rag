import { useEffect, useState } from 'react';
import { Cpu, Download, Mic, Trash2 } from 'lucide-react';
import { useLocalModelStore } from '../../stores/localModel';
import { useDictationStore } from '../../stores/dictation';
import { getModelOptions, type ModelOption } from '../../lib/tauri';
import styles from './SettingsModal.module.css';

interface LocalAISettingsProps {
  /** Open the enrichment-model download dialog. */
  onManageEnrichmentModel: () => void;
}

function formatMB(bytes: number | null | undefined): string {
  return bytes ? `${Math.round(bytes / 1_000_000)} MB` : '';
}

/** Offline models: query enrichment and dictation. Changes apply immediately. */
export function LocalAISettings({ onManageEnrichmentModel }: LocalAISettingsProps) {
  const { status: slm, refreshStatus } = useLocalModelStore();
  const dictation = useDictationStore();
  const { refresh: refreshDictation } = dictation;
  const [slmOptions, setSlmOptions] = useState<ModelOption[]>([]);

  useEffect(() => {
    void refreshStatus();
    void refreshDictation();
    getModelOptions().then(setSlmOptions).catch(() => {});
  }, [refreshStatus, refreshDictation]);

  const slmName = slmOptions.find((o) => o.id === slm?.model_id)?.name ?? slm?.model_id;

  const dictationModel = dictation.options.find((o) => o.id === dictation.status?.model_id);

  return (
    <>
      <div className={styles.card}>
        <div className={styles.cardHeader}>
          <Cpu size={16} className={styles.cardIcon} />
          <div className={styles.cardText}>
            <span className={styles.cardTitle}>Query enrichment model</span>
            <span className={styles.cardDesc}>
              Rewrites questions and drafts search terms on your CPU before each answer, and
              summarises document sections. Optional — without it, keyword search is used.
            </span>
          </div>
        </div>
        <div className={styles.cardRow}>
          <span className={styles.statusText}>
            {slm?.downloaded ? (
              <>
                <span className={styles.statusDotOn} /> Installed: {slmName}
                {slm.size_bytes ? ` · ${formatMB(slm.size_bytes)}` : ''}
              </>
            ) : (
              <>
                <span className={styles.statusDotOff} /> Not installed
              </>
            )}
          </span>
          <button type="button" className={styles.secondaryButton} onClick={onManageEnrichmentModel}>
            {slm?.downloaded ? 'Change model' : 'Choose a model'}
          </button>
        </div>
      </div>

      <div className={styles.card}>
        <div className={styles.cardHeader}>
          <Mic size={16} className={styles.cardIcon} />
          <div className={styles.cardText}>
            <span className={styles.cardTitle}>Dictation</span>
            <span className={styles.cardDesc}>
              Speak your question with the mic button. Speech is transcribed offline by Whisper;
              audio never leaves this computer.
            </span>
          </div>
        </div>

        {dictation.status?.downloaded ? (
          <div className={styles.cardRow}>
            <span className={styles.statusText}>
              <span className={styles.statusDotOn} /> Installed: {dictationModel?.name ?? dictation.status.model_id}
            </span>
            <button
              type="button"
              className={styles.iconTextButton}
              onClick={() => void dictation.remove()}
              title="Remove dictation model"
            >
              <Trash2 size={13} /> Remove
            </button>
          </div>
        ) : (
          <div className={styles.optionList}>
            {dictation.options.map((opt, i) => (
              <div key={opt.id} className={styles.optionRow}>
                <div className={styles.cardText}>
                  <span className={styles.optionName}>
                    {opt.name}
                    {i === 0 && <span className={styles.recommended}>Recommended</span>}
                  </span>
                  <span className={styles.cardDesc}>
                    {opt.description} {opt.size_label}
                  </span>
                </div>
                <button
                  type="button"
                  className={styles.secondaryButton}
                  onClick={() => void dictation.download(opt.id)}
                  disabled={dictation.isDownloading}
                >
                  <Download size={13} /> Download
                </button>
              </div>
            ))}
            {dictation.isDownloading && (
              <div className={styles.progress} role="progressbar" aria-valuenow={Math.round(dictation.progress?.percent ?? 0)}>
                <div className={styles.progressBar} style={{ width: `${dictation.progress?.percent ?? 0}%` }} />
              </div>
            )}
          </div>
        )}
        {dictation.error && <p className={styles.inlineError}>{dictation.error}</p>}
      </div>
    </>
  );
}
