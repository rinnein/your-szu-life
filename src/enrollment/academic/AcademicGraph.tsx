import { useEffect, useMemo, useRef, useState } from "react";
import {
  Background,
  Controls,
  ControlButton,
  Handle,
  Position,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  type Node,
  type NodeProps,
} from "@xyflow/react";
import dagre from "@dagrejs/dagre";
import { Button } from "@heroui/react";
import { LuChevronRight, LuMinus, LuPlus, LuScan, LuHouse } from "react-icons/lu";
import { completionSummary, type AcademicTree } from "./tree";
import { getViewState, saveViewState } from "./view-state";
import "@xyflow/react/dist/style.css";
type Data = {
  label: string;
  summary: string;
  kind: string;
  expanded: boolean;
  childrenCount: number;
  open: () => void;
  toggle: () => void;
};
function AcademicNode({ data }: NodeProps<Node<Data>>) {
  return (
    <div
      className={`flex h-[90px] w-[240px] items-center rounded-xl border shadow-sm ${data.kind === "plan" ? "border-accent bg-accent-soft" : "border-separator bg-surface"}`}
    >
      <Handle type="target" position={Position.Left} className="opacity-0" />
      <Button
        variant="ghost"
        className="nodrag h-full min-w-0 flex-1 flex-col items-start gap-1 whitespace-normal rounded-xl px-4 text-left"
        aria-label={`查看${data.label}的课程`}
        onPress={data.open}
      >
        <span className="line-clamp-2 text-sm font-semibold leading-5">{data.label}</span>
        <span className="text-xs text-muted">{data.summary}</span>
      </Button>
      {data.childrenCount > 0 && (
        <Button
          className="nodrag mr-2 shrink-0"
          size="sm"
          isIconOnly
          variant="tertiary"
          aria-label={`${data.expanded ? "收起" : "展开"}${data.label}`}
          onPress={data.toggle}
        >
          {data.expanded ? <LuMinus /> : <LuChevronRight />}
        </Button>
      )}
      <Handle type="source" position={Position.Right} className="opacity-0" />
    </div>
  );
}
const nodeTypes = { academic: AcademicNode };
function Graph({
  tree,
  viewKey,
  onOpen,
}: {
  tree: AcademicTree;
  viewKey: string;
  onOpen: (id: string) => void;
}) {
  const flow = useReactFlow();
  const initialView = useRef(false);
  const restoredViewport = useRef(getViewState(viewKey).viewport);
  const anchor = useRef<{ id: string; x: number; y: number } | undefined>(undefined);
  const [expanded, setExpanded] = useState(() => new Set(getViewState(viewKey).expanded));
  const saved = getViewState(viewKey);
  const elements = useMemo(() => {
    const visible = new Set<string>();
    const visit = (id: string) => {
      if (visible.has(id)) return;
      visible.add(id);
      if (expanded.has(id)) tree.nodes.get(id)?.children.forEach(visit);
    };
    visit(tree.root);
    const graph = new dagre.graphlib.Graph();
    graph.setGraph({ rankdir: "LR", ranksep: 70, nodesep: 22, marginx: 24, marginy: 24 });
    graph.setDefaultEdgeLabel(() => ({}));
    for (const id of visible) graph.setNode(id, { width: 240, height: 90 });
    const edges = [...visible].flatMap((id) => {
      const p = tree.nodes.get(id)?.parent;
      return p && visible.has(p)
        ? [
            {
              id: `${p}->${id}`,
              source: p,
              target: id,
              type: "smoothstep",
              style: { stroke: "var(--separator)", strokeWidth: 1.5 },
            },
          ]
        : [];
    });
    edges.forEach((e) => graph.setEdge(e.source, e.target));
    dagre.layout(graph);
    const nodes: Node<Data>[] = [...visible].map((id) => {
      const node = tree.nodes.get(id)!;
      const pos = graph.node(id);
      return {
        id,
        type: "academic",
        initialWidth: 240,
        initialHeight: 90,
        position: { x: pos.x - 120, y: pos.y - 45 },
        sourcePosition: Position.Right,
        targetPosition: Position.Left,
        data: {
          label: node.label,
          kind: node.kind,
          summary: completionSummary(node.row),
          childrenCount: node.children.length,
          expanded: expanded.has(id),
          open: () => onOpen(id),
          toggle: () => {
            anchor.current = { id, x: pos.x - 120, y: pos.y - 45 };
            setExpanded((old) => {
              const next = new Set(old);
              if (next.has(id)) next.delete(id);
              else next.add(id);
              saveViewState(viewKey, { expanded: [...next] });
              return next;
            });
          },
        },
      };
    });
    return { nodes, edges };
  }, [tree, expanded, viewKey, onOpen]);
  useEffect(() => {
    if (!flow.viewportInitialized || initialView.current) return;
    initialView.current = true;
    if (restoredViewport.current) return;
    const mobile = window.matchMedia("(max-width: 639px)").matches;
    void flow.fitView({
      nodes: mobile ? [{ id: tree.root }] : undefined,
      padding: 0.18,
      minZoom: mobile ? 0.9 : 0.15,
      maxZoom: 1,
    });
  }, [flow, tree.root]);
  useEffect(() => {
    const prior = anchor.current;
    if (!prior) return;
    const node = elements.nodes.find((n) => n.id === prior.id);
    if (!node) return;
    anchor.current = undefined;
    const view = flow.getViewport();
    void flow.setViewport({
      ...view,
      x: view.x + (prior.x - node.position.x) * view.zoom,
      y: view.y + (prior.y - node.position.y) * view.zoom,
    });
  }, [elements, flow]);
  return (
    <div
      className="relative h-[65dvh] min-h-[420px] w-full overflow-hidden rounded-2xl border border-separator bg-surface"
      aria-label="学业完成树图"
    >
      <ReactFlow
        nodes={elements.nodes}
        edges={elements.edges}
        nodeTypes={nodeTypes}
        nodesDraggable={false}
        nodesConnectable={false}
        deleteKeyCode={null}
        defaultViewport={saved.viewport}
        minZoom={0.15}
        maxZoom={1.8}
        fitViewOptions={{ padding: 0.18, maxZoom: 1 }}
        onMoveEnd={(_, viewport) => {
          if (initialView.current) saveViewState(viewKey, { viewport });
        }}
        zoomOnScroll={false}
      >
        <Background gap={24} size={1} color="var(--separator)" />
        <Controls
          showZoom={false}
          showFitView={false}
          showInteractive={false}
          aria-label="树图缩放"
        >
          <ControlButton aria-label="放大" title="放大" onClick={() => void flow.zoomIn()}>
            <LuPlus />
          </ControlButton>
          <ControlButton aria-label="缩小" title="缩小" onClick={() => void flow.zoomOut()}>
            <LuMinus />
          </ControlButton>
        </Controls>
      </ReactFlow>
      <div className="absolute right-3 top-3 flex gap-2">
        <Button
          size="sm"
          variant="secondary"
          onPress={() => void flow.fitView({ padding: 0.18, maxZoom: 1, duration: 250 })}
        >
          <LuScan />
          适应窗口
        </Button>
        <Button
          size="sm"
          variant="secondary"
          onPress={() => {
            const root = elements.nodes.find((n) => n.id === tree.root);
            if (root)
              void flow.setCenter(root.position.x + 120, root.position.y + 45, {
                zoom: 1,
                duration: 250,
              });
          }}
        >
          <LuHouse />
          根节点
        </Button>
      </div>
    </div>
  );
}
export default function AcademicGraph(props: {
  tree: AcademicTree;
  viewKey: string;
  onOpen: (id: string) => void;
}) {
  return (
    <ReactFlowProvider>
      <Graph key={props.viewKey} {...props} />
    </ReactFlowProvider>
  );
}
