import { useEffect, useState } from 'react';
import { Mic, Square, LoaderCircle, Download, X } from 'lucide-react';
import clsx from 'clsx';
import { useDictationStore } from '../../stores/dictation';
import styles from './DictationButton.module.css';

interface DictationButtonProps {
  /** Called with the transcribed text (never empty). */
  onTranscript: (text: string) => void;
  disabled?: boolean;
}

/** Mic button for offline voice input; offers the model download on first use. */
export function DictationButton({ onTranscript, disabled }: DictationButtonProps) {
  const {
    status,
    options,
    phase,
    isDownloading,
    progress,
    error,
    refresh,
    download,
    startRecording,
    stopAndTranscribe,
    clearError,
  } = useDictationStore();
  const [showPrompt, setShowPrompt] = useState(false);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Close the download prompt once the model is ready.
  useEffect(() => {
    if (status?.downloaded) setShowPrompt(false);
  }, [status?.downloaded]);

  const handleClick = async () => {
    clearError();
    if (phase === 'recording') {
      const text = await stopAndTranscribe();
      if (text) onTranscript(text);
      return;
    }
    if (!status?.downloaded) {
      setShowPrompt((open) => !open);
      return;
    }
    await startRecording();
  };

  const recommended = options[0];
  const title =
    phase === 'recording'
      ? 'Stop and transcribe'
      : phase === 'transcribing'
        ? 'Transcribing…'
        : status?.downloaded
          ? 'Voice input (offline)'
          : 'Set up voice input';

  return (
    <div className={styles.wrap}>
      <button
        type="button"
        className={clsx(styles.micBtn, phase === 'recording' && styles.recording)}
        onClick={handleClick}
        disabled={disabled || phase === 'transcribing'}
        title={error ?? title}
        aria-label={title}
        aria-pressed={phase === 'recording'}
      >
        {phase === 'recording' ? (
          <Square size={12} fill="currentColor" />
        ) : phase === 'transcribing' ? (
          <LoaderCircle size={16} className={styles.spin} />
        ) : (
          <Mic size={16} />
        )}
      </button>

      {(showPrompt || (error && phase === 'idle')) && (
        <div className={styles.popover} role="dialog" aria-label="Voice input">
          <button
            type="button"
            className={styles.close}
            onClick={() => {
              setShowPrompt(false);
              clearError();
            }}
            aria-label="Close"
          >
            <X size={12} />
          </button>
          {showPrompt && !status?.downloaded && (
            <>
              <p className={styles.text}>
                Voice input runs offline with {recommended?.name ?? 'Whisper'}
                {recommended ? ` (${recommended.size_label})` : ''}. Audio never leaves your computer.
              </p>
              {isDownloading ? (
                <div className={styles.progress}>
                  <div className={styles.progressBar} style={{ width: `${progress?.percent ?? 0}%` }} />
                </div>
              ) : (
                <button type="button" className={styles.download} onClick={() => void download()}>
                  <Download size={12} /> Download
                </button>
              )}
            </>
          )}
          {error && <p className={styles.error}>{error}</p>}
        </div>
      )}
    </div>
  );
}
