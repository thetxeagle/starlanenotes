import { useCallback, useEffect, useState } from "react";

const STORAGE_KEY = "editor_zoom";
const MIN = 75;
const MAX = 200;
const STEP = 10;
const DEFAULT = 100;

function clamp(value: number): number {
  return Math.min(MAX, Math.max(MIN, value));
}

function loadZoom(): number {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored) {
    const parsed = parseInt(stored, 10);
    if (!isNaN(parsed)) return clamp(parsed);
  }
  return DEFAULT;
}

export function useEditorZoom() {
  const [zoom, setZoomState] = useState(loadZoom);

  const setZoom = useCallback((value: number) => {
    const clamped = clamp(value);
    setZoomState(clamped);
    localStorage.setItem(STORAGE_KEY, String(clamped));
  }, []);

  const zoomIn = useCallback(() => setZoom(loadZoom() + STEP), [setZoom]);
  const zoomOut = useCallback(() => setZoom(loadZoom() - STEP), [setZoom]);
  const zoomReset = useCallback(() => setZoom(DEFAULT), [setZoom]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;

      if (e.key === "=" || e.key === "+") {
        e.preventDefault();
        zoomIn();
      } else if (e.key === "-") {
        e.preventDefault();
        zoomOut();
      } else if (e.key === "0") {
        e.preventDefault();
        zoomReset();
      }
    };

    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [zoomIn, zoomOut, zoomReset]);

  const cssVar = zoom / 100;

  return { zoom, cssVar, zoomIn, zoomOut, zoomReset };
}
