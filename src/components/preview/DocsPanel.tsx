import { useState, useCallback, useEffect } from 'react';
import { FileText, Plus, X, Loader, Cpu, RefreshCw, Upload, Link2 } from 'lucide-react';
import clsx from 'clsx';
import { useDocumentsStore } from '../../stores/documents';
import { useChatStore } from '../../stores/chat';
import { useLocalModelStore } from '../../stores/localModel';
import { getCrossDocRelations, reenrichDocument, reenrichDocuments } from '../../lib/tauri';
import { useAddDocument, batchLabel } from '../../hooks/useAddDocument';
import { TreeView } from './TreeView';
import styles from './DocsPanel.module.css';

export function DocsPanel() {
  const [isDragOver, setIsDragOver] = useState(false);
  const [reenrichState, setReenrichState] = useState<Record<string, 'idle' | 'running' | 'done'>>({});

  const {
    documents,
    activeDocumentId,
    error: docError,
    setActiveDocument,
  } = useDocumentsStore();
  const { pickAndAdd, addManyFromPaths, isIngesting, progress } = useAddDocument();

  const {
    activeConversationId,
    conversationDocIds,
    relationsVersion,
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

  const handleAddDocument = () => void pickAndAdd();

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

  /** Re-enrich every document in this chat as one batch. */
  const handleReenrichAll = useCallback(async () => {
    const ids = conversationDocIds;
    if (ids.length === 0) return;
    setReenrichState((prev) => ({ ...prev, ...Object.fromEntries(ids.map((id) => [id, 'running'])) }));
    try {
      const results = await reenrichDocuments(ids);
      setReenrichState((prev) => ({
        ...prev,
        ...Object.fromEntries(results.map((r) => [r.doc_id, r.error ? 'idle' : 'done'])),
      }));
    } catch {
      setReenrichState((prev) => ({ ...prev, ...Object.fromEntries(ids.map((id) => [id, 'idle'])) }));
    }
  }, [conversationDocIds]);

  const isBatchEnriching = conversationDocIds.some((id) => reenrichState[id] === 'running');

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
    const paths = Array.from(e.dataTransfer.files)
      .map((file) => (file as unknown as { path?: string }).path ?? file.name)
      .filter(Boolean);
    await addManyFromPaths(paths);
  }, [addManyFromPaths]);

  if (!activeConversationId) {
    return (
      <div>
        <p className={styles.emptyText}>No chat is open. Adding a document starts a new chat.</p>
        <button type="button" className={styles.addBtn} onClick={handleAddDocument} disabled={isIngesting}>
          <Plus size={14} />
          <span>Add Document</span>
        </button>
      </div>
    );
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
          {conversationDocIds.length > 1 && (
            <button
              type="button"
              className={styles.docAction}
              onClick={() => void handleReenrichAll()}
              disabled={isBatchEnriching}
              title="Re-enrich all documents in this chat"
            >
              <RefreshCw size={12} className={isBatchEnriching ? styles.spinner : undefined} />
            </button>
          )}
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
              <span>{batchLabel(progress, 'Ingesting...')}</span>
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
