import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Overlay } from "./Overlay";
import "../styles/index.css";
import "./overlay.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Overlay />
  </StrictMode>,
);
