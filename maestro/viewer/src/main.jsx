import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@xyflow/react/dist/style.css";
import "./styles.css";
import { FlowViewer } from "./FlowViewer.jsx";

const selectedRun = new URLSearchParams(window.location.search).get("run") ?? import.meta.env.VITE_FLOW_RUN;

createRoot(document.getElementById("root")).render(
  <StrictMode>
    <FlowViewer runId={selectedRun} />
  </StrictMode>,
);
