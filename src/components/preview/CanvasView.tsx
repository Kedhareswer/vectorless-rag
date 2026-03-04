import { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import { Network, ZoomIn, ZoomOut, Maximize2 } from 'lucide-react';
import { useDocumentsStore } from '../../stores/documents';
import { useChatStore } from '../../stores/chat';
import { getDocument } from '../../lib/tauri';
import type { DocumentTree, TreeNode } from '../../lib/tauri';
import sharedStyles from './PreviewPanel.module.css';
import styles from './CanvasView.module.css';

/** Color for each node type */
function getNodeColor(nodeType: string): string {
  switch (nodeType.toLowerCase()) {
    case 'root': return 'var(--accent)';
    case 'section': case 'heading': return '#3B82F6';
    case 'paragraph': return '#8B5CF6';
    case 'table': return '#10B981';
    case 'image': return '#F59E0B';
    case 'codeblock': case 'code_block': case 'code': return '#EC4899';
    case 'listitem': case 'list_item': case 'list': return '#06B6D4';
    case 'link': return '#14B8A6';
    default: return 'var(--text-tertiary)';
  }
}

function truncateLabel(text: string, max: number): string {
  const cleaned = text.replace(/\s+/g, ' ').trim();
  if (!cleaned) return '';
  if (cleaned.length <= max) return cleaned;
  return cleaned.slice(0, max) + '…';
}

interface LayoutNode {
  id: string;
  x: number;
  y: number;
  node: TreeNode;
  depth: number;
}

interface TooltipData {
  x: number;
  y: number;
  nodeType: string;
  content: string;
}

// Layout constants — larger spacing so nodes are readable at natural size
const NODE_R = 12;
const ROOT_R = 18;
const H_GAP = 70;
const V_GAP = 80;

function computeLayout(
  tree: DocumentTree,
  rootId: string,
  maxDepth = 3,
): { positions: Map<string, { x: number; y: number }>; bounds: { minX: number; minY: number; maxX: number; maxY: number } } {
  const positions = new Map<string, { x: number; y: number }>();

  function subtreeWidth(nodeId: string, depth: number): number {
    if (depth >= maxDepth) return H_GAP;
    const node = tree.nodes[nodeId];
    if (!node || node.children.length === 0) return H_GAP;
    return node.children.slice(0, 10).reduce((sum, cId) => sum + subtreeWidth(cId, depth + 1), 0);
  }

  function placeNode(nodeId: string, depth: number, leftX: number): void {
    const node = tree.nodes[nodeId];
    if (!node) return;

    const y = depth * V_GAP + ROOT_R + 8;
    const visibleChildren = depth < maxDepth ? node.children.slice(0, 10) : [];

    if (visibleChildren.length === 0) {
      positions.set(nodeId, { x: leftX + H_GAP / 2, y });
      return;
    }

    let childLeft = leftX;
    for (const childId of visibleChildren) {
      placeNode(childId, depth + 1, childLeft);
      childLeft += subtreeWidth(childId, depth + 1);
    }

    const firstChild = positions.get(visibleChildren[0]);
    const lastChild = positions.get(visibleChildren[visibleChildren.length - 1]);
    const cx = firstChild && lastChild
      ? (firstChild.x + lastChild.x) / 2
      : leftX + H_GAP / 2;
    positions.set(nodeId, { x: cx, y });
  }

  placeNode(rootId, 0, 0);

  const allX = Array.from(positions.values()).map((p) => p.x);
  const allY = Array.from(positions.values()).map((p) => p.y);
  const pad = 40;
  return {
    positions,
    bounds: {
      minX: Math.min(...allX) - pad,
      minY: Math.min(...allY) - pad,
      maxX: Math.max(...allX) + pad,
      maxY: Math.max(...allY) + pad + 20,
    },
  };
}

export function CanvasView() {
  const activeDocumentId = useDocumentsStore((s) => s.activeDocumentId);
  const visitedNodeIds = useChatStore((s) => s.visitedNodeIds);
  const activeNodeId = useChatStore((s) => s.activeNodeId);
  const visitedSet = useMemo(() => new Set(visitedNodeIds), [visitedNodeIds]);
  const [tree, setTree] = useState<DocumentTree | null>(null);
  const [tooltip, setTooltip] = useState<TooltipData | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Transform state: zoom + pan in SVG-coordinate space
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [isPanning, setIsPanning] = useState(false);
  const dragStart = useRef({ mouseX: 0, mouseY: 0, panX: 0, panY: 0 });

  // Load tree
  useEffect(() => {
    if (!activeDocumentId) { setTree(null); return; }
    let cancelled = false;
    getDocument(activeDocumentId)
      .then((doc) => { if (!cancelled) setTree(doc); })
      .catch((err: unknown) => { if (!cancelled) { console.warn('CanvasView:', err); setTree(null); } });
    return () => { cancelled = true; };
  }, [activeDocumentId]);

  // Build layout
  const { layoutNodes, edges, treeBounds } = useMemo(() => {
    if (!tree) return { layoutNodes: [], edges: [], treeBounds: null };

    const rootNode = tree.nodes[tree.root_id];
    if (!rootNode) return { layoutNodes: [], edges: [], treeBounds: null };

    let rootId = tree.root_id;
    if (rootNode.children.length === 1) {
      const child = tree.nodes[rootNode.children[0]];
      if (child && child.children.length > 0) rootId = child.id;
    }

    const { positions, bounds } = computeLayout(tree, rootId, 3);

    const nodes: LayoutNode[] = [];
    const edgeList: { x1: number; y1: number; x2: number; y2: number }[] = [];
    const treeRef = tree;

    function collect(nodeId: string, depth: number): void {
      const pos = positions.get(nodeId);
      const node = treeRef.nodes[nodeId];
      if (!pos || !node) return;
      nodes.push({ id: nodeId, x: pos.x, y: pos.y, node, depth });
      const children = depth < 3 ? node.children.slice(0, 10) : [];
      for (const cId of children) {
        const cp = positions.get(cId);
        if (cp) edgeList.push({ x1: pos.x, y1: pos.y, x2: cp.x, y2: cp.y });
        collect(cId, depth + 1);
      }
    }
    collect(rootId, 0);

    return { layoutNodes: nodes, edges: edgeList, treeBounds: bounds };
  }, [tree]);

  // Auto-fit: compute zoom so the tree fills the panel at load
  const fitView = useCallback(() => {
    const container = containerRef.current;
    if (!container || !treeBounds) return;
    const { width, height } = container.getBoundingClientRect();
    if (width === 0 || height === 0) return;
    const treeW = treeBounds.maxX - treeBounds.minX;
    const treeH = treeBounds.maxY - treeBounds.minY;
    // Fit with a bit of margin
    const fitZoom = Math.min(width / treeW, height / treeH) * 0.92;
    // Clamp: don't go above 1.5x on small trees, don't go below a minimum
    const clamped = Math.min(Math.max(fitZoom, 0.1), 1.5);
    setZoom(clamped);
    // Center the tree
    const scaledW = treeW * clamped;
    const scaledH = treeH * clamped;
    setPan({
      x: (width - scaledW) / 2 - treeBounds.minX * clamped,
      y: (height - scaledH) / 2 - treeBounds.minY * clamped,
    });
  }, [treeBounds]);

  // Auto-fit whenever the layout changes
  useEffect(() => {
    if (treeBounds) {
      // Small delay so the container has rendered
      const id = setTimeout(fitView, 50);
      return () => clearTimeout(id);
    }
  }, [treeBounds, fitView]);

  const zoomAround = useCallback((factor: number) => {
    const container = containerRef.current;
    if (!container) return;
    const { width, height } = container.getBoundingClientRect();
    // Zoom around container center
    const cx = width / 2;
    const cy = height / 2;
    setZoom((z) => {
      const newZ = Math.min(Math.max(z * factor, 0.1), 8);
      setPan((p) => ({
        x: cx - (cx - p.x) * (newZ / z),
        y: cy - (cy - p.y) * (newZ / z),
      }));
      return newZ;
    });
  }, []);

  const zoomIn = useCallback(() => zoomAround(1.3), [zoomAround]);
  const zoomOut = useCallback(() => zoomAround(1 / 1.3), [zoomAround]);

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const container = containerRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const mouseX = e.clientX - rect.left;
    const mouseY = e.clientY - rect.top;
    const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
    setZoom((z) => {
      const newZ = Math.min(Math.max(z * factor, 0.1), 8);
      setPan((p) => ({
        x: mouseX - (mouseX - p.x) * (newZ / z),
        y: mouseY - (mouseY - p.y) * (newZ / z),
      }));
      return newZ;
    });
  }, []);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    setIsPanning(true);
    dragStart.current = { mouseX: e.clientX, mouseY: e.clientY, panX: pan.x, panY: pan.y };
  }, [pan]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isPanning) return;
    setPan({
      x: dragStart.current.panX + (e.clientX - dragStart.current.mouseX),
      y: dragStart.current.panY + (e.clientY - dragStart.current.mouseY),
    });
  }, [isPanning]);

  const handleMouseUp = useCallback(() => setIsPanning(false), []);

  const handleNodeHover = useCallback((layoutNode: LayoutNode, event: React.MouseEvent) => {
    const container = containerRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const tooltipW = 220;
    const tooltipH = 64;
    let x = event.clientX - rect.left + 12;
    let y = event.clientY - rect.top + 12;
    if (x + tooltipW > rect.width) x = rect.width - tooltipW - 4;
    if (y + tooltipH > rect.height) y = event.clientY - rect.top - tooltipH - 4;
    setTooltip({
      x, y,
      nodeType: layoutNode.node.node_type,
      content: truncateLabel(layoutNode.node.content || layoutNode.node.summary || '', 200),
    });
  }, []);

  const handleNodeLeave = useCallback(() => setTooltip(null), []);

  // Empty state
  if (!activeDocumentId || !tree) {
    return (
      <div className={sharedStyles.placeholder}>
        <Network size={32} className={sharedStyles.placeholderIcon} />
        <p className={sharedStyles.placeholderText}>
          {activeDocumentId ? 'Loading document graph...' : 'Select a document to explore'}
        </p>
        <p className={sharedStyles.placeholderHint}>Explore document structure visually</p>
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

  // SVG viewBox: fixed large canvas, transform applied to inner <g>
  const svgW = 10000;
  const svgH = 10000;

  return (
    <div
      className={styles.container}
      ref={containerRef}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
    >
      {/* Controls */}
      <div className={styles.controls}>
        <button type="button" className={styles.controlBtn} onClick={zoomIn} title="Zoom in">
          <ZoomIn size={14} />
        </button>
        <button type="button" className={styles.controlBtn} onClick={zoomOut} title="Zoom out">
          <ZoomOut size={14} />
        </button>
        <button type="button" className={styles.controlBtn} onClick={fitView} title="Fit to view">
          <Maximize2 size={14} />
        </button>
        <span className={styles.zoomLabel}>{Math.round(zoom * 100)}%</span>
      </div>

      {/* Use a plain div canvas — CSS transform for zoom+pan */}
      <div className={styles.svgWrapper}>
        <svg
          className={styles.svgCanvas}
          viewBox={`0 0 ${svgW} ${svgH}`}
          onWheel={handleWheel}
        >
          {/* Single transform group — all zoom/pan lives here */}
          <g transform={`translate(${pan.x}, ${pan.y}) scale(${zoom})`}>
            {edges.map((edge, i) => {
              const midY = (edge.y1 + edge.y2) / 2;
              const d = `M ${edge.x1} ${edge.y1} C ${edge.x1} ${midY}, ${edge.x2} ${midY}, ${edge.x2} ${edge.y2}`;
              return <path key={i} d={d} className={styles.edge} />;
            })}

            {layoutNodes.map((ln) => {
              const isVisited = visitedSet.has(ln.id);
              const isActive = activeNodeId === ln.id;
              const isRoot = ln.depth === 0;
              const r = isRoot ? ROOT_R : NODE_R - ln.depth * 1.5;
              const radius = Math.max(6, r);
              const color = isVisited ? 'var(--accent)' : getNodeColor(ln.node.node_type);
              const labelMaxLen = isRoot ? 18 : 14 - ln.depth * 2;
              const label = truncateLabel(ln.node.content || ln.node.node_type, Math.max(6, labelMaxLen));

              return (
                <g
                  key={ln.id}
                  className={styles.node}
                  onMouseMove={(e) => handleNodeHover(ln, e)}
                  onMouseLeave={handleNodeLeave}
                >
                  {isActive && (
                    <circle cx={ln.x} cy={ln.y} r={radius + 5} fill="none"
                      stroke="var(--accent)" strokeWidth={2} className={styles.activeRing} />
                  )}
                  <circle
                    cx={ln.x} cy={ln.y} r={radius}
                    fill={color}
                    opacity={isRoot ? 1 : isVisited ? 0.9 : 0.75}
                    stroke={isActive ? 'var(--accent)' : color}
                    strokeWidth={isRoot ? 2.5 : isVisited ? 2 : 1}
                  />
                  <text
                    x={ln.x}
                    y={isRoot ? ln.y + 4 : ln.y + radius + 13}
                    className={isRoot ? styles.nodeLabelRoot : styles.nodeLabel}
                    textAnchor="middle"
                  >
                    {isRoot ? truncateLabel(ln.node.content || tree?.name || '', 20) : label}
                  </text>
                </g>
              );
            })}
          </g>
        </svg>
      </div>

      {tooltip && (
        <div
          className={styles.tooltip}
          style={{ '--tooltip-x': `${tooltip.x}px`, '--tooltip-y': `${tooltip.y}px` } as React.CSSProperties}
        >
          <div className={styles.tooltipType}>{tooltip.nodeType}</div>
          <div className={styles.tooltipContent}>{tooltip.content || '(empty)'}</div>
        </div>
      )}
    </div>
  );
}
