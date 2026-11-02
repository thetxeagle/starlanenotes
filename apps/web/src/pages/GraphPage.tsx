import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { useNavigate, useParams } from "react-router-dom";
import { forceCenter, forceLink, forceManyBody, forceSimulation } from "d3-force";
import { Link2, Loader2, Maximize2, Minus, Plus, RefreshCw, RotateCcw } from "lucide-react";
import { SearchInput } from "@/components/ui/search-input";
import { Select } from "@/components/ui/select";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
} from "@/components/ui/control-chrome";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import { dataClient, type GraphNodeRecord, type GraphRecord } from "@/lib/dataClient";
import { addGraphDirtyListener } from "@/lib/graphEvents";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";

interface GraphNodeApi {
  id: string;
  title: string;
  folder_id?: string | null;
  connection_count: number;
  orphan: boolean;
}

interface GraphEdgeApi {
  source: string;
  target: string;
  relation: string;
}

type GraphResponse = GraphRecord;

interface FolderItem {
  id: string;
  name: string;
}

interface GraphNodeLayout extends GraphNodeApi {
  x: number;
  y: number;
  r: number;
}

interface GraphEdgeLayout extends GraphEdgeApi {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

interface DocumentPreview {
  id: string;
  title: string;
  content: string;
}

interface CachedLayoutEntry {
  signature: string;
  positions: Record<string, { x: number; y: number; r: number }>;
  updated_at: string;
}

const VIEW_WIDTH = 1200;
const VIEW_HEIGHT = 760;
const ZOOM_MIN = 0.25;
const ZOOM_MAX = 4;
const WORKER_LAYOUT_THRESHOLD = 500;
const GRAPH_CACHE_DB = "starlane-graph-cache";
const GRAPH_CACHE_STORE = "layouts";
const MINIMAP_SIZE = 112;
export const GRAPH_ERROR_CLASS =
  "rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-sm text-danger";

interface GraphLayoutWorkerResult {
  signature: string;
  nodes: Array<{ id: string; x: number; y: number; r: number }>;
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

export function graphBounds(nodes: Array<{ x: number; y: number; r: number }>) {
  if (nodes.length === 0) return null;
  return nodes.reduce(
    (bounds, node) => ({
      minX: Math.min(bounds.minX, node.x - node.r),
      minY: Math.min(bounds.minY, node.y - node.r),
      maxX: Math.max(bounds.maxX, node.x + node.r),
      maxY: Math.max(bounds.maxY, node.y + node.r),
    }),
    {
      minX: Number.POSITIVE_INFINITY,
      minY: Number.POSITIVE_INFINITY,
      maxX: Number.NEGATIVE_INFINITY,
      maxY: Number.NEGATIVE_INFINITY,
    },
  );
}

export function fitGraphViewport(nodes: Array<{ x: number; y: number; r: number }>): {
  zoom: number;
  pan: { x: number; y: number };
} {
  const bounds = graphBounds(nodes);
  if (!bounds) return { zoom: 1, pan: { x: 0, y: 0 } };
  const padding = 80;
  const width = Math.max(1, bounds.maxX - bounds.minX + padding * 2);
  const height = Math.max(1, bounds.maxY - bounds.minY + padding * 2);
  const nextZoom = clamp(Math.min(VIEW_WIDTH / width, VIEW_HEIGHT / height), ZOOM_MIN, ZOOM_MAX);
  const centerX = (bounds.minX + bounds.maxX) / 2;
  const centerY = (bounds.minY + bounds.maxY) / 2;
  return {
    zoom: Number(nextZoom.toFixed(2)),
    pan: {
      x: VIEW_WIDTH / 2 - centerX * nextZoom,
      y: VIEW_HEIGHT / 2 - centerY * nextZoom,
    },
  };
}

export function graphVisibleStats(graph: GraphRecord, visibleNodeIds: Set<string>) {
  return {
    visibleNodes: visibleNodeIds.size,
    hiddenNodes: Math.max(0, graph.nodes.length - visibleNodeIds.size),
    visibleEdges: graph.edges.filter(
      (edge) => visibleNodeIds.has(edge.source) && visibleNodeIds.has(edge.target),
    ).length,
    orphanNodes: graph.nodes.filter((node) => node.orphan).length,
  };
}


export function graphNeighborsByNode(
  nodes: Array<Pick<GraphNodeRecord, "id">>,
  edges: GraphRecord["edges"],
): Record<string, Set<string>> {
  const neighbors: Record<string, Set<string>> = {};
  for (const node of nodes) neighbors[node.id] = new Set<string>();
  for (const edge of edges) {
    if (!neighbors[edge.source] || !neighbors[edge.target]) continue;
    neighbors[edge.source]?.add(edge.target);
    neighbors[edge.target]?.add(edge.source);
  }
  return neighbors;
}

export function graphVisibleFocusNodeIds(
  focusNodeId: string | null,
  neighborsByNode: Record<string, Set<string>>,
): Set<string> | null {
  if (!focusNodeId) return null;
  const neighbors = neighborsByNode[focusNodeId];
  if (!neighbors) return new Set<string>([focusNodeId]);
  return new Set<string>([focusNodeId, ...neighbors.values()]);
}

export function graphSelectedNeighborIds(
  selectedNodeId: string | null,
  neighborsByNode: Record<string, Set<string>>,
): Set<string> | null {
  if (!selectedNodeId) return null;
  const neighbors = neighborsByNode[selectedNodeId];
  return new Set<string>([selectedNodeId, ...(neighbors?.values() ?? [])]);
}

export function graphVisibleNodeIds(
  nodes: Array<Pick<GraphNodeRecord, "id" | "folder_id" | "connection_count" | "orphan">>,
  options: {
    showOrphans: boolean;
    folderFilter: string;
    minConnections: number;
    focusVisibleNodeIds?: Set<string> | null;
  },
): Set<string> {
  const ids = new Set<string>();

  for (const node of nodes) {
    if (!options.showOrphans && node.orphan) continue;
    if (node.connection_count < options.minConnections) continue;
    if (options.folderFilter !== "all") {
      const nodeFolder = node.folder_id ?? "none";
      if (nodeFolder !== options.folderFilter) continue;
    }
    if (options.focusVisibleNodeIds && !options.focusVisibleNodeIds.has(node.id)) continue;
    ids.add(node.id);
  }

  return ids;
}

export function getGraphDocumentPath(spaceId: string, nodeId: string): string {
  return buildDocumentRoutePath(spaceId, nodeId);
}

function openGraphCacheDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(GRAPH_CACHE_DB, 1);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(GRAPH_CACHE_STORE)) {
        db.createObjectStore(GRAPH_CACHE_STORE);
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function loadLayoutCache(spaceId: string): Promise<CachedLayoutEntry | null> {
  try {
    const db = await openGraphCacheDb();
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(GRAPH_CACHE_STORE, "readonly");
      const store = tx.objectStore(GRAPH_CACHE_STORE);
      const req = store.get(spaceId);
      req.onsuccess = () => resolve((req.result as CachedLayoutEntry | undefined) ?? null);
      req.onerror = () => reject(req.error);
    });
  } catch {
    return null;
  }
}

async function saveLayoutCache(spaceId: string, entry: CachedLayoutEntry): Promise<void> {
  try {
    const db = await openGraphCacheDb();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(GRAPH_CACHE_STORE, "readwrite");
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
      tx.objectStore(GRAPH_CACHE_STORE).put(entry, spaceId);
    });
  } catch {
    // best-effort cache only
  }
}

function GraphNodeContextMenu({
  nodeTitle,
  open,
  pinned,
  onOpenChange,
  onOpen,
  onOpenInNewTab,
  onShowConnections,
  onTogglePin,
  children,
}: {
  nodeTitle: string;
  open: boolean;
  pinned: boolean;
  onOpenChange: (open: boolean) => void;
  onOpen: () => void;
  onOpenInNewTab: () => void;
  onShowConnections: () => void;
  onTogglePin: () => void;
  children: ReactNode;
}) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const handleMenuKeyDown = useMenuKeyboardNavigation(menuRef);

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent
        align="start"
        side="bottom"
        sideOffset={6}
        className="w-48 p-1"
        role="menu"
        aria-label={`${nodeTitle} actions`}
      >
        <div ref={menuRef} onKeyDown={handleMenuKeyDown}>
          <button type="button" role="menuitem" onClick={onOpen} className="starlane-menu-item">
            Open
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={onOpenInNewTab}
            className="starlane-menu-item"
          >
            Open in new tab
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={onShowConnections}
            className="starlane-menu-item"
          >
            Show connections
          </button>
          <button
            type="button"
            role="menuitemcheckbox"
            aria-checked={pinned}
            onClick={onTogglePin}
            className="starlane-menu-item"
          >
            {pinned ? "Unpin node" : "Pin node"}
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}

export function GraphPage() {
  const navigate = useNavigate();
  const { spaceId } = useParams<{ spaceId: string }>();
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [graph, setGraph] = useState<GraphResponse>({ nodes: [], edges: [] });
  const [folders, setFolders] = useState<FolderItem[]>([]);
  const [focusNodeId, setFocusNodeId] = useState<string | null>(null);
  const [pinnedNodes, setPinnedNodes] = useState<Record<string, boolean>>({});
  const [menuNodeId, setMenuNodeId] = useState<string | null>(null);
  const [showOrphans, setShowOrphans] = useState(true);
  const [folderFilter, setFolderFilter] = useState<string>("all");
  const [minConnections, setMinConnections] = useState(0);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState("");
  const [previewDoc, setPreviewDoc] = useState<DocumentPreview | null>(null);
  const [cachedEntry, setCachedEntry] = useState<CachedLayoutEntry | null>(null);
  const [workerLayout, setWorkerLayout] = useState<GraphLayoutWorkerResult | null>(null);
  const [workerLayoutBusy, setWorkerLayoutBusy] = useState(false);

  const dragStateRef = useRef<{
    pointerId: number;
    x: number;
    y: number;
    panX: number;
    panY: number;
  } | null>(null);
  const incrementalRefreshTimerRef = useRef<number | null>(null);

  const reloadGraph = useCallback(
    async (mode: "initial" | "refresh" = "refresh") => {
      if (!spaceId) return;
      if (mode === "initial") setLoading(true);
      if (mode === "refresh") setRefreshing(true);
      setError("");

      try {
        const [graphData, folderData] = await Promise.all([
          dataClient.getGraph(spaceId),
          dataClient.listFolders(spaceId).catch(() => []),
        ]);
        setGraph(graphData);
        setFolders(folderData);
      } catch {
        setError("Failed to load graph data.");
      } finally {
        if (mode === "initial") setLoading(false);
        if (mode === "refresh") setRefreshing(false);
      }
    },
    [spaceId],
  );

  useEffect(() => {
    void reloadGraph("initial");
  }, [reloadGraph]);

  useEffect(() => {
    if (!spaceId) return;

    const interval = window.setInterval(() => {
      if (document.visibilityState !== "visible") return;
      void reloadGraph("refresh");
    }, 30_000);

    return () => clearInterval(interval);
  }, [reloadGraph, spaceId]);

  useEffect(() => {
    if (!spaceId) return;

    const cancel = addGraphDirtyListener((detail) => {
      if (detail.spaceId !== spaceId) return;

      if (incrementalRefreshTimerRef.current !== null) {
        window.clearTimeout(incrementalRefreshTimerRef.current);
      }

      incrementalRefreshTimerRef.current = window.setTimeout(() => {
        void reloadGraph("refresh");
        incrementalRefreshTimerRef.current = null;
      }, 1200);
    });

    return () => {
      cancel();
      if (incrementalRefreshTimerRef.current !== null) {
        window.clearTimeout(incrementalRefreshTimerRef.current);
        incrementalRefreshTimerRef.current = null;
      }
    };
  }, [reloadGraph, spaceId]);

  useEffect(() => {
    if (!spaceId) return;
    void loadLayoutCache(spaceId).then((entry) => setCachedEntry(entry));
  }, [spaceId]);

  useEffect(() => {
    if (!spaceId || !selectedNodeId) {
      setPreviewDoc(null);
      setPreviewError("");
      setPreviewLoading(false);
      return;
    }

    setPreviewLoading(true);
    setPreviewError("");
    dataClient
      .getDocumentContent(spaceId, selectedNodeId)
      .then((data) => setPreviewDoc(data))
      .catch(() => {
        setPreviewDoc(null);
        setPreviewError("Failed to load document preview.");
      })
      .finally(() => setPreviewLoading(false));
  }, [selectedNodeId, spaceId]);

  const simulationDisabled = graph.nodes.length > 2000;
  const useWorkerLayout = graph.nodes.length > WORKER_LAYOUT_THRESHOLD;
  const graphSignature = useMemo(() => {
    const nodeIds = [...graph.nodes.map((n) => n.id)].sort();
    const edgeKeys = [...graph.edges.map((e) => `${e.source}:${e.target}`)].sort();
    return `${nodeIds.join(",")}|${edgeKeys.join(",")}`;
  }, [graph.edges, graph.nodes]);

  useEffect(() => {
    if (!useWorkerLayout) {
      setWorkerLayout(null);
      setWorkerLayoutBusy(false);
      return;
    }

    const cachedPositions =
      cachedEntry?.signature === graphSignature ? cachedEntry.positions : null;
    const worker = new Worker(new URL("../workers/graphLayoutWorker.ts", import.meta.url), {
      type: "module",
    });

    setWorkerLayoutBusy(true);
    worker.onmessage = (event: MessageEvent<GraphLayoutWorkerResult>) => {
      setWorkerLayout(event.data);
      setWorkerLayoutBusy(false);
    };
    worker.onerror = () => {
      setWorkerLayout(null);
      setWorkerLayoutBusy(false);
    };

    worker.postMessage({
      signature: graphSignature,
      nodes: graph.nodes,
      edges: graph.edges,
      cachedPositions,
      width: VIEW_WIDTH,
      height: VIEW_HEIGHT,
      maxIterations: simulationDisabled ? 70 : 120,
    });

    return () => {
      worker.terminate();
    };
  }, [
    cachedEntry?.positions,
    cachedEntry?.signature,
    graph.edges,
    graph.nodes,
    graphSignature,
    simulationDisabled,
    useWorkerLayout,
  ]);

  const layout = useMemo(() => {
    const cachedPositions =
      cachedEntry?.signature === graphSignature ? cachedEntry.positions : null;
    const workerPositions =
      workerLayout?.signature === graphSignature
        ? Object.fromEntries(workerLayout.nodes.map((node) => [node.id, node]))
        : null;

    const baseNodes: GraphNodeLayout[] = graph.nodes.map((node, i) => {
      const angle = (i / Math.max(1, graph.nodes.length)) * Math.PI * 2;
      const radius = 220 + (i % 9) * 8;
      const connectionScale = Math.min(node.connection_count, 16) / 16;
      const size = 8 + connectionScale * 16;
      const worker = workerPositions?.[node.id];
      const cached = cachedPositions?.[node.id];
      return {
        ...node,
        x: worker?.x ?? cached?.x ?? VIEW_WIDTH / 2 + Math.cos(angle) * radius,
        y: worker?.y ?? cached?.y ?? VIEW_HEIGHT / 2 + Math.sin(angle) * radius,
        r: worker?.r ?? cached?.r ?? size,
      };
    });

    const byId = new Map(baseNodes.map((n) => [n.id, n]));
    const links = graph.edges
      .map((edge) => ({
        source: byId.get(edge.source),
        target: byId.get(edge.target),
      }))
      .filter((link) => link.source && link.target) as Array<{
      source: GraphNodeLayout;
      target: GraphNodeLayout;
    }>;

    if (baseNodes.length > 0 && !simulationDisabled && !useWorkerLayout) {
      const simulation = forceSimulation(baseNodes)
        .force(
          "link",
          forceLink<GraphNodeLayout, { source: GraphNodeLayout; target: GraphNodeLayout }>(links)
            .id((d) => d.id)
            .distance(80)
            .strength(0.08),
        )
        .force("charge", forceManyBody<GraphNodeLayout>().strength(-200))
        .force("center", forceCenter(VIEW_WIDTH / 2, VIEW_HEIGHT / 2))
        .stop();

      for (let i = 0; i < 180; i += 1) simulation.tick();
    }

    const edges: GraphEdgeLayout[] = graph.edges
      .map((edge) => {
        const source = byId.get(edge.source);
        const target = byId.get(edge.target);
        if (!source || !target) return null;
        return {
          ...edge,
          x1: source.x,
          y1: source.y,
          x2: target.x,
          y2: target.y,
        };
      })
      .filter((edge): edge is GraphEdgeLayout => edge !== null);

    return { nodes: baseNodes, edges };
  }, [
    cachedEntry?.positions,
    cachedEntry?.signature,
    graph.edges,
    graph.nodes,
    graphSignature,
    simulationDisabled,
    useWorkerLayout,
    workerLayout,
  ]);

  useEffect(() => {
    if (!spaceId || layout.nodes.length === 0) return;

    const positions: Record<string, { x: number; y: number; r: number }> = {};
    for (const node of layout.nodes) {
      positions[node.id] = { x: node.x, y: node.y, r: node.r };
    }

    const entry: CachedLayoutEntry = {
      signature: graphSignature,
      positions,
      updated_at: new Date().toISOString(),
    };
    void saveLayoutCache(spaceId, entry);
  }, [graphSignature, layout.nodes, spaceId]);

  const neighborsByNode = useMemo(() => {
    return graphNeighborsByNode(layout.nodes, layout.edges);
  }, [layout.edges, layout.nodes]);

  const normalizedQuery = query.trim().toLowerCase();
  const nodeMatches = useMemo(() => {
    if (!normalizedQuery) return new Set<string>();
    return new Set(
      layout.nodes
        .filter((node) => node.title.toLowerCase().includes(normalizedQuery))
        .map((node) => node.id),
    );
  }, [layout.nodes, normalizedQuery]);

  const focusVisibleNodeIds = useMemo(() => {
    return graphVisibleFocusNodeIds(focusNodeId, neighborsByNode);
  }, [focusNodeId, neighborsByNode]);

  const selectedNeighborIds = useMemo(() => {
    return graphSelectedNeighborIds(selectedNodeId, neighborsByNode);
  }, [neighborsByNode, selectedNodeId]);

  const visibleNodeIds = useMemo(() => {
    return graphVisibleNodeIds(layout.nodes, {
      focusVisibleNodeIds,
      folderFilter,
      minConnections,
      showOrphans,
    });
  }, [focusVisibleNodeIds, folderFilter, layout.nodes, minConnections, showOrphans]);

  const filteredNodes = useMemo(
    () => layout.nodes.filter((node) => visibleNodeIds.has(node.id)),
    [layout.nodes, visibleNodeIds],
  );

  const filteredEdges = useMemo(
    () =>
      layout.edges.filter(
        (edge) => visibleNodeIds.has(edge.source) && visibleNodeIds.has(edge.target),
      ),
    [layout.edges, visibleNodeIds],
  );

  const stats = useMemo(() => graphVisibleStats(graph, visibleNodeIds), [graph, visibleNodeIds]);

  const closeGraphMenu = useCallback(() => setMenuNodeId(null), []);

  const handleGraphNodeKeyDown = (
    event: ReactKeyboardEvent<SVGCircleElement>,
    nodeId: string,
  ) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      event.stopPropagation();
      setSelectedNodeId(nodeId);
      closeGraphMenu();
      return;
    }
    if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) return;
    event.preventDefault();
    event.stopPropagation();
    setMenuNodeId(nodeId);
  };

  const viewportRect = useMemo(() => {
    const x = clamp(-pan.x / zoom, 0, VIEW_WIDTH);
    const y = clamp(-pan.y / zoom, 0, VIEW_HEIGHT);
    const w = clamp(VIEW_WIDTH / zoom, 0, VIEW_WIDTH);
    const h = clamp(VIEW_HEIGHT / zoom, 0, VIEW_HEIGHT);
    return { x, y, w, h };
  }, [pan.x, pan.y, zoom]);

  const openNode = (nodeId: string) => {
    if (spaceId) navigate(getGraphDocumentPath(spaceId, nodeId));
    setMenuNodeId(null);
  };

  const openNodeInNewTab = (nodeId: string) => {
    if (spaceId) {
      window.open(getGraphDocumentPath(spaceId, nodeId), "_blank", "noopener,noreferrer");
    }
    setMenuNodeId(null);
  };

  const showConnections = (nodeId: string) => {
    setFocusNodeId(nodeId);
    setMenuNodeId(null);
  };

  const togglePinNode = (nodeId: string) => {
    setPinnedNodes((prev) => ({
      ...prev,
      [nodeId]: !prev[nodeId],
    }));
    setMenuNodeId(null);
  };

  const handleGraphWheel = (e: React.WheelEvent<SVGSVGElement>) => {
    e.preventDefault();
    const direction = e.deltaY > 0 ? -0.1 : 0.1;
    setZoom((prev) => clamp(Number((prev + direction).toFixed(2)), ZOOM_MIN, ZOOM_MAX));
  };

  const handleGraphPointerDown = (e: React.PointerEvent<SVGSVGElement>) => {
    if (!e.isPrimary || (e.pointerType === "mouse" && e.button !== 0)) return;
    if ((e.target as Element).closest("[data-graph-node]")) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    dragStateRef.current = {
      pointerId: e.pointerId,
      x: e.clientX,
      y: e.clientY,
      panX: pan.x,
      panY: pan.y,
    };
  };

  const handleGraphPointerMove = (e: React.PointerEvent<SVGSVGElement>) => {
    const drag = dragStateRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    e.preventDefault();
    setPan({
      x: drag.panX + e.clientX - drag.x,
      y: drag.panY + e.clientY - drag.y,
    });
  };

  const finishGraphPointerDrag = (e: React.PointerEvent<SVGSVGElement>) => {
    const drag = dragStateRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    dragStateRef.current = null;
  };

  const handleMinimapClick = useCallback(
    (e: React.MouseEvent<SVGSVGElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return;

      const graphX = ((e.clientX - rect.left) / rect.width) * VIEW_WIDTH;
      const graphY = ((e.clientY - rect.top) / rect.height) * VIEW_HEIGHT;

      setPan({
        x: VIEW_WIDTH / 2 - graphX * zoom,
        y: VIEW_HEIGHT / 2 - graphY * zoom,
      });
    },
    [zoom],
  );

  const fitVisibleGraph = () => {
    const next = fitGraphViewport(filteredNodes);
    setZoom(next.zoom);
    setPan(next.pan);
  };

  const resetGraphView = () => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
    setFocusNodeId(null);
    setSelectedNodeId(null);
    setMinConnections(0);
    setFolderFilter("all");
    setQuery("");
    setShowOrphans(true);
  };

  return (
    <div
      className="bg-bg relative flex h-full min-h-0 flex-col"
      onClick={() => {
        if (menuNodeId) closeGraphMenu();
      }}
    >
      <div className="border-border border-b px-4 py-3">
        <div className="mb-2 flex items-center justify-between gap-3">
          <h1 className="text-text text-sm font-semibold">Graph View</h1>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => void reloadGraph("refresh")}
              className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text border px-2 py-1 text-xs transition-colors duration-100"
            >
              <span className="inline-flex items-center gap-1">
                <RefreshCw size={12} className={refreshing ? "animate-spin" : ""} />
                Refresh
              </span>
            </button>
            {focusNodeId && (
              <button
                type="button"
                onClick={() => setFocusNodeId(null)}
                className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text border px-2 py-1 text-xs transition-colors duration-100"
              >
                Clear focus
              </button>
            )}
            <p className="text-text-muted text-xs">
              {graph.nodes.length} nodes · {graph.edges.length} edges
            </p>
          </div>
        </div>

        <div className="grid gap-2 lg:grid-cols-[1fr_auto_auto_auto_auto_auto]">
          <SearchInput
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search documents in graph..."
            aria-label="Search documents in graph"
            className="h-9 text-sm"
          />

          <label className="rounded-input border-border bg-surface text-text-muted flex items-center gap-2 border px-2 py-1.5 text-xs">
            <input
              type="checkbox"
              checked={showOrphans}
              onChange={(e) => setShowOrphans(e.target.checked)}
              className="accent-[var(--primary)]"
            />
            Show orphans
          </label>

          <Select
            value={folderFilter}
            onChange={(e) => setFolderFilter(e.target.value)}
            className="text-xs"
            aria-label="Filter graph by folder"
          >
            <option value="all">All folders</option>
            <option value="none">Root docs</option>
            {folders.map((folder) => (
              <option key={folder.id} value={folder.id}>
                {folder.name}
              </option>
            ))}
          </Select>

          <Select
            value={minConnections}
            onChange={(e) => setMinConnections(Number(e.target.value))}
            className="text-xs"
            aria-label="Minimum graph connections"
          >
            <option value={0}>Any links</option>
            <option value={1}>1+ links</option>
            <option value={2}>2+ links</option>
            <option value={5}>5+ links</option>
          </Select>

          <div
            role="group"
            aria-label="Graph zoom controls"
            className={`${CONTROL_CHROME_RAIL_CLASS} px-1.5 py-1`}
          >
            <button
              type="button"
              onClick={() => setZoom((prev) => clamp(prev - 0.1, ZOOM_MIN, ZOOM_MAX))}
              className={`${CONTROL_CHROME_ACTION_CLASS} p-1`}
              aria-label="Zoom out"
            >
              <Minus size={12} />
            </button>
            <span className="text-text-muted min-w-[56px] text-center text-xs">
              {Math.round(zoom * 100)}%
            </span>
            <button
              type="button"
              onClick={() => setZoom((prev) => clamp(prev + 0.1, ZOOM_MIN, ZOOM_MAX))}
              className={`${CONTROL_CHROME_ACTION_CLASS} p-1`}
              aria-label="Zoom in"
            >
              <Plus size={12} />
            </button>
          </div>

          <div
            role="group"
            aria-label="Graph viewport controls"
            className={`${CONTROL_CHROME_RAIL_CLASS} px-1.5 py-1`}
          >
            <button
              type="button"
              onClick={fitVisibleGraph}
              className={`${CONTROL_CHROME_ACTION_CLASS} p-1`}
              aria-label="Fit graph to view"
            >
              <Maximize2 size={12} />
            </button>
            <button
              type="button"
              onClick={resetGraphView}
              className={`${CONTROL_CHROME_ACTION_CLASS} p-1`}
              aria-label="Reset graph view"
            >
              <RotateCcw size={12} />
            </button>
          </div>
        </div>

        <div className="text-text-muted mt-2 flex flex-wrap items-center gap-2 text-xs">
          <span>{stats.visibleNodes} visible</span>
          <span>{stats.visibleEdges} visible edges</span>
          <span>{stats.hiddenNodes} hidden</span>
          <span>{stats.orphanNodes} orphans</span>
          {selectedNodeId && (
            <span className="text-text inline-flex items-center gap-1">
              <Link2 size={12} />
              {neighborsByNode[selectedNodeId]?.size ?? 0} selected links
            </span>
          )}
        </div>

        {useWorkerLayout && (
          <p className="text-text-muted mt-2 text-xs">
            Large graph — layout running in background worker
            {workerLayoutBusy ? "…" : "."}
          </p>
        )}
      </div>

      {loading && (
        <div className="text-text-muted flex flex-1 items-center justify-center text-sm">
          <Loader2 size={16} className="mr-2 animate-spin" />
          Building graph…
        </div>
      )}

      {!loading && error && <div className={`m-4 ${GRAPH_ERROR_CLASS}`}>{error}</div>}

      {!loading && !error && filteredNodes.length === 0 && (
        <div className="rounded-input border-border bg-surface text-text-muted m-4 border px-3 py-2 text-sm">
          No graph nodes match the current filters.
        </div>
      )}

      {!loading && !error && filteredNodes.length > 0 && (
        <div className="flex min-h-0 flex-1 gap-3 p-3">
          <div className={`${selectedNodeId ? "w-2/5" : "w-full"} min-w-0`}>
            <div className="rounded-card border-border bg-surface relative h-full border">
              <svg
                viewBox={`0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}`}
                className="h-full w-full touch-none select-none"
                role="group"
                aria-label="Document relationship graph"
                onWheel={handleGraphWheel}
                onPointerDown={handleGraphPointerDown}
                onPointerMove={handleGraphPointerMove}
                onPointerUp={finishGraphPointerDrag}
                onPointerCancel={finishGraphPointerDrag}
                onLostPointerCapture={() => {
                  dragStateRef.current = null;
                }}
                onDragStart={(event) => event.preventDefault()}
                onContextMenu={(e) => e.preventDefault()}
              >
                <g transform={`translate(${pan.x}, ${pan.y}) scale(${zoom})`}>
                  <g>
                    {filteredEdges.map((edge) => {
                      const queryDimmed =
                        normalizedQuery.length > 0 &&
                        !nodeMatches.has(edge.source) &&
                        !nodeMatches.has(edge.target);
                      const neighborDimmed =
                        selectedNeighborIds !== null &&
                        (!selectedNeighborIds.has(edge.source) ||
                          !selectedNeighborIds.has(edge.target));
                      return (
                        <line
                          key={`${edge.source}-${edge.target}`}
                          x1={edge.x1}
                          y1={edge.y1}
                          x2={edge.x2}
                          y2={edge.y2}
                          stroke="var(--border)"
                          strokeOpacity={queryDimmed || neighborDimmed ? 0.18 : 0.55}
                          strokeWidth={1}
                        />
                      );
                    })}
                  </g>

                  <g>
                    {filteredNodes.map((node) => {
                      const matchActive = normalizedQuery.length > 0 && nodeMatches.has(node.id);
                      const queryDimmed = normalizedQuery.length > 0 && !matchActive;
                      const neighborDimmed =
                        selectedNeighborIds !== null && !selectedNeighborIds.has(node.id);
                      const isFocused = focusNodeId === node.id;
                      const isPinned = !!pinnedNodes[node.id];
                      const isSelected = selectedNodeId === node.id;
                      const fill = node.orphan ? "var(--text-muted)" : "var(--surface-raised)";

                      return (
                        <g key={node.id}>
                          <GraphNodeContextMenu
                            nodeTitle={node.title}
                            open={menuNodeId === node.id}
                            pinned={isPinned}
                            onOpenChange={(open) => setMenuNodeId(open ? node.id : null)}
                            onOpen={() => openNode(node.id)}
                            onOpenInNewTab={() => openNodeInNewTab(node.id)}
                            onShowConnections={() => showConnections(node.id)}
                            onTogglePin={() => togglePinNode(node.id)}
                          >
                            <circle
                              data-graph-node
                              cx={node.x}
                              cy={node.y}
                              r={node.r}
                              fill={fill}
                              fillOpacity={
                                node.orphan
                                  ? queryDimmed || neighborDimmed
                                    ? 0.16
                                    : 0.45
                                  : queryDimmed || neighborDimmed
                                    ? 0.2
                                    : 0.95
                              }
                              stroke={
                                isSelected || isFocused || isPinned || matchActive
                                  ? "var(--primary)"
                                  : "var(--border)"
                              }
                              strokeWidth={isSelected || isFocused || isPinned || matchActive ? 2 : 1}
                              className="cursor-pointer transition-all duration-100 focus-visible:stroke-primary focus-visible:stroke-[3]"
                              role="button"
                              tabIndex={0}
                              aria-label={`Select ${node.title}`}
                              onClick={(e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                setSelectedNodeId(node.id);
                                closeGraphMenu();
                              }}
                              onDoubleClick={(e) => {
                                e.stopPropagation();
                                setFocusNodeId(node.id);
                              }}
                              onContextMenu={(e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                setMenuNodeId(node.id);
                              }}
                              onKeyDown={(event) => handleGraphNodeKeyDown(event, node.id)}
                            >
                              <title>{node.title}</title>
                            </circle>
                          </GraphNodeContextMenu>
                        </g>
                      );
                    })}
                  </g>
                </g>
              </svg>

              <div className="border-border bg-bg/95 text-text-muted absolute bottom-3 left-3 rounded border px-2 py-1 text-xs">
                <span className="mr-3 inline-flex items-center gap-1">
                  <span className="bg-surface-raised ring-border h-2 w-2 rounded-full ring-1" />
                  Connected
                </span>
                <span className="inline-flex items-center gap-1">
                  <span className="bg-text-muted/60 h-2 w-2 rounded-full" />
                  Orphan
                </span>
              </div>

              <div
                className="border-border bg-bg/95 absolute bottom-3 right-3 rounded border p-1"
                style={{ height: MINIMAP_SIZE, width: MINIMAP_SIZE }}
              >
                <svg
                  viewBox={`0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}`}
                  className="h-full w-full"
                  onClick={handleMinimapClick}
                >
                  {filteredEdges.map((edge) => (
                    <line
                      key={`mini-${edge.source}-${edge.target}`}
                      x1={edge.x1}
                      y1={edge.y1}
                      x2={edge.x2}
                      y2={edge.y2}
                      stroke="var(--border)"
                      strokeOpacity={0.35}
                      strokeWidth={1}
                    />
                  ))}
                  {filteredNodes.map((node) => (
                    <circle
                      key={`mini-${node.id}`}
                      cx={node.x}
                      cy={node.y}
                      r={Math.max(3, node.r * 0.35)}
                      fill="var(--surface-raised)"
                      stroke="var(--border)"
                      strokeWidth={1}
                    />
                  ))}
                  <rect
                    x={viewportRect.x}
                    y={viewportRect.y}
                    width={viewportRect.w}
                    height={viewportRect.h}
                    fill="none"
                    stroke="var(--primary)"
                    strokeWidth={8 / zoom}
                  />
                </svg>
              </div>
            </div>
          </div>

          {selectedNodeId && (
            <aside className="rounded-card border-border bg-surface min-w-0 flex-1 border">
              <div className="border-border flex items-center justify-between border-b px-3 py-2">
                <div>
                  <p className="text-text-muted text-xs">Selected document</p>
                  <p className="text-text text-sm font-medium">
                    {previewDoc?.title ?? selectedNodeId}
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => openNode(selectedNodeId)}
                    className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text border px-2 py-1 text-xs transition-colors duration-100"
                  >
                    Open editor
                  </button>
                  <button
                    type="button"
                    onClick={() => setSelectedNodeId(null)}
                    className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text border px-2 py-1 text-xs transition-colors duration-100"
                  >
                    Close
                  </button>
                </div>
              </div>

              <div className="h-[calc(100%-49px)] overflow-auto p-3">
                {previewLoading && (
                  <div className="text-text-muted flex items-center text-sm">
                    <Loader2 size={14} className="mr-2 animate-spin" />
                    Loading preview…
                  </div>
                )}
                {!previewLoading && previewError && (
                  <div className={GRAPH_ERROR_CLASS}>{previewError}</div>
                )}
                {!previewLoading && !previewError && previewDoc && (
                  <pre className="text-text whitespace-pre-wrap text-xs leading-6">
                    {previewDoc.content}
                  </pre>
                )}
              </div>
            </aside>
          )}
        </div>
      )}

    </div>
  );
}
