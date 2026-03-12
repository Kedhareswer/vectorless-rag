import { useState, useCallback, useEffect } from 'react';
import { FileText, Plus, X, Loader, Cpu, RefreshCw, Upload, Link2 } from 'lucide-react';
import clsx from 'clsx';
import { useDocumentsStore } from '../../stores/documents';
import { useChatStore } from '../../stores/chat';
import { useLocalModelStore } from '../../stores/localModel';
import { openFileDialog, getCrossDocRelations, reenrichDocument } from '../../lib/tauri';
import { TreeView } from './TreeView';
import styles from './DocsPanel.module.css';

export function DocsPanel() {
  const [isDragOver, setIsDragOver] = useState(false);
  const [reenrichState, setReenrichState] = useState<Record<string, 'idle' | 'running' | 'done'>>({});

  const {
    documents,
    activeDocumentId,
    isIngesting,
    error: docError,
    setActiveDocument,
    ingestDocumentFromPath,
  } = useDocumentsStore();

  const {
    activeConversationId,
    conversationDocIds,
    relationsVersion,
    addDocToActiveConversation,
    removeDocFromActiveConversation,
  } = useChatStore();

  const { status: localModelStatus } = useLocalModelStore();
  const localModelInstalled = localModelStatus?.downloaded ?? false;
  const localModelName = localModelStatus?.model_id ?? null;

  // Relation count
  const [relationCount, setRelationCount] = useState(0);
  useEffect(() => {
    if (conversationDocIds.length > 1) {
      getCrossDocRelations(conversationDocIds)
        .then((rels) => setRelationCount(rels.length))
        .catch(() => setRelationCount(0));
    } else {
      setRelationCount(0);
    }
  }, [conversationDocIds, relationsVersion]);

  const conversationDocs = conversationDocIds
    .map((id) => documents.find((d) => d.id === id))
    .filter((d): d is NonNullable<typeof d> => d != null);

  const handleAddDocument = async () => {
    try {
      const filePath = await openFileDialog();
      if (filePath) {
        const summary = await ingestDocumentFromPath(filePath);
        if (summary && activeConversationId) {
          await addDocToActiveConversation(summary.id);
        }
      }
    } catch (err) {
      console.warn('Failed to add document:', err);
    }
  };

  const handleRemoveDoc = async (e: React.MouseEvent, docId: string) => {
    e.stopPropagation();
    await removeDocFromActiveConversation(docId);
  };

  const handleReenrich = useCallback(async (e: React.MouseEvent, docId: string) => {
    e.stopPropagation();
    setReenrichState((prev) => ({ ...prev, [docId]: 'running' }));
    try {
      await reenrichDocument(docId);
      setReenrichState((prev) => ({ ...prev, [docId]: 'done' }));
    } catch {
      setReenrichState((prev) => ({ ...prev, [docId]: 'idle' }));
    }
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
  }, []);

  const handleDrop = useCallback(async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    const files = e.dataTransfer.files;
    for (let i = 0; i < files.length; i++) {
      const file = files[i];
      const filePath = (file as unknown as { path?: string }).path ?? file.name;
      if (filePath) {
        try {
          const summary = await ingestDocumentFromPath(filePath);
          if (summary && activeConversationId) {
            await addDocToActiveConversation(summary.id);
          }
        } catch (err) {
          console.warn('Failed to ingest dropped file:', err);
        }
      }
    }
  }, [ingestDocumentFromPath, activeConversationId, addDocToActiveConversation]);

  if (!activeConversationId) {
    return <p className={styles.emptyText}>Select or create a chat to manage its documents</p>;
  }

  return (
    <div
      className={clsx(isDragOver && styles.dropActive)}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {isDragOver && (
        <div className={styles.dropOverlay}>
          <Upload size={16} />
          <span>Drop files to add</span>
        </div>
      )}

      <div className={styles.section}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <span className={styles.sectionTitle}>Chat Documents</span>
          {relationCount > 0 && (
            <span className={styles.relationBadge}>
              <Link2 size={10} />
              {relationCount} relation{relationCount !== 1 ? 's' : ''}
            </span>
          )}
        </div>

        {docError && <p style={{ fontSize: 'var(--text-xs)', color: 'var(--error)', marginBottom: 'var(--space-3)' }}>{docError}</p>}

        <div className={styles.list}>
          {conversationDocs.length === 0 ? (
            <p className={styles.emptyText}>No documents in this chat</p>
          ) : (
            conversationDocs.map((doc) => (
              <div
                key={doc.id}
                className={clsx(styles.docItem, activeDocumentId === doc.id && styles.docItemActive)}
                onClick={() => setActiveDocument(doc.id)}
              >
                <FileText size={14} className={styles.docIcon} />
                <span className={styles.docName}>{doc.name}</span>
                <span className={styles.docType}>{doc.docType}</span>
                {localModelInstalled && (
                  <button
                    className={styles.docAction}
                    onClick={(e) => handleReenrich(e, doc.id)}
                    title={reenrichState[doc.id] === 'done' ? 'Re-enriched' : 'Re-enrich'}
                    type="button"
                    disabled={reenrichState[doc.id] === 'running'}
                  >
                    <RefreshCw size={12} className={reenrichState[doc.id] === 'running' ? styles.spinner : undefined} />
                  </button>
                )}
                <button
                  className={styles.docAction}
                  onClick={(e) => handleRemoveDoc(e, doc.id)}
                  title="Remove from chat"
                  type="button"
                >
                  <X size={12} />
                </button>
              </div>
            ))
          )}
        </div>

        <button
          type="button"
          className={styles.addBtn}
          onClick={handleAddDocument}
          disabled={isIngesting}
        >
          {isIngesting ? (
            <>
              <Loader size={14} className={styles.spinner} />
              <span>Ingesting...</span>
            </>
          ) : (
            <>
              <Plus size={14} />
              <span>Add Document</span>
            </>
          )}
        </button>

        {localModelInstalled && (
          <div className={clsx(styles.enrichmentStatus, styles.enrichmentActive)}>
            <span className={styles.activeDot} />
            <Cpu size={12} />
            <span>Enrichment: {localModelName ?? 'local model'}</span>
          </div>
        )}
      </div>

      {/* Document tree (if a doc is selected) */}
      {activeDocumentId && (
        <div className={styles.treeSection}>
          <span className={styles.sectionTitle}>Document Structure</span>
          <TreeView />
        </div>
      )}
    </div>
  );
}
