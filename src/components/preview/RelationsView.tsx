import { useState, useEffect } from 'react';
import { Link2 } from 'lucide-react';
import clsx from 'clsx';
import { useDocumentsStore } from '../../stores/documents';
import { useChatStore } from '../../stores/chat';
import { getCrossDocRelations, type CrossDocRelation } from '../../lib/tauri';
import sharedStyles from './shared.module.css';
import styles from './RelationsView.module.css';

const TYPE_LABELS: Record<string, string> = {
  shared_entity: 'Shared Entity',
  SharedEntity: 'Shared Entity',
  topic_overlap: 'Topic Overlap',
  TopicOverlap: 'Topic Overlap',
  contradiction: 'Contradiction',
  Contradiction: 'Contradiction',
  supports: 'Supports',
  Supports: 'Supports',
  references: 'References',
  References: 'References',
};

function confidenceClass(c: number): string {
  if (c >= 0.7) return styles.confidenceHigh;
  if (c >= 0.4) return styles.confidenceMed;
  return styles.confidenceLow;
}

export function RelationsView() {
  const conversationDocIds = useChatStore((s) => s.conversationDocIds);
  const relationsVersion = useChatStore((s) => s.relationsVersion);
  const activeDocumentId = useDocumentsStore((s) => s.activeDocumentId);
  const [relations, setRelations] = useState<CrossDocRelation[]>([]);

  const docIds = conversationDocIds.length > 0
    ? conversationDocIds
    : activeDocumentId ? [activeDocumentId] : [];

  // Re-fetch when doc IDs change OR when a query completes (relationsVersion bumps)
  useEffect(() => {
    if (docIds.length === 0) {
      setRelations([]);
      return;
    }
    let cancelled = false;
    getCrossDocRelations(docIds)
      .then((rels) => { if (!cancelled) setRelations(rels); })
      .catch(() => { if (!cancelled) setRelations([]); });
    return () => { cancelled = true; };
  }, [docIds.join(','), relationsVersion]);

  if (relations.length === 0) {
    return (
      <div className={sharedStyles.placeholder}>
        <Link2 size={24} className={sharedStyles.placeholderIcon} />
        <p className={sharedStyles.placeholderText}>No relations discovered yet</p>
        <p className={sharedStyles.placeholderHint}>
          Relations are recorded as the agent explores documents
        </p>
      </div>
    );
  }

  // Group by relation type
  const grouped = new Map<string, CrossDocRelation[]>();
  for (const rel of relations) {
    const existing = grouped.get(rel.relation_type) ?? [];
    existing.push(rel);
    grouped.set(rel.relation_type, existing);
  }

  return (
    <div className={styles.container}>
      {Array.from(grouped.entries()).map(([type, rels]) => (
        <div key={type} className={styles.group}>
          <div className={styles.groupLabel}>
            {TYPE_LABELS[type] ?? type} ({rels.length})
          </div>
          {rels.map((rel) => (
            <div key={rel.id} className={styles.relation}>
              <div className={styles.relationHeader}>
                <span className={clsx(styles.confidenceDot, confidenceClass(rel.confidence))} />
                <span className={styles.typeBadge}>{TYPE_LABELS[rel.relation_type] ?? rel.relation_type}</span>
                <span style={{ fontSize: '0.7rem', color: 'var(--text-tertiary)' }}>
                  {Math.round(rel.confidence * 100)}%
                </span>
              </div>
              <div className={styles.nodeIds}>
                {rel.source_node_id.slice(0, 8)}… → {rel.target_node_id.slice(0, 8)}…
              </div>
              {rel.description && (
                <div className={styles.description}>{rel.description}</div>
              )}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}
