import { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import { Network } from 'lucide-react';
import { useDocumentsStore } from '../../stores/documents';
import { useChatStore } from '../../stores/chat';
import { getDocument } from '../../lib/tauri';
import type { DocumentTree, TreeNode } from '../../lib/tauri';
import sharedStyles from './PreviewPanel.module.css';
import styles from './CanvasView.module.css';

/** Color for each node type */
function getNodeColor(nodeType: string): string {
  switch (nodeType.toLowerCase()) {
    case 'root':
      return 'var(--accent)';
    case 'section':
    case 'heading':
      return '#3B82F6';
    case 'paragraph':
      return '#8B5CF6';
    case 'table':
      return '#10B981';
    case 'image':
      return '#F59E0B';
    case 'codeblock':
    case 'code_block':
    case 'code':
      return '#EC4899';
    case 'listitem':
    case 'list_item':
    case 'list':
      return '#06B6D4';
    case 'link':
      return '#14B8A6';
    default:
      return 'var(--text-tertiary)';
  }
}

/** Truncate for labels */
function truncateLabel(text: string, max: number): string {
  const cleaned = text.replace(/\s+/g, ' ').trim();
  if (!cleaned) return '';
  if (cleaned.length <= max) return cleaned;
  return cleaned.slice(0, max) + '...';
}

interface LayoutNode {
  id: string;
  x: number;
  y: number;
  radius: number;
  node: TreeNode;
  isRoot: boolean;
}

interface TooltipData {
  x: number;
  y: number;
  nodeType: string;
  content: string;
}

export function CanvasView() {
  const activeDocumentId = useDocumentsStore((s) => s.activeDocumentId);
  const visitedNodeIds = useChatStore((s) => s.visitedNodeIds);
  const activeNodeId = useChatStore((s) => s.activeNodeId);
  const visitedSet = useMemo(() => new Set(visitedNodeIds), [visitedNodeIds]);
  const [tree, setTree] = useState<DocumentTree | null>(null);
  const [tooltip, setTooltip] = useState<TooltipData | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Load tree when active document changes
  useEffect(() => {
    if (!activeDocumentId) {
      setTree(null);
      return;
    }

    let cancelled = false;

    getDocument(activeDocumentId)
      .then((doc) => {
        if (!cancelled) setTree(doc);
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          console.warn('CanvasView: failed to load document', err);
          setTree(null);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [activeDocumentId]);

  // Build layout: root in center, direct children in a ring
  const { layoutNodes, edges, viewBox } = useMemo(() => {
    if (!tree) {
      return { layoutNodes: [], edges: [], viewBox: '0 0 400 400' };
    }

    const rootNode = tree.nodes[tree.root_id];
    if (!rootNode) {
      return { layoutNodes: [], edges: [], viewBox: '0 0 400 400' };
    }

    // If root has only 1 child, use that child as the visual center
    // (e.g. Markdown with a single top-level heading containing all sections)
    let centerNode = rootNode;
    let centerId = tree.root_id;
    if (rootNode.children.length === 1) {
      const singleChild = tree.nodes[rootNode.children[0]];
      if (singleChild && singleChild.children.length > 0) {
        centerNode = singleChild;
        centerId = singleChild.id;
      }
    }

    const cx = 200;
    const cy = 200;
    const rootRadius = 24;
    const childRadius = 14;

    // Limit to first 24 direct children to keep the graph readable
    const childIds = centerNode.children.slice(0, 24);
    const childCount = childIds.length;

    // Ring radius scales with child count
    const ringRadius = Math.max(80, Math.min(160, 40 + childCount * 10));

    const nodes: LayoutNode[] = [];
    const edgeList: { x1: number; y1: number; x2: number; y2: number }[] = [];

    // Center node
    nodes.push({
      id: centerId,
      x: cx,
      y: cy,
      radius: rootRadius,
      node: centerNode,
      isRoot: true,
    });

    // Place children in a circle
    childIds.forEach((childId, i) => {
      const childNode = tree.nodes[childId];
      if (!childNode) return;

      const angle = (2 * Math.PI * i) / childCount - Math.PI / 2;
      const x = cx + ringRadius * Math.cos(angle);
      const y = cy + ringRadius * Math.sin(angle);

      nodes.push({
        id: childId,
        x,
        y,
        radius: childRadius,
        node: childNode,
        isRoot: false,
      });

      edgeList.push({ x1: cx, y1: cy, x2: x, y2: y });
    });

    // Calculate viewBox with padding
    const padding = 40;
    const minX = Math.min(...nodes.map((n) => n.x - n.radius)) - padding;
    const minY = Math.min(...nodes.map((n) => n.y - n.radius)) - padding;
    const maxX = Math.max(...nodes.map((n) => n.x + n.radius)) + padding;
    const maxY = Math.max(...nodes.map((n) => n.y + n.radius)) + padding;
    const vb = `${minX} ${minY} ${maxX - minX} ${maxY - minY}`;

    return { layoutNodes: nodes, edges: edgeList, viewBox: vb };
  }, [tree]);

  const handleNodeHover = useCallback(
    (layoutNode: LayoutNode, event: React.MouseEvent) => {
      const container = containerRef.current;
      if (!container) return;

      const rect = container.getBoundingClientRect();
      const x = event.clientX - rect.left + 12;
      const y = event.clientY - rect.top + 12;

      setTooltip({
        x,
        y,
        nodeType: layoutNode.node.node_type,
        content: truncateLabel(
          layoutNode.node.content || layoutNode.node.summary || '',
          200,
        ),
      });
    },
    [],
  );

  const handleNodeLeave = useCallback(() => {
    setTooltip(null);
  }, []);

  // Empty state
  if (!activeDocumentId || !tree) {
    return (
      <div className={sharedStyles.placeholder}>
        <Network size={32} className={sharedStyles.placeholderIcon} />
        <p className={sharedStyles.placeholderText}>
          {activeDocumentId
            ? 'Loading document graph...'
            : 'Select a document to explore'}
        </p>
        <p className={sharedStyles.placeholderHint}>
          Explore document relationships visually
        </p>
      </div>
    );
  }

  if (layoutNodes.length === 0) {
    return (
      <div className={sharedStyles.placeholder}>
        <Network size={32} className={sharedStyles.placeholderIcon} />
        <p className={sharedStyles.placeholderText}>No nodes to display</p>
      </div>
    );
  }

  return (
    <div className={styles.container} ref={containerRef}>
      <svg
        ref={svgRef}
        className={styles.svgCanvas}
        viewBox={viewBox}
        preserveAspectRatio="xMidYMid meet"
      >
        {/* Edges */}
        {edges.map((edge, i) => (
          <line
            key={i}
            x1={edge.x1}
            y1={edge.y1}
            x2={edge.x2}
            y2={edge.y2}
            className={styles.edge}
          />
        ))}

        {/* Nodes */}
        {layoutNodes.map((ln) => {
          const isVisited = visitedSet.has(ln.id);
          const isActive = activeNodeId === ln.id;
          return (
            <g
              key={ln.id}
              className={styles.node}
              onMouseMove={(e) => handleNodeHover(ln, e)}
              onMouseLeave={handleNodeLeave}
            >
              {/* Active node animated ring */}
              {isActive && (
                <circle
                  cx={ln.x}
                  cy={ln.y}
                  r={ln.radius + 4}
                  fill="none"
                  stroke="var(--accent)"
                  strokeWidth={2}
                  className={styles.activeRing}
                />
              )}
              <circle
                cx={ln.x}
                cy={ln.y}
                r={ln.radius}
                fill={isVisited ? 'var(--accent)' : getNodeColor(ln.node.node_type)}
                opacity={ln.isRoot ? 1 : isVisited ? 0.9 : 0.7}
                stroke={isVisited ? 'var(--accent)' : getNodeColor(ln.node.node_type)}
                strokeWidth={ln.isRoot ? 2 : isVisited ? 2 : 1}
              />
              <text
                x={ln.x}
                y={ln.isRoot ? ln.y + 4 : ln.y + ln.radius + 12}
                className={
                  ln.isRoot ? styles.nodeLabelRoot : styles.nodeLabel
                }
                textAnchor="middle"
              >
                {ln.isRoot
                  ? truncateLabel(ln.node.content || tree.name, 12)
                  : truncateLabel(
                      ln.node.content || ln.node.node_type,
                      10,
                    )}
              </text>
            </g>
          );
        })}
      </svg>

      {tooltip && (
        <div
          className={styles.tooltip}
          style={{ left: tooltip.x, top: tooltip.y }}
        >
          <div className={styles.tooltipType}>{tooltip.nodeType}</div>
          <div className={styles.tooltipContent}>
            {tooltip.content || '(empty)'}
          </div>
        </div>
      )}
    </div>
  );
}
