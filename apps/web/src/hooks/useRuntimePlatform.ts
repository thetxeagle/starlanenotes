import { useEffect, useState } from "react";
import { isTauri } from "@/lib/api";

export type RuntimePlatform = "web" | "tauri-desktop" | "tauri-mobile";

interface RuntimePlatformState {
  runtime: RuntimePlatform;
  loading: boolean;
}

export const MOBILE_VIEWPORT_MEDIA_QUERY = "(max-width: 767px)";

function readMobileViewportMatch(): boolean {
  return typeof window !== "undefined" && window.matchMedia(MOBILE_VIEWPORT_MEDIA_QUERY).matches;
}

export function isLikelyMobilePlatform(platformName: string): boolean {
  const normalized = platformName.toLowerCase();
  return normalized.includes("ios") || normalized.includes("android");
}

interface BrowserPlatformSignals {
  userAgent: string;
  platform: string;
  maxTouchPoints: number;
}

export function isLikelyMobileBrowser(signals: BrowserPlatformSignals): boolean {
  const userAgent = signals.userAgent.toLowerCase();
  const platform = signals.platform.toLowerCase();

  return (
    /android|iphone|ipod|ipad/.test(userAgent) ||
    /iphone|ipod|ipad/.test(platform) ||
    (platform === "macintel" && signals.maxTouchPoints > 1)
  );
}

function detectBrowserPlatform(): RuntimePlatform {
  return isLikelyMobileBrowser({
    userAgent: navigator.userAgent,
    platform: navigator.platform,
    maxTouchPoints: navigator.maxTouchPoints,
  })
    ? "tauri-mobile"
    : "tauri-desktop";
}

export function useRuntimePlatform() {
  const [state, setState] = useState<RuntimePlatformState>(() => {
    if (!isTauri()) {
      return { runtime: "web", loading: false };
    }

    return { runtime: detectBrowserPlatform(), loading: true };
  });
  const [isMobileViewport, setIsMobileViewport] = useState(readMobileViewportMatch);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const media = window.matchMedia(MOBILE_VIEWPORT_MEDIA_QUERY);
    const onChange = (event: MediaQueryListEvent) => setIsMobileViewport(event.matches);
    setIsMobileViewport(media.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    if (!isTauri()) return;

    let cancelled = false;

    async function detectPlatform() {
      try {
        const osModulePath = "@tauri-apps/api/os";
        const osMod = (await import(/* @vite-ignore */ osModulePath)) as {
          platform: () => Promise<string>;
        };
        const platformName = await osMod.platform();

        if (cancelled) return;
        setState({
          runtime: isLikelyMobilePlatform(platformName) ? "tauri-mobile" : "tauri-desktop",
          loading: false,
        });
      } catch {
        if (cancelled) return;
        setState({ runtime: detectBrowserPlatform(), loading: false });
      }
    }

    detectPlatform();

    return () => {
      cancelled = true;
    };
  }, []);

  return {
    runtime: state.runtime,
    loading: state.loading,
    isMobileRuntime: state.runtime === "tauri-mobile",
    isMobileViewport,
    isDesktopRuntime: state.runtime === "tauri-desktop",
  };
}
