import { useEffect, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import styles from './MessageActions.module.css';

interface MessageActionsProps {
  /** Markdown source of the answer. */
  content: string;
}

/** Actions under an assistant answer. */
export function MessageActions({ content }: MessageActionsProps) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 1500);
    return () => clearTimeout(t);
  }, [copied]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
    } catch {
      // Clipboard unavailable (permissions); nothing useful to show.
    }
  };

  return (
    <div className={styles.actions}>
      <button type="button" className={styles.action} onClick={copy} aria-label={copied ? 'Copied' : 'Copy answer'} title="Copy answer">
        {copied ? <Check size={13} /> : <Copy size={13} />}
        <span>{copied ? 'Copied' : 'Copy'}</span>
      </button>
    </div>
  );
}
