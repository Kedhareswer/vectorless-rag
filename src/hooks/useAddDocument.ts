import { useCallback, useState } from 'react';
import { openFileDialog } from '../lib/tauri';
import { useDocumentsStore, type DocumentSummary } from '../stores/documents';
import { useChatStore } from '../stores/chat';

/** Progress of a multi-file add: `done` of `total` processed so far. */
export interface BatchProgress {
  done: number;
  total: number;
  failed: number;
}

/**
 * The one way to add documents: ingest into the library, then attach to the
 * active chat (a chat is created if none is open).
 */
export function useAddDocument() {
  const ingest = useDocumentsStore((s) => s.ingestDocumentFromPath);
  const isIngesting = useDocumentsStore((s) => s.isIngesting);
  const attach = useChatStore((s) => s.addDocToActiveConversation);
  const [progress, setProgress] = useState<BatchProgress | null>(null);

  const addFromPath = useCallback(
    async (filePath: string): Promise<DocumentSummary | null> => {
      const summary = await ingest(filePath);
      if (summary) await attach(summary.id);
      return summary;
    },
    [ingest, attach],
  );

  /**
   * Add several documents as one batch. Files are parsed one at a time: parsing
   * (and OCR for scanned PDFs) is CPU-bound, so running them in parallel would
   * only make each one slower. A file that fails is counted and skipped; the
   * rest of the batch still goes in.
   */
  const addManyFromPaths = useCallback(
    async (filePaths: string[]): Promise<DocumentSummary[]> => {
      const added: DocumentSummary[] = [];
      if (filePaths.length === 0) return added;
      let failed = 0;
      setProgress({ done: 0, total: filePaths.length, failed: 0 });
      for (const [i, path] of filePaths.entries()) {
        const summary = await addFromPath(path);
        if (summary) added.push(summary);
        else failed += 1;
        setProgress({ done: i + 1, total: filePaths.length, failed });
      }
      setProgress(null);
      if (failed > 0) {
        console.warn(`${failed} of ${filePaths.length} documents could not be added.`);
      }
      return added;
    },
    [addFromPath],
  );

  /** Open the file picker and add every chosen document. */
  const pickAndAdd = useCallback(async (): Promise<DocumentSummary[]> => {
    try {
      return await addManyFromPaths(await openFileDialog());
    } catch (err) {
      console.warn('Failed to add documents:', err);
      return [];
    }
  }, [addManyFromPaths]);

  return {
    pickAndAdd,
    addFromPath,
    addManyFromPaths,
    isIngesting: isIngesting || progress !== null,
    progress,
  };
}

/** Button label while a batch is in flight, e.g. "Adding 2 of 5…". */
export function batchLabel(progress: BatchProgress | null, idle: string): string {
  if (!progress) return idle;
  if (progress.total === 1) return 'Adding…';
  return `Adding ${Math.min(progress.done + 1, progress.total)} of ${progress.total}…`;
}
