import { useState, useEffect, useCallback } from 'react';
import { X, Download, Check, Cpu, Trash2 } from 'lucide-react';
import clsx from 'clsx';
import {
  getModelOptions,
  listDocuments,
  reenrichDocument,
  type ModelOption,
  type DocumentSummary,
} from '../../lib/tauri';
import { useLocalModelStore } from '../../stores/localModel';
import styles from './ModelDownloadDialog.module.css';

interface ModelDownloadDialogProps {
  onClose: () => void;
  /** When true, shows onboarding framing (first-run experience) */
  isOnboarding?: boolean;
}

export function ModelDownloadDialog({ onClose, isOnboarding }: ModelDownloadDialogProps) {
  const [options, setOptions] = useState<ModelOption[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [reenriching, setReenriching] = useState(false);
  const [reenrichDone, setReenrichDone] = useState(false);
  const [existingDocs, setExistingDocs] = useState<DocumentSummary[]>([]);

  // Shared store — download survives dialog close
  const {
    status,
    isDownloading,
    progress,
    error,
    refreshStatus,
    startDownload,
    removeModel,
    clearError,
  } = useLocalModelStore();

  useEffect(() => {
    getModelOptions().then(setOptions).catch(() => {});
    refreshStatus();
    listDocuments().then(setExistingDocs).catch(() => {});
  }, [refreshStatus]);

  const handleDownload = useCallback(async () => {
    if (!selectedId) return;
    clearError();
    startDownload(selectedId);
    // Don't await — download runs in background even if dialog closes
  }, [selectedId, startDownload, clearError]);

  const handleReenrichAll = useCallback(async () => {
    if (existingDocs.length === 0) return;
    setReenriching(true);
    setReenrichDone(false);
    for (const doc of existingDocs) {
      try {
        await reenrichDocument(doc.id);
      } catch {
        // Non-fatal — continue with remaining docs
      }
    }
    setReenriching(false);
    setReenrichDone(true);
  }, [existingDocs]);

  const handleDelete = useCallback(async () => {
    await removeModel();
  }, [removeModel]);

  // Refresh doc list when download completes
  useEffect(() => {
    if (status?.downloaded && !isDownloading) {
      listDocuments().then(setExistingDocs).catch(() => {});
    }
  }, [status?.downloaded, isDownloading]);

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.dialog} onClick={(e) => e.stopPropagation()}>
        <div className={styles.header}>
          <Cpu size={18} />
          <h2 className={styles.title}>
            {isOnboarding ? 'Welcome to TGG' : 'Local Model for Document Enrichment'}
          </h2>
          <button type="button" className={styles.closeBtn} onClick={onClose}>
            <X size={16} />
          </button>
        </div>

        <p className={styles.description}>
          {isOnboarding
            ? 'TGG uses a small local AI model to enrich your documents with summaries, entities, and topics before you query them — making answers faster and more accurate. The model runs entirely on your device and is never sent to any server.'
            : 'A small local AI model generates summaries and metadata for each document section, enabling faster and more accurate answers. The model runs entirely on your device.'}
        </p>

        {status?.downloaded ? (
          <div className={styles.installedSection}>
            <div className={styles.installedBadge}>
              <Check size={14} />
              <span>Model installed: {status.model_id}</span>
            </div>
            {status.size_bytes && (
              <span className={styles.sizeLabel}>
                {(status.size_bytes / 1_000_000).toFixed(0)} MB
              </span>
            )}
            <button type="button" className={styles.deleteBtn} onClick={handleDelete}>
              <Trash2 size={14} />
              Remove model
            </button>
          </div>
        ) : (
          <div className={styles.optionsList}>
            {options.map((opt) => (
              <label
                key={opt.id}
                className={clsx(styles.optionCard, selectedId === opt.id && styles.optionSelected)}
              >
                <input
                  type="radio"
                  name="model"
                  value={opt.id}
                  checked={selectedId === opt.id}
                  onChange={() => setSelectedId(opt.id)}
                  className={styles.radio}
                  disabled={isDownloading}
                />
                <div className={styles.optionInfo}>
                  <span className={styles.optionName}>{opt.name}</span>
                  <span className={styles.optionDesc}>{opt.description}</span>
                </div>
                <span className={styles.optionSize}>{opt.size_label}</span>
              </label>
            ))}
          </div>
        )}

        {isDownloading && progress && (
          <div className={styles.progressSection}>
            <span className={styles.phaseLabel}>{progress.phase}</span>
            <div className={styles.progressBar}>
              <div
                className={styles.progressFill}
                style={{ width: `${Math.min(progress.percent, 100)}%` }}
              />
            </div>
            <span className={styles.progressLabel}>
              {progress.total_bytes > 0
                ? `${(progress.downloaded_bytes / 1_000_000).toFixed(1)} / ${(progress.total_bytes / 1_000_000).toFixed(0)} MB`
                : progress.phase}
            </span>
          </div>
        )}

        {error && (
          <div className={styles.error}>{error}</div>
        )}

        {/* Re-enrich existing docs after download */}
        {status?.downloaded && existingDocs.length > 0 && !reenrichDone && (
          <div className={styles.reenrichSection}>
            <p className={styles.reenrichHint}>
              You have {existingDocs.length} existing document{existingDocs.length !== 1 ? 's' : ''} that were ingested without the local model.
              Re-enrich them now to add LLM-generated summaries and improve relation discovery.
            </p>
            <button
              type="button"
              className={styles.reenrichBtn}
              onClick={handleReenrichAll}
              disabled={reenriching}
            >
              {reenriching ? `Re-enriching… (${existingDocs.length} docs)` : `Re-enrich ${existingDocs.length} document${existingDocs.length !== 1 ? 's' : ''}`}
            </button>
          </div>
        )}
        {reenrichDone && (
          <div className={styles.reenrichDone}>
            <Check size={14} /> Re-enrichment complete — summaries and relations updated.
          </div>
        )}

        <div className={styles.footer}>
          <button type="button" className={styles.cancelBtn} onClick={onClose}>
            {status?.downloaded ? 'Close' : isOnboarding ? 'Skip for now' : 'Cancel'}
          </button>
          {!status?.downloaded && !isDownloading && (
            <button
              type="button"
              className={styles.downloadBtn}
              disabled={!selectedId}
              onClick={handleDownload}
            >
              <Download size={14} />
              Download
            </button>
          )}
          {isDownloading && (
            <button type="button" className={styles.cancelBtn} disabled>
              Downloading in background...
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
