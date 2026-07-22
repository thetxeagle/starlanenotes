import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { isTauri } from "@/lib/api";
import "./styles/globals.css";

if (isTauri()) {
  document
    .querySelector('meta[name="viewport"]')
    ?.setAttribute(
      "content",
      "width=device-width, initial-scale=1.0, maximum-scale=1.0, viewport-fit=cover, user-scalable=no",
    );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
