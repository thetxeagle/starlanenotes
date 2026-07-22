/// <reference lib="webworker" />

interface GraphNodeInput {
  id: string;
  connection_count: number;
}

interface GraphEdgeInput {
  source: string;
  target: string;
}

interface WorkerPayload {
  signature: string;
  nodes: GraphNodeInput[];
  edges: GraphEdgeInput[];
  cachedPositions?: Record<string, { x: number; y: number; r: number }> | null;
  width: number;
  height: number;
  maxIterations: number;
}

interface WorkerNodeOutput {
  id: string;
  x: number;
  y: number;
  r: number;
}

interface WorkerResult {
  signature: string;
  nodes: WorkerNodeOutput[];
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

function nodeRadius(connectionCount: number) {
  const connectionScale = Math.min(connectionCount, 16) / 16;
  return 8 + connectionScale * 16;
}

self.onmessage = (event: MessageEvent<WorkerPayload>) => {
  const { signature, nodes, edges, cachedPositions, width, height, maxIterations } = event.data;

  const nodeState = nodes.map((node, i) => {
    const angle = (i / Math.max(1, nodes.length)) * Math.PI * 2;
    const radius = 220 + (i % 11) * 7;
    const cached = cachedPositions?.[node.id];
    return {
      id: node.id,
      x: cached?.x ?? (width / 2 + Math.cos(angle) * radius),
      y: cached?.y ?? (height / 2 + Math.sin(angle) * radius),
      vx: 0,
      vy: 0,
      r: cached?.r ?? nodeRadius(node.connection_count),
    };
  });

  const byId = new Map(nodeState.map((node) => [node.id, node]));
  const links = edges
    .map((edge) => {
      const source = byId.get(edge.source);
      const target = byId.get(edge.target);
      if (!source || !target) return null;
      return { source, target };
    })
    .filter((value): value is { source: (typeof nodeState)[number]; target: (typeof nodeState)[number] } => !!value);

  const iterations = Math.max(40, Math.min(maxIterations, 160));
  const sampleCount = Math.min(8, Math.max(3, Math.floor(nodeState.length / 250)));
  const widthPadding = 36;
  const heightPadding = 36;

  for (let step = 0; step < iterations; step += 1) {
    // Gentle center gravity to keep the cloud in frame.
    for (const node of nodeState) {
      node.vx += (width / 2 - node.x) * 0.0007;
      node.vy += (height / 2 - node.y) * 0.0007;
    }

    // Spring force along graph edges.
    for (const link of links) {
      const dx = link.target.x - link.source.x;
      const dy = link.target.y - link.source.y;
      const distance = Math.max(1, Math.hypot(dx, dy));
      const targetDistance = 82;
      const stretch = (distance - targetDistance) * 0.0009;
      const fx = (dx / distance) * stretch;
      const fy = (dy / distance) * stretch;

      link.source.vx += fx;
      link.source.vy += fy;
      link.target.vx -= fx;
      link.target.vy -= fy;
    }

    // Approximate repulsion with a random sample to avoid O(n^2).
    for (let i = 0; i < nodeState.length; i += 1) {
      const node = nodeState[i];
      for (let sample = 0; sample < sampleCount; sample += 1) {
        const j = Math.floor(Math.random() * nodeState.length);
        if (i === j) continue;
        const other = nodeState[j];
        const dx = node.x - other.x;
        const dy = node.y - other.y;
        const distanceSq = Math.max(16, dx * dx + dy * dy);
        const force = 90 / distanceSq;
        node.vx += dx * force * 0.0002;
        node.vy += dy * force * 0.0002;
      }
    }

    // Integrate velocity with damping and boundary clamp.
    for (const node of nodeState) {
      node.vx *= 0.9;
      node.vy *= 0.9;
      node.x = clamp(node.x + node.vx * 16, widthPadding, width - widthPadding);
      node.y = clamp(node.y + node.vy * 16, heightPadding, height - heightPadding);
    }
  }

  const result: WorkerResult = {
    signature,
    nodes: nodeState.map((node) => ({
      id: node.id,
      x: node.x,
      y: node.y,
      r: node.r,
    })),
  };

  self.postMessage(result);
};
