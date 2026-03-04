import { useState, useEffect, useMemo, useCallback } from 'react';
import {
  Search,
  ChevronRight,
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
} from 'lucide-react';
import clsx from 'clsx';
import { useDocumentsStore } from '../../stores/documents';
import { useChatStore } from '../../stores/chat';
import { getDocument } from '../../lib/tauri';
import type { DocumentTree, TreeNode } from '../../lib/tauri';
import sharedStyles from './PreviewPanel.module.css';
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
}: TreeNodeItemProps) {
  const [expanded, setExpanded] = useState(depth === 0);
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
    if (activeNodeId && hasChildren) {
      const hasActiveDescendant = (nId: string): boolean => {
        const n = nodes[nId];
        if (!n) return false;
        return n.children.some((cId) => cId === activeNodeId || hasActiveDescendant(cId));
      };
      if (hasActiveDescendant(nodeId)) {
        setExpanded(true);
      }
    }
  }, [activeNodeId, nodeId, nodes, hasChildren]);

  const handleClick = () => {
    if (hasChildren) {
      setExpanded((prev) => !prev);
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
            />
          ))}
        </ul>
      )}
    </li>
  );
}

export function TreeView() {
  const activeDocumentId = useDocumentsStore((s) => s.activeDocumentId);
  const explorationSteps = useChatStore((s) => s.explorationSteps);
  const visitedNodeIdsArr = useChatStore((s) => s.visitedNodeIds);
  const activeNodeId = useChatStore((s) => s.activeNodeId);

  const visitedNodeIdsSet = useMemo(() => new Set(visitedNodeIdsArr), [visitedNodeIdsArr]);

  const [tree, setTree] = useState<DocumentTree | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filterText, setFilterText] = useState('');

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
          />
        ))}
      </ul>
    </div>
  );
}
