import { useState, useEffect, useMemo, useCallback } from 'react';
import {
  Search,
  ChevronRight,
  ChevronsUpDown,
  FileText,
  AlignLeft,
  Hash,
  Table,
  Image,
  Code,
  List,
  Link,
  FolderOpen,
  GitBranch,
  File,
  X,
} from 'lucide-react';
import clsx from 'clsx';
import { useDocumentsStore } from '../../stores/documents';
import { useChatStore } from '../../stores/chat';
import { getDocument } from '../../lib/tauri';
import type { DocumentTree, TreeNode } from '../../lib/tauri';
import sharedStyles from './shared.module.css';
import styles from './TreeView.module.css';

/** Map node_type strings to lucide icons */
function getNodeIcon(nodeType: string) {
  switch (nodeType.toLowerCase()) {
    case 'section':
      return FileText;
    case 'paragraph':
      return AlignLeft;
    case 'heading':
      return Hash;
    case 'table':
      return Table;
    case 'image':
      return Image;
    case 'codeblock':
    case 'code_block':
    case 'code':
      return Code;
    case 'listitem':
    case 'list_item':
    case 'list':
      return List;
    case 'link':
      return Link;
    case 'root':
      return FolderOpen;
    default:
      return File;
  }
}

/** Truncate content for display */
function truncate(text: string, max: number): string {
  const cleaned = text.replace(/\s+/g, ' ').trim();
  if (cleaned.length <= max) return cleaned;
  return cleaned.slice(0, max) + '...';
}

interface TreeNodeItemProps {
  nodeId: string;
  nodes: Record<string, TreeNode>;
  depth: number;
  exploredNodeIds: Set<string>;
  visitedNodeIds: Set<string>;
  activeNodeId: string | null;
  filterText: string;
  visibleIds: Set<string> | null;
  selectedNodeId: string | null;
  onSelectNode: (id: string) => void;
  expandedIds: Set<string>;
  onToggleExpand: (id: string) => void;
}

function TreeNodeItem({
  nodeId,
  nodes,
  depth,
  exploredNodeIds,
  visitedNodeIds,
  activeNodeId,
  filterText,
  visibleIds,
  selectedNodeId,
  onSelectNode,
  expandedIds,
  onToggleExpand,
}: TreeNodeItemProps) {
  const expanded = expandedIds.has(nodeId);
  const node = nodes[nodeId];

  if (!node) return null;

  // If filtering is active and this node is not in the visible set, skip it
  if (visibleIds && !visibleIds.has(nodeId)) return null;

  const hasChildren = node.children.length > 0;
  const isExplored = exploredNodeIds.has(nodeId);
  const isVisited = visitedNodeIds.has(nodeId);
  const isActive = activeNodeId === nodeId;
  const Icon = getNodeIcon(node.node_type);
  const preview = node.content
    ? truncate(node.content, 80)
    : node.summary
      ? truncate(node.summary, 80)
      : `(${node.node_type})`;

  // Auto-expand if a descendant is the active node
  useEffect(() => {
    if (activeNodeId && hasChildren && !expanded) {
      const hasActiveDescendant = (nId: string): boolean => {
        const n = nodes[nId];
        if (!n) return false;
        return n.children.some((cId) => cId === activeNodeId || hasActiveDescendant(cId));
      };
      if (hasActiveDescendant(nodeId)) {
        onToggleExpand(nodeId);
      }
    }
  }, [activeNodeId, nodeId, nodes, hasChildren, expanded, onToggleExpand]);

  const isSelected = selectedNodeId === nodeId;

  const handleClick = () => {
    onSelectNode(nodeId);
    if (hasChildren) {
      onToggleExpand(nodeId);
    }
  };

  return (
    <li
      role="treeitem"
      aria-expanded={hasChildren ? expanded : undefined}
    >
      <div
        className={clsx(
          styles.treeItem,
          expanded && hasChildren && styles.treeItemExpanded,
          isExplored && styles.treeItemExplored,
          isVisited && styles.treeItemVisited,
          isActive && styles.treeItemActive,
          isSelected && styles.treeItemSelected,
        )}
        style={{ paddingLeft: `${8 + depth * 20}px` }}
        onClick={handleClick}
      >
        <span
          className={clsx(
            styles.expandArrow,
            expanded && styles.expandArrowExpanded,
            !hasChildren && styles.expandArrowHidden,
          )}
        >
          <ChevronRight size={12} />
        </span>
        <Icon size={14} className={clsx(styles.nodeIcon, isVisited && styles.nodeIconVisited)} />
        <span className={styles.nodeContent} title={node.content || undefined}>
          {preview}
        </span>
        {hasChildren && (
          <span className={styles.badge}>{node.children.length}</span>
        )}
      </div>

      {expanded && hasChildren && (
        <ul role="group">
          {node.children.map((childId) => (
            <TreeNodeItem
              key={childId}
              nodeId={childId}
              nodes={nodes}
              depth={depth + 1}
              exploredNodeIds={exploredNodeIds}
              visitedNodeIds={visitedNodeIds}
              activeNodeId={activeNodeId}
              filterText={filterText}
              visibleIds={visibleIds}
              selectedNodeId={selectedNodeId}
              onSelectNode={onSelectNode}
              expandedIds={expandedIds}
              onToggleExpand={onToggleExpand}
            />
          ))}
        </ul>
      )}
    </li>
  );
}

/** Metadata detail card for a selected node. */
function NodeMetadataCard({ node }: { node: TreeNode }) {
  const metadata = node.metadata || {};
  const summary = node.summary || null;
  const entities: string[] = metadata.entities
    ? (Array.isArray(metadata.entities) ? metadata.entities : [])
    : [];
  const topics: string[] = metadata.topics
    ? (Array.isArray(metadata.topics) ? metadata.topics : [])
    : [];
  const headingSource = metadata.heading_source as string | undefined;
  const pageNumber = metadata.page_number as number | undefined;
  const wordCount = metadata.word_count as number | undefined;

  // Determine enrichment source
  const hasSummary = !!summary;
  const hasEntities = entities.length > 0;
  const hasTopics = topics.length > 0;
  const hasAnyMetadata = hasSummary || hasEntities || hasTopics;

  return (
    <div className={styles.metadataCard}>
      <div className={styles.metadataHeader}>
        <span className={styles.metadataTitle}>
          {truncate(node.content || node.node_type, 50)}
        </span>
        {headingSource && (
          <span className={clsx(styles.sourceBadge, headingSource === 'slm' ? styles.sourceBadgeSlm : styles.sourceBadgeHeuristic)}>
            {headingSource === 'slm' ? 'SLM' : 'Heuristic'}
          </span>
        )}
      </div>

      {hasSummary && (
        <div className={styles.metadataRow}>
          <span className={styles.metadataLabel}>Summary</span>
          <span className={styles.metadataValue}>{summary}</span>
        </div>
      )}

      {hasEntities && (
        <div className={styles.metadataRow}>
          <span className={styles.metadataLabel}>Entities</span>
          <div className={styles.tagList}>
            {entities.map((e, i) => (
              <span key={i} className={styles.tag}>{e}</span>
            ))}
          </div>
        </div>
      )}

      {hasTopics && (
        <div className={styles.metadataRow}>
          <span className={styles.metadataLabel}>Topics</span>
          <div className={styles.tagList}>
            {topics.map((t, i) => (
              <span key={i} className={clsx(styles.tag, styles.tagTopic)}>{t}</span>
            ))}
          </div>
        </div>
      )}

      <div className={styles.metadataMeta}>
        {pageNumber && <span>Page {pageNumber}</span>}
        {wordCount && <span>{wordCount} words</span>}
        {!hasAnyMetadata && <span className={styles.metadataEmpty}>No metadata available</span>}
      </div>
    </div>
  );
}

export function TreeView() {
  const activeDocumentId = useDocumentsStore((s) => s.activeDocumentId);
  const enrichmentStatus = useDocumentsStore((s) => s.enrichmentStatus);
  const explorationSteps = useChatStore((s) => s.explorationSteps);
  const visitedNodeIdsArr = useChatStore((s) => s.visitedNodeIds);
  const activeNodeId = useChatStore((s) => s.activeNodeId);

  const enriching = activeDocumentId
    ? enrichmentStatus[activeDocumentId]
    : undefined;
  const isEnriching = enriching?.status === 'started';

  const visitedNodeIdsSet = useMemo(() => new Set(visitedNodeIdsArr), [visitedNodeIdsArr]);

  const [tree, setTree] = useState<DocumentTree | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filterText, setFilterText] = useState('');
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());

  const toggleExpand = useCallback((id: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  // Collect all node IDs that have children (for expand/collapse all)
  const allExpandableIds = useMemo(() => {
    if (!tree) return new Set<string>();
    const ids = new Set<string>();
    for (const [id, node] of Object.entries(tree.nodes)) {
      if (node.children.length > 0) {
        ids.add(id);
      }
    }
    return ids;
  }, [tree]);

  const expandAll = useCallback(() => {
    setExpandedIds(new Set(allExpandableIds));
  }, [allExpandableIds]);

  const collapseAll = useCallback(() => {
    setExpandedIds(new Set());
  }, []);

  // Load tree when active document changes
  useEffect(() => {
    if (!activeDocumentId) {
      setTree(null);
      setError(null);
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);

    getDocument(activeDocumentId)
      .then((doc) => {
        if (!cancelled) {
          setTree(doc);
          setLoading(false);
          // Auto-expand root's direct children
          const root = doc.nodes[doc.root_id];
          if (root) {
            setExpandedIds(new Set(root.children.filter((id) => doc.nodes[id]?.children.length > 0)));
          }
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          const message =
            err instanceof Error ? err.message : String(err);
          console.warn('Failed to load document tree:', message);
          setError(message);
          setTree(null);
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [activeDocumentId]);

  // Build set of explored node IDs from exploration steps
  const exploredNodeIds = useMemo(() => {
    const ids = new Set<string>();
    for (const step of explorationSteps) {
      // Check inputSummary and outputSummary for node IDs
      // The input/output may reference node IDs via expand_node calls
      if (step.tool === 'expand_node' && step.inputSummary) {
        ids.add(step.inputSummary);
      }
    }
    return ids;
  }, [explorationSteps]);

  // Compute which nodes match the filter (and their ancestors)
  const visibleIds = useMemo(() => {
    if (!filterText.trim() || !tree) return null;

    const lower = filterText.toLowerCase();
    const matchSet = new Set<string>();

    // Find all matching nodes
    for (const [id, node] of Object.entries(tree.nodes)) {
      const text = (node.content || '') + ' ' + (node.summary || '') + ' ' + node.node_type;
      if (text.toLowerCase().includes(lower)) {
        matchSet.add(id);
      }
    }

    // Walk up from each match to mark all ancestors visible
    const allVisible = new Set<string>(matchSet);
    const addAncestors = (nodeId: string) => {
      // find parent by checking which node has this as a child
      for (const [parentId, parentNode] of Object.entries(tree.nodes)) {
        if (parentNode.children.includes(nodeId) && !allVisible.has(parentId)) {
          allVisible.add(parentId);
          addAncestors(parentId);
        }
      }
    };

    for (const id of matchSet) {
      addAncestors(id);
    }

    return allVisible;
  }, [filterText, tree]);

  const handleFilterChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      setFilterText(e.target.value);
    },
    [],
  );

  // Empty state: no document selected
  if (!activeDocumentId) {
    return (
      <div className={sharedStyles.placeholder}>
        <GitBranch size={32} className={sharedStyles.placeholderIcon} />
        <p className={sharedStyles.placeholderText}>
          Select a document to view its structure
        </p>
        <p className={sharedStyles.placeholderHint}>
          Ingest a document to visualize its tree
        </p>
      </div>
    );
  }

  if (loading) {
    return (
      <div className={sharedStyles.placeholder}>
        <GitBranch size={32} className={sharedStyles.placeholderIcon} />
        <p className={sharedStyles.placeholderText}>Loading tree...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className={sharedStyles.placeholder}>
        <GitBranch size={32} className={sharedStyles.placeholderIcon} />
        <p className={sharedStyles.placeholderText}>
          Could not load document tree
        </p>
        <p className={sharedStyles.placeholderHint}>{error}</p>
      </div>
    );
  }

  if (!tree) {
    return (
      <div className={sharedStyles.placeholder}>
        <GitBranch size={32} className={sharedStyles.placeholderIcon} />
        <p className={sharedStyles.placeholderText}>No tree data available</p>
      </div>
    );
  }

  const rootNode = tree.nodes[tree.root_id];
  if (!rootNode) {
    return (
      <div className={sharedStyles.placeholder}>
        <GitBranch size={32} className={sharedStyles.placeholderIcon} />
        <p className={sharedStyles.placeholderText}>Invalid tree structure</p>
      </div>
    );
  }

  const nodeCount = Object.keys(tree.nodes).length - 1; // exclude root

  return (
    <div className={styles.container}>
      <div className={styles.searchWrapper}>
        <Search size={14} className={styles.searchIcon} />
        <input
          className={styles.searchInput}
          type="text"
          placeholder="Filter nodes..."
          value={filterText}
          onChange={handleFilterChange}
        />
      </div>

      <div className={styles.toolbar}>
        <span className={styles.toolbarLabel}>
          {nodeCount} nodes
          {isEnriching && <span className={styles.enrichingLabel}> · Enriching metadata...</span>}
        </span>
        <button className={styles.toolbarBtn} onClick={expandAll} title="Expand all">
          <ChevronsUpDown size={13} />
        </button>
        <button className={styles.toolbarBtn} onClick={collapseAll} title="Collapse all">
          <ChevronRight size={13} />
        </button>
      </div>

      <ul className={styles.treeList} role="tree">
        {rootNode.children.map((childId) => (
          <TreeNodeItem
            key={childId}
            nodeId={childId}
            nodes={tree.nodes}
            depth={0}
            exploredNodeIds={exploredNodeIds}
            visitedNodeIds={visitedNodeIdsSet}
            activeNodeId={activeNodeId}
            filterText={filterText}
            visibleIds={visibleIds}
            selectedNodeId={selectedNodeId}
            onSelectNode={setSelectedNodeId}
            expandedIds={expandedIds}
            onToggleExpand={toggleExpand}
          />
        ))}
      </ul>

      {selectedNodeId && tree.nodes[selectedNodeId] && (
        <div className={styles.metadataCardWrapper}>
          <button
            className={styles.metadataClose}
            onClick={() => setSelectedNodeId(null)}
            title="Close metadata"
          >
            <X size={12} />
          </button>
          <NodeMetadataCard node={tree.nodes[selectedNodeId]} />
        </div>
      )}
    </div>
  );
}
