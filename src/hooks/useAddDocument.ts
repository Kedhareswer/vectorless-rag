import { useCallback } from 'react';
import { openFileDialog } from '../lib/tauri';
import { useDocumentsStore, type DocumentSummary } from '../stores/documents';
import { useChatStore } from '../stores/chat';

/**
 * The one way to add documents: ingest into the library, then attach to the
 * active chat (a chat is created if none is open).
 */
export function useAddDocument() {
  const ingest = useDocumentsStore((s) => s.ingestDocumentFromPath);
  const isIngesting = useDocumentsStore((s) => s.isIngesting);
  const attach = useChatStore((s) => s.addDocToActiveConversation);

  const addFromPath = useCallback(
    async (filePath: string): Promise<DocumentSummary | null> => {
      const summary = await ingest(filePath);
      if (summary) await attach(summary.id);
      return summary;
    },
    [ingest, attach],
  );

  /** Open the file picker and add the chosen document. */
  const pickAndAdd = useCallback(async (): Promise<DocumentSummary | null> => {
    try {
      const filePath = await openFileDialog();
      return filePath ? await addFromPath(filePath) : null;
    } catch (err) {
      console.warn('Failed to add document:', err);
      return null;
    }
  }, [addFromPath]);

  return { pickAndAdd, addFromPath, isIngesting };
}
