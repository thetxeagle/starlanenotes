import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { isTauri } from "@/lib/api";
import { useTheme } from "@/hooks/useTheme";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import { buildSpaceHomePath } from "@/lib/lastViewedSpace";

interface TauriEvent<T = string> {
  payload: T;
}

interface TauriEventModule {
  listen: <T>(event: string, handler: (e: TauriEvent<T>) => void) => Promise<() => void>;
}

interface TauriCoreModule {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

interface DeepLinkRoute {
  action: string;
  space_id?: string;
  doc_id?: string;
  query?: string;
}

function dispatchNativeAuthHandoff(url: string) {
  if (url.startsWith("starlane://auth/handoff?")) {
    document.dispatchEvent(new CustomEvent("starlane:native-auth-handoff", { detail: url }));
    return true;
  }
  return false;
}

export function getTauriDeepLinkRoutePath(route: DeepLinkRoute): string | null {
  switch (route.action) {
    case "open_note":
      return route.space_id && route.doc_id
        ? buildDocumentRoutePath(route.space_id, route.doc_id)
        : null;
    case "open_space":
      return route.space_id ? buildSpaceHomePath(route.space_id) : null;
    default:
      return null;
  }
}

function handleDeepLinkRoute(
  route: DeepLinkRoute,
  navigate: ReturnType<typeof useNavigate>,
) {
  switch (route.action) {
    case "open":
      // Just focus the app — window is already brought to front by Rust
      break;
    case "open_note":
      {
        const path = getTauriDeepLinkRoutePath(route);
        if (path) navigate(path);
      }
      break;
    case "open_space":
      {
        const path = getTauriDeepLinkRoutePath(route);
        if (path) navigate(path);
      }
      break;
    case "new_document":
      document.dispatchEvent(new CustomEvent("starlane:new-document"));
      break;
    case "search":
      document.dispatchEvent(
        new CustomEvent("starlane:toggle-search", {
          detail: { query: route.query || "" },
        }),
      );
      break;
  }
}

export function useTauriEvents() {
  const navigate = useNavigate();
  const { setTheme, resolvedTheme } = useTheme();

  useEffect(() => {
    if (!isTauri()) return;

    const unlisteners: Array<() => void> = [];

    async function setup() {
      try {
        const eventModulePath = "@tauri-apps/api/event";
        const coreModulePath = "@tauri-apps/api/core";
        const eventMod = (await import(
          /* @vite-ignore */ eventModulePath
        )) as unknown as TauriEventModule;
        const coreMod = (await import(
          /* @vite-ignore */ coreModulePath
        )) as unknown as TauriCoreModule;

        // Listen for menu actions
        const unlistenMenu = await eventMod.listen("menu-action", (event: TauriEvent) => {
          switch (event.payload) {
            case "new-note":
              navigate("/");
              break;
            case "search":
              document.dispatchEvent(new CustomEvent("starlane:toggle-search"));
              break;
            case "toggle-sidebar":
              document.dispatchEvent(new CustomEvent("starlane:toggle-sidebar"));
              break;
            case "toggle-dark-mode":
              setTheme(resolvedTheme === "dark" ? "light" : "dark");
              break;
            case "preferences":
              navigate("/admin");
              break;
            case "new_window":
              coreMod.invoke("open_new_window").catch(() => {});
              break;
          }
        });
        unlisteners.push(unlistenMenu);

        // Listen for deep link events (emitted by Rust when app is already running)
        const unlistenDeepLink = await eventMod.listen<DeepLinkRoute>(
          "deep-link",
          (event: TauriEvent<DeepLinkRoute>) => {
            handleDeepLinkRoute(event.payload, navigate);
          },
        );
        unlisteners.push(unlistenDeepLink);

        unlisteners.push(await eventMod.listen<string[]>("deep-link://new-url", (event) => {
          for (const url of event.payload) dispatchNativeAuthHandoff(url);
        }));

        // Check for a pending deep link from app launch
        try {
          const pending = await coreMod.invoke<DeepLinkRoute | null>(
            "consume_pending_deep_link",
          );
          if (pending) {
            handleDeepLinkRoute(pending, navigate);
          }
        } catch {
          // Command may not exist in non-Tauri environments
        }
      } catch {
        // @tauri-apps/api not available in browser
      }
    }

    setup();
    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, [navigate, setTheme, resolvedTheme]);
}
