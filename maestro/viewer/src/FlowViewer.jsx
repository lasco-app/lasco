import { useEffect, useMemo, useState } from "react";
import {
  Background,
  Controls,
  Handle,
  MarkerType,
  Position,
  ReactFlow,
} from "@xyflow/react";

const platforms = ["ios", "android"];
const cardWidth = 198;
const connectorWidth = 54;
const groupGap = 84;

// Older captures have no groups in their manifest. Keeping this mapping here
// means the viewer can still organize them without having to recapture first.
const fallbackGroups = [
  { id: "onboarding", title: "Onboarding", screenIds: ["onboarding-intro", "onboarding-encryption", "onboarding-safety", "library-choice", "new-library-preview", "existing-library-choice"] },
  { id: "library-list", title: "Library list", screenIds: ["library-list", "settings", "existing-library-source", "existing-library-form"] },
  { id: "create-library", title: "Create library", screenIds: ["create-library", "master-key"] },
  { id: "remote-import", title: "Local filesystem remote", screenIds: ["add-remote", "local-fs-remote", "initial-import", "import-library", "import-complete", "auto-import"] },
  { id: "library", title: "Library", screenIds: ["home", "albums", "status", "manage"] },
];

function ScreenshotCard({ screen, platform, platformStatus, runId }) {
  const image = `/runs/${runId}/${platform}/screenshots/${screen.file}.png`;
  const [missing, setMissing] = useState(false);
  const unavailable = missing || platformStatus !== "passed";

  return (
    <article className={`screen-card ${platform} ${platformStatus ?? "not-run"}`}>
      <p className="platform-label">{platform === "ios" ? "iOS" : "Android"}</p>
      {unavailable ? (
        <div className="thumbnail-unavailable">
          {platformStatus === "passed" ? "Screenshot missing" : `Not run on ${platform === "ios" ? "iOS" : "Android"}`}
        </div>
      ) : (
        <img src={image} alt={`${screen.title} on ${platform}`} onError={() => setMissing(true)} />
      )}
    </article>
  );
}

function FlowGroupNode({ data }) {
  return (
    <section className="flow-group" style={{ width: data.width }}>
      <Handle type="target" position={Position.Top} />
      <header className="group-header">
        <span className="group-kicker">Flow section</span>
        <div>
          <h2>{data.title}</h2>
          <p>{data.screens.length} captured screens</p>
        </div>
      </header>
      <div className="screen-rail">
        {data.screens.map((screen, index) => (
          <div className="rail-item" key={screen.id}>
            <div className="screen-step">
              <span>{String(screen.order).padStart(2, "0")}</span>
              <p>{screen.title}</p>
            </div>
            <div className="platform-pair">
              {platforms.map((platform) => (
                <ScreenshotCard
                  key={platform}
                  screen={screen}
                  platform={platform}
                  platformStatus={data.platforms[platform]?.status}
                  runId={data.runId}
                />
              ))}
            </div>
            {index < data.screens.length - 1 && (
              <div className="rail-connector" aria-label={data.internalLabels[index] ?? "Next screen"}>
                <span>{data.internalLabels[index] ?? "Next"}</span>
                <b>→</b>
              </div>
            )}
          </div>
        ))}
      </div>
      <Handle type="source" position={Position.Bottom} />
    </section>
  );
}

function groupWidth(screenCount) {
  return Math.max(430, screenCount * cardWidth + Math.max(0, screenCount - 1) * connectorWidth + 40);
}

function buildGraph(runId, manifest) {
  const flow = manifest.flows[0];
  const screensById = new Map(flow.screenshots.map((screen) => [screen.id, screen]));
  const groups = (flow.groups ?? fallbackGroups)
    .map((group) => ({ ...group, screens: group.screenIds.map((id) => screensById.get(id)).filter(Boolean) }))
    .filter((group) => group.screens.length);
  const groupByScreen = new Map(groups.flatMap((group) => group.screens.map((screen) => [screen.id, group.id])));

  const transitionByPair = new Map();
  for (const transition of flow.transitions) {
    const fromGroup = groupByScreen.get(transition.from);
    const toGroup = groupByScreen.get(transition.to);
    if (!fromGroup || !toGroup || fromGroup === toGroup) continue;
    const key = `${fromGroup}:${toGroup}`;
    if (!transitionByPair.has(key)) transitionByPair.set(key, { ...transition, fromGroup, toGroup });
  }

  let y = 0;
  const nodes = groups.map((group) => {
    const width = groupWidth(group.screens.length);
    const node = {
      id: group.id,
      type: "flowGroup",
      position: { x: 0, y },
      draggable: false,
      data: {
        ...group,
        runId,
        width,
        platforms: manifest.platforms,
        internalLabels: group.screens.slice(0, -1).map((screen) =>
          flow.transitions.find((transition) => transition.from === screen.id && groupByScreen.get(transition.to) === group.id)?.label,
        ),
      },
    };
    y += 800 + groupGap;
    return node;
  });

  const edges = [...transitionByPair.values()].map((transition) => ({
    id: `${transition.fromGroup}:${transition.toGroup}`,
    source: transition.fromGroup,
    target: transition.toGroup,
    label: transition.label,
    type: "smoothstep",
    markerEnd: { type: MarkerType.ArrowClosed },
    style: { stroke: "#b2437e", strokeWidth: 2 },
    labelStyle: { fill: "#413a58", fontSize: 11, fontWeight: 700 },
    labelBgStyle: { fill: "#f7f5fb", fillOpacity: 0.92 },
    labelBgPadding: [5, 3],
  }));

  return { nodes, edges, title: flow.title, screenCount: flow.screenshots.length, groupCount: groups.length };
}

export function FlowViewer({ runId }) {
  const [manifest, setManifest] = useState();
  const [error, setError] = useState();

  useEffect(() => {
    if (!runId) {
      setError("Start the viewer with pnpm flow:view -- --run <run-id>.");
      return;
    }
    fetch(`/runs/${runId}/manifest.json`)
      .then((response) => response.ok ? response.json() : Promise.reject(new Error("Captured flow manifest not found.")))
      .then(setManifest)
      .catch((cause) => setError(cause.message));
  }, [runId]);

  const graph = useMemo(() => manifest && buildGraph(runId, manifest), [manifest, runId]);
  if (error) return <main className="message"><h1>Lasco flow viewer</h1><p>{error}</p></main>;
  if (!graph) return <main className="message"><p>Loading captured flow…</p></main>;

  return (
    <main className="viewer">
      <section className="legend">
        <div>
          <p className="eyebrow">Captured flow</p>
          <h1>{graph.title}</h1>
          <p>{graph.screenCount} screens in {graph.groupCount} flow sections · run {runId}</p>
        </div>
        <p className="interaction-hint">Scroll to zoom · drag the canvas to inspect a section</p>
      </section>
      <ReactFlow
        nodes={graph.nodes}
        edges={graph.edges}
        nodeTypes={{ flowGroup: FlowGroupNode }}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={false}
        defaultViewport={{ x: 28, y: 130, zoom: 0.42 }}
        minZoom={0.08}
      >
        <Background gap={18} size={1} color="#dfdbea" />
        <Controls showInteractive={false} />
      </ReactFlow>
    </main>
  );
}
