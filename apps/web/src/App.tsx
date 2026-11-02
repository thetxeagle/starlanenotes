import SharedWithMePage from "@/pages/SharedWithMePage";
import { lazy, Suspense, useEffect, useState } from "react";
import {
  createBrowserRouter,
  RouterProvider,
  Routes,
  Route,
  Navigate,
  Outlet,
  useLocation,
  type Location,
} from "react-router-dom";
import { AuthProvider } from "@/components/AuthProvider";
import { useAuth, type AuthConfig, type User } from "@/hooks/useAuth";
import { api, isTauri, replayOfflineServerMutations } from "@/lib/api";
import { AppLayout } from "@/components/layout/AppLayout";
import { MobileAppLayout } from "@/components/layout/MobileAppLayout";
import { ServerSetupPage } from "@/pages/ServerSetupPage";
import { StorageModeSetupPage } from "@/pages/StorageModeSetupPage";
import { useTauriEvents } from "@/hooks/useTauriEvents";
import { ThemeEventHandler } from "@/components/ThemeEventHandler";
import { LoadingState } from "@/components/ui/loading-state";
import { useRuntimePlatform } from "@/hooks/useRuntimePlatform";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import {
  MobileNotificationsScreen,
  MobileRecentScreen,
  MobileSearchScreen,
  MobileSpacesScreen,
} from "@/components/mobile/MobileScreens";
import {
  clearStorageSelection,
  getStorageSelection,
  hasStoredStorageSelection,
  setStorageMode,
  setSyncProvider,
} from "@/lib/storageProviders";
import { resolveLaunchMode, setCurrentLaunchMode } from "@/lib/launchMode";
import {
  clearVaultPathWithoutInitializing,
  createVaultFolder,
  getCorruptedVaultRecord,
  getVaultLocationStatus,
  markCorruptedVault,
  setVaultPath,
  type VaultLocation,
} from "@/lib/tauriVault";
import { startNativeBackgroundReplayScheduler } from "@/lib/backgroundReplay";
import { dataClient } from "@/lib/dataClient";
import { runLocalVaultStartupCheck } from "@/lib/localVault";
import { checkServerStartup, hasOfflineServerWorkspace } from "@/lib/serverStartup";
import {
  clearLastViewedDocumentRoute,
  readLastViewedDocumentRoute,
} from "@/lib/lastViewedDocument";
import { getPreferredSpaceHomePath } from "@/lib/lastViewedSpace";
import { shouldResolveDesktopStartupSpaceFallback } from "@/lib/startupRouting";

// Heavy pages stay lazy-loaded so authenticated startup does not parse editor,
// graph, journal, or workflow code before the user opens those routes.
const loadDocumentPage = () => import("@/pages/DocumentPage");
const loadGraphPage = () => import("@/pages/GraphPage");
const loadJournalPage = () => import("@/pages/JournalPage");
const loadWorkflowQueuePage = () => import("@/pages/WorkflowQueuePage");
const loadStarredDocumentsPage = () => import("@/pages/StarredDocumentsPage");
const loadRecentDocumentsPage = () => import("@/pages/RecentDocumentsPage");
const loadSpaceHomePage = () => import("@/pages/SpaceHomePage");

const LoginPage = lazy(() => import("@/pages/LoginPage").then((m) => ({ default: m.LoginPage })));
const AppSignInPage = lazy(() =>
  import("@/pages/AppSignInPage").then((m) => ({ default: m.AppSignInPage })),
);
const RegisterPage = lazy(() =>
  import("@/pages/RegisterPage").then((m) => ({ default: m.RegisterPage })),
);
const ResetPasswordPage = lazy(() =>
  import("@/pages/ResetPasswordPage").then((m) => ({ default: m.ResetPasswordPage })),
);
const DocumentPage = lazy(() => loadDocumentPage().then((m) => ({ default: m.DocumentPage })));
const AdminPage = lazy(() => import("@/pages/AdminPage").then((m) => ({ default: m.AdminPage })));
const SetupPage = lazy(() => import("@/pages/SetupPage").then((m) => ({ default: m.SetupPage })));
const GraphPage = lazy(() => loadGraphPage().then((m) => ({ default: m.GraphPage })));
const JournalPage = lazy(() => loadJournalPage().then((m) => ({ default: m.JournalPage })));
const WorkflowQueuePage = lazy(() =>
  loadWorkflowQueuePage().then((m) => ({ default: m.WorkflowQueuePage })),
);
const StarredDocumentsPage = lazy(() =>
  loadStarredDocumentsPage().then((m) => ({ default: m.StarredDocumentsPage })),
);
const RecentDocumentsPage = lazy(() =>
  loadRecentDocumentsPage().then((m) => ({ default: m.RecentDocumentsPage })),
);
const SpaceHomePage = lazy(() => loadSpaceHomePage().then((m) => ({ default: m.SpaceHomePage })));
const SpaceSettingsPage = lazy(() =>
  import("@/pages/SpaceSettingsPage").then((m) => ({ default: m.SpaceSettingsPage })),
);
const TemplateEditorPage = lazy(() =>
  import("@/pages/TemplateEditorPage").then((m) => ({ default: m.TemplateEditorPage })),
);
const AccountSettingsPage = lazy(() =>
  import("@/pages/AccountSettingsPage").then((m) => ({ default: m.AccountSettingsPage })),
);
const AppSettingsPage = lazy(() =>
  import("@/pages/AppSettingsPage").then((m) => ({ default: m.AppSettingsPage })),
);
const PublicPage = lazy(() =>
  import("@/pages/PublicPage").then((m) => ({ default: m.PublicPage })),
);

function AuthenticatedRoutePreloader() {
  // Route chunks intentionally load on navigation. Avoid an idle-time fan-out
  // that parses every heavy workspace surface in the background.
  return null;
}

function PageLoader({ label = "Loading page", viewport = false } = {}) {
  return <LoadingState label={label} viewport={viewport} />;
}

function safeLocalStorageGetItem(key: string): string | null {
  try {
    if (typeof window === "undefined") return null;
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

function safeLocalStorageSetItem(key: string, value: string): void {
  try {
    if (typeof window === "undefined") return;
    window.localStorage.setItem(key, value);
  } catch {
    // Startup recovery must continue even when WebKit storage is unavailable.
  }
}

function safeLocalStorageRemoveItem(key: string): void {
  try {
    if (typeof window === "undefined") return;
    window.localStorage.removeItem(key);
  } catch {
    // Startup recovery must continue even when WebKit storage is unavailable.
  }
}

export function shouldRedirectToRequiredTwoFactorProfile(
  config: Pick<AuthConfig, "require_2fa"> | null,
  user: Pick<User, "totp_enabled"> | null,
  pathname: string,
): boolean {
  if (!config?.require_2fa || !user || user.totp_enabled) return false;
  return (
    pathname !== "/profile" &&
    pathname !== "/profile/security" &&
    pathname !== "/mobile/profile" &&
    pathname !== "/mobile/profile/security"
  );
}

export function shouldUseMobileAppLayout(
  isMobileRuntime: boolean,
  isMobileBrowser: boolean,
): boolean {
  return isMobileRuntime || isMobileBrowser;
}

export interface SettingsOverlayLocationState {
  backgroundLocation?: Location;
}

export function getSettingsOverlayBackground(
  location: Location,
  mobileLayout: boolean,
): Location | null {
  if (mobileLayout) return null;
  const state = location.state as SettingsOverlayLocationState | null;
  return state?.backgroundLocation ?? null;
}

function ProtectedRoute() {
  const { user, loading, config } = useAuth();
  const { isMobileRuntime, isMobileBrowser, loading: runtimeLoading } = useRuntimePlatform();
  const location = useLocation();

  if (loading || runtimeLoading) {
    return <PageLoader label="Loading workspace" viewport />;
  }

  // Redirect to setup if not completed
  if (config && !config.setup_completed) {
    return <Navigate to="/setup" replace />;
  }

  if (!user) {
    return <Navigate to="/login" replace />;
  }

  if (shouldRedirectToRequiredTwoFactorProfile(config, user, location.pathname)) {
    return <Navigate to="/profile" replace />;
  }

  if (shouldUseMobileAppLayout(isMobileRuntime, isMobileBrowser)) {
    return (
      <MobileAppLayout>
        <AuthenticatedRoutePreloader />
        <Outlet />
      </MobileAppLayout>
    );
  }

  return (
    <AppLayout>
      <AuthenticatedRoutePreloader />
      <Outlet />
    </AppLayout>
  );
}

function GuestRoute() {
  const { user, loading, config } = useAuth();

  if (loading) return null;

  // Redirect to setup if not completed
  if (config && !config.setup_completed) {
    return <Navigate to="/setup" replace />;
  }

  if (user) return <Navigate to="/" replace />;

  return <Outlet />;
}

function SetupGuard() {
  const { config, loading } = useAuth();

  if (loading) return null;

  // Setup page is only accessible when setup hasn't been completed
  if (config?.setup_completed) {
    return <Navigate to="/" replace />;
  }

  return <Outlet />;
}

function HomePage() {
  const { isMobileRuntime, isMobileViewport } = useRuntimePlatform();
  const { config } = useAuth();
  const location = useLocation();
  const [restorePath, setRestorePath] = useState<string | null>(null);
  const [checkingRestore, setCheckingRestore] = useState(location.pathname === "/");
  const [desktopHomePath, setDesktopHomePath] = useState<string | null>(null);
  const [checkingDesktopHome, setCheckingDesktopHome] = useState(false);
  const [mobileHomePath, setMobileHomePath] = useState<string | null>(null);
  const mobileLayout = shouldUseMobileAppLayout(isMobileRuntime, isMobileViewport);
  const [checkingMobileHome, setCheckingMobileHome] = useState(
    () => location.pathname === "/" && mobileLayout,
  );
  const siteName = config?.site_name ?? "Starlane Notes";

  useEffect(() => {
    if (location.pathname !== "/" || mobileLayout) {
      setCheckingRestore(false);
      setRestorePath(null);
      return;
    }

    let cancelled = false;
    const lastViewed = readLastViewedDocumentRoute();
    if (!lastViewed) {
      setCheckingRestore(false);
      setRestorePath(null);
      return;
    }

    setCheckingRestore(true);
    dataClient
      .getDocument(lastViewed.spaceId, lastViewed.docId)
      .then(() => {
        if (!cancelled) setRestorePath(lastViewed.path);
      })
      .catch(() => {
        clearLastViewedDocumentRoute();
        if (!cancelled) setRestorePath(null);
      })
      .finally(() => {
        if (!cancelled) setCheckingRestore(false);
      });

    return () => {
      cancelled = true;
    };
  }, [location.pathname, mobileLayout]);

  useEffect(() => {
    if (
      !shouldResolveDesktopStartupSpaceFallback({
        pathname: location.pathname,
        isMobileRuntime: mobileLayout,
        checkingDocumentRestore: checkingRestore,
        documentRestorePath: restorePath,
      })
    ) {
      setCheckingDesktopHome(false);
      setDesktopHomePath(null);
      return;
    }

    let cancelled = false;
    setCheckingDesktopHome(true);
    dataClient
      .listSpaces()
      .then((spaces) => {
        if (!cancelled) setDesktopHomePath(getPreferredSpaceHomePath(spaces));
      })
      .catch(() => {
        if (!cancelled) setDesktopHomePath(null);
      })
      .finally(() => {
        if (!cancelled) setCheckingDesktopHome(false);
      });

    return () => {
      cancelled = true;
    };
  }, [checkingRestore, location.pathname, mobileLayout, restorePath]);

  useEffect(() => {
    if (location.pathname !== "/" || !mobileLayout) {
      setCheckingMobileHome(false);
      setMobileHomePath(null);
      return;
    }

    let cancelled = false;
    setCheckingMobileHome(true);
    dataClient
      .listSpaces()
      .then((spaces) => {
        if (!cancelled) setMobileHomePath(getPreferredSpaceHomePath(spaces));
      })
      .catch(() => {
        if (!cancelled) setMobileHomePath(null);
      })
      .finally(() => {
        if (!cancelled) setCheckingMobileHome(false);
      });

    return () => {
      cancelled = true;
    };
  }, [location.pathname, mobileLayout]);

  if (mobileLayout) {
    if (mobileHomePath) {
      return <Navigate to={mobileHomePath} replace />;
    }

    if (checkingMobileHome) {
      return <PageLoader />;
    }

    return <MobileSpacesScreen />;
  }

  if (restorePath) {
    return <Navigate to={restorePath} replace />;
  }

  if (desktopHomePath) {
    return <Navigate to={desktopHomePath} replace />;
  }

  if (checkingRestore || checkingDesktopHome) {
    return <PageLoader />;
  }

  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center">
        <h2 className="text-text text-lg font-medium">Welcome to {siteName}</h2>
        <p className="text-text-muted mt-1 text-sm">
          Select a document from the sidebar or create a new one.
        </p>
      </div>
    </div>
  );
}

function AppTitleManager() {
  const { config } = useAuth();
  const location = useLocation();
  const siteName = config?.site_name ?? "Starlane Notes";

  useEffect(() => {
    if (location.pathname === "/login") {
      document.title = `Login - ${siteName}`;
    } else if (location.pathname === "/register") {
      document.title = `Register - ${siteName}`;
    } else if (location.pathname === "/reset-password") {
      document.title = `Reset Password - ${siteName}`;
    } else if (location.pathname === "/setup") {
      document.title = `Setup - ${siteName}`;
    } else if (location.pathname === "/admin") {
      document.title = `Admin - ${siteName}`;
    } else if (/^\/spaces\/[^/]+\/settings\/?$/.test(location.pathname)) {
      document.title = `Space Settings - ${siteName}`;
    } else if (location.pathname === "/profile" || location.pathname === "/profile/security") {
      document.title = `Account Settings - ${siteName}`;
    } else if (!location.pathname.includes("/docs/")) {
      document.title = siteName;
    }
  }, [location.pathname, siteName]);

  return null;
}

function AppRoutes() {
  useTauriEvents();
  const location = useLocation();
  const { isMobileRuntime, isMobileViewport } = useRuntimePlatform();
  const mobileLayout = shouldUseMobileAppLayout(isMobileRuntime, isMobileViewport);
  const settingsBackground = getSettingsOverlayBackground(location, mobileLayout);

  return (
    <ErrorBoundary>
      <ThemeEventHandler />
      <AppTitleManager />
      <Suspense fallback={<PageLoader />}>
        <Routes location={settingsBackground ?? location}>
          {/* Public pages intentionally stay outside authenticated app chrome. */}
          <Route path="/p/:slug" element={<PublicPage />} />
          <Route path="/app-signin/" element={<AppSignInPage />} />

          {/* Setup wizard (only when no users exist) */}
          <Route element={<SetupGuard />}>
            <Route path="/setup" element={<SetupPage />} />
          </Route>

          {/* Guest routes (login, register) */}
          <Route element={<GuestRoute />}>
            <Route path="/login" element={<LoginPage />} />
            <Route path="/register" element={<RegisterPage />} />
            <Route path="/reset-password" element={<ResetPasswordPage />} />
            <Route path="/server-setup" element={<ServerSetupPage onConnected={() => {}} />} />
          </Route>

          {/* Protected routes (require auth) */}
          <Route element={<ProtectedRoute />}>
            <Route path="/" element={<HomePage />} />
            <Route path="/shared-with-me" element={<SharedWithMePage />} />
            <Route path="/spaces/:spaceId/docs" element={<SpaceHomePage />} />
            <Route path="/spaces/:spaceId/recent" element={<RecentDocumentsPage />} />
            <Route path="/spaces/:spaceId/docs/:docId" element={<DocumentPage />} />
            <Route path="/spaces/:spaceId/workflows" element={<WorkflowQueuePage />} />
            <Route path="/spaces/:spaceId/journal" element={<JournalPage />} />
            <Route path="/spaces/:spaceId/graph" element={<GraphPage />} />
            <Route path="/spaces/:spaceId/starred" element={<StarredDocumentsPage />} />
            <Route path="/spaces/:spaceId/settings" element={<SpaceSettingsPage />} />
            <Route path="/spaces/:spaceId/templates/new" element={<TemplateEditorPage />} />
            <Route
              path="/spaces/:spaceId/templates/:templateId/edit"
              element={<TemplateEditorPage />}
            />
            <Route path="/profile" element={<AccountSettingsPage />} />
            <Route
              path="/profile/security"
              element={<AccountSettingsPage initialSection="security" />}
            />
            <Route path="/admin" element={<AdminPage />} />
            <Route path="/mobile/recent" element={<MobileRecentScreen />} />
            <Route path="/mobile/search" element={<MobileSearchScreen />} />
            <Route path="/mobile/notifications" element={<MobileNotificationsScreen />} />
            <Route path="/mobile/app-settings" element={<AppSettingsPage />} />
            <Route path="/mobile/profile" element={<AccountSettingsPage />} />
            <Route
              path="/mobile/profile/security"
              element={<AccountSettingsPage initialSection="security" />}
            />
          </Route>
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
        {settingsBackground && (
          <Suspense fallback={null}>
            <Routes>
              <Route path="/spaces/:spaceId/settings" element={<SpaceSettingsPage />} />
              <Route path="/profile" element={<AccountSettingsPage />} />
              <Route
                path="/profile/security"
                element={<AccountSettingsPage initialSection="security" />}
              />
              <Route path="/admin" element={<AdminPage />} />
              <Route path="/mobile/app-settings" element={<AppSettingsPage />} />
            </Routes>
          </Suspense>
        )}
      </Suspense>
    </ErrorBoundary>
  );
}

function AppRouterHost({ mode }: { mode: "server" | "local" }) {
  const [router, setRouter] = useState<ReturnType<typeof createBrowserRouter> | null>(null);

  useEffect(() => {
    const nextRouter = createBrowserRouter([{ path: "*", element: <AppRoutes /> }], {
      future: { v7_relativeSplatPath: true },
    });
    setRouter(nextRouter);

    return () => {
      nextRouter.dispose();
      setRouter((current) => (current === nextRouter ? null : current));
    };
  }, [mode]);

  if (!router) return <PageLoader label="Loading application" viewport />;

  return (
    <AuthProvider mode={mode}>
      <RouterProvider router={router} future={{ v7_startTransition: true }} />
    </AuthProvider>
  );
}

export function shouldShowNativeServerSetup(input: {
  launchMode: "server" | "local";
  serverReady: boolean;
  tauriRuntime: boolean;
}): boolean {
  return input.tauriRuntime && input.launchMode === "server" && !input.serverReady;
}

export function App() {
  const { runtime, loading: runtimeLoading } = useRuntimePlatform();
  const tauriRuntime = isTauri();
  const [storageConfigured, setStorageConfigured] = useState(
    () => !tauriRuntime || hasStoredStorageSelection() || !!safeLocalStorageGetItem("server_url"),
  );
  const [serverReady, setServerReady] = useState(() => {
    if (!tauriRuntime) return true;
    const serverUrl = safeLocalStorageGetItem("server_url");
    return Boolean(
      serverUrl && hasOfflineServerWorkspace(serverUrl, api.getToken(), api.getRefreshToken()),
    );
  });
  const [startupCheck, setStartupCheck] = useState<{
    status: "checking" | "ready" | "error";
    message?: string;
  }>({
    status:
      !tauriRuntime ||
      Boolean(
        safeLocalStorageGetItem("server_url") &&
        hasOfflineServerWorkspace(
          safeLocalStorageGetItem("server_url") ?? "",
          api.getToken(),
          api.getRefreshToken(),
        ),
      )
        ? "ready"
        : "checking",
  });
  const [startupCheckRetry, setStartupCheckRetry] = useState(0);
  const launchResolution = resolveLaunchMode(getStorageSelection(), runtime);
  setCurrentLaunchMode(launchResolution.mode);

  useEffect(() => {
    return startNativeBackgroundReplayScheduler();
  }, [launchResolution.mode, tauriRuntime]);

  useEffect(() => {
    if (launchResolution.mode !== "server" || !serverReady) return;

    const replay = () => {
      void replayOfflineServerMutations().catch(() => undefined);
    };
    replay();
    window.addEventListener("online", replay);
    return () => window.removeEventListener("online", replay);
  }, [launchResolution.mode, serverReady]);

  useEffect(() => {
    if (
      runtimeLoading ||
      storageConfigured ||
      !tauriRuntime ||
      safeLocalStorageGetItem("server_url")
    ) {
      return;
    }

    let cancelled = false;
    getVaultLocationStatus()
      .then((status) => {
        if (cancelled || status.configured === false || getCorruptedVaultRecord(status.path)) {
          return;
        }
        setStorageMode("local");
        setSyncProvider("none");
        setStorageConfigured(true);
      })
      .catch(() => undefined);

    return () => {
      cancelled = true;
    };
  }, [runtimeLoading, storageConfigured, tauriRuntime]);

  useEffect(() => {
    if (runtimeLoading || !storageConfigured) return;
    if (launchResolution.mode === "server") {
      return;
    }

    let cancelled = false;
    setStartupCheck({ status: "checking" });
    const checkStorage = async () => {
      let activeVault: VaultLocation | null = null;
      if (tauriRuntime) {
        activeVault = await getVaultLocationStatus();
        if (activeVault.configured === false) {
          clearStorageSelection();
          setSyncProvider("none");
          if (!cancelled) {
            setStorageConfigured(false);
            setStartupCheck({ status: "ready" });
          }
          return;
        }
      }

      try {
        await runLocalVaultStartupCheck(launchResolution.selected.provider);
      } catch (error) {
        if (tauriRuntime && activeVault?.configured !== false && activeVault?.path) {
          const message = error instanceof Error ? error.message : "Vault health check failed.";
          markCorruptedVault(activeVault.path, message);
          await clearVaultPathWithoutInitializing().catch(() => undefined);
          clearStorageSelection();
          setSyncProvider("none");
          if (!cancelled) {
            setStorageConfigured(false);
            setStartupCheck({ status: "ready" });
          }
          return;
        }
        throw error;
      }
      if (!cancelled) setStartupCheck({ status: "ready" });
    };

    checkStorage().catch((error) => {
      if (!cancelled) {
        setStartupCheck({
          status: "error",
          message: error instanceof Error ? error.message : "Local storage could not be checked.",
        });
      }
    });

    return () => {
      cancelled = true;
    };
  }, [
    launchResolution.mode,
    launchResolution.selected.provider,
    runtimeLoading,
    startupCheckRetry,
    storageConfigured,
    tauriRuntime,
  ]);

  useEffect(() => {
    if (runtimeLoading || !storageConfigured || launchResolution.mode !== "server") return;

    const serverUrl = safeLocalStorageGetItem("server_url");
    if (!serverUrl) {
      setServerReady(false);
      setStartupCheck({ status: "ready" });
      return;
    }

    if (!tauriRuntime) {
      setServerReady(true);
      setStartupCheck({ status: "ready" });
      return;
    }

    let cancelled = false;
    const hasCachedWorkspace = hasOfflineServerWorkspace(
      serverUrl,
      api.getToken(),
      api.getRefreshToken(),
    );
    setServerReady(hasCachedWorkspace);
    setStartupCheck(
      hasCachedWorkspace
        ? { status: "ready", message: "Refreshing server connection…" }
        : { status: "checking", message: "Checking server connection…" },
    );

    checkServerStartup(serverUrl)
      .then((result) => {
        if (cancelled) return;
        safeLocalStorageSetItem("server_url", result.serverUrl);
        setServerReady(true);
        setStartupCheck({ status: "ready" });
      })
      .catch((error) => {
        if (cancelled) return;
        if (hasOfflineServerWorkspace(serverUrl, api.getToken(), api.getRefreshToken())) {
          setServerReady(true);
          setStartupCheck({
            status: "ready",
            message: "The linked server is unavailable. Opening the downloaded workspace offline.",
          });
          return;
        }
        const message =
          error instanceof Error
            ? error.message
            : "Starlane Notes could not reach the linked server.";
        console.error("[starlane:start] server startup check failed", {
          serverUrl,
          message,
        });
        setServerReady(false);
        setStartupCheck({ status: "error", message });
      });

    return () => {
      cancelled = true;
    };
  }, [launchResolution.mode, runtimeLoading, startupCheckRetry, storageConfigured, tauriRuntime]);

  const applyInitialLocalVault = async (name: string, path?: string) => {
    if (!tauriRuntime) return;
    if (path) {
      await setVaultPath(path);
    } else {
      await createVaultFolder(name);
    }
  };

  if (runtimeLoading) {
    return <PageLoader label="Loading application" viewport />;
  }

  if (!storageConfigured) {
    return (
      <StorageModeSetupPage
        onChooseLocal={async (vaultName, vaultPath) => {
          await applyInitialLocalVault(vaultName, vaultPath);
          setStorageMode("local");
          setSyncProvider("none");
          setStorageConfigured(true);
        }}
        onChooseServer={(serverUrl) => {
          if (serverUrl) {
            safeLocalStorageSetItem("server_url", serverUrl);
          }
          setStorageMode("server");
          setSyncProvider("none");
          setServerReady(false);
          setStorageConfigured(true);
        }}
      />
    );
  }

  if (
    launchResolution.mode === "server" &&
    safeLocalStorageGetItem("server_url") &&
    startupCheck.status === "checking"
  ) {
    return (
      <div className="app-viewport app-background flex items-center justify-center">
        <div className="text-text-muted text-sm">Checking server connection…</div>
      </div>
    );
  }

  if (launchResolution.mode === "server" && startupCheck.status === "error") {
    return (
      <div className="app-viewport app-background flex items-center justify-center p-4">
        <div className="rounded-card border-border bg-surface shadow-card border p-4">
          <h1 className="text-text text-base font-medium">Server connection needs attention</h1>
          <p className="text-text-muted mt-2 max-w-md text-sm">
            Starlane Notes could not reach the linked server, so it stopped before opening the
            remote vault. No local files were changed.
          </p>
          {startupCheck.message && (
            <p className="text-text-muted mt-2 max-w-md text-xs">{startupCheck.message}</p>
          )}
          <div className="mt-4 flex flex-col justify-end gap-2 sm:flex-row">
            <button
              type="button"
              onClick={() => setStartupCheckRetry((value) => value + 1)}
              className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text border px-3 py-1.5 text-sm transition-colors duration-100"
            >
              Retry connection
            </button>
            <button
              type="button"
              onClick={() => {
                safeLocalStorageRemoveItem("server_url");
                setServerReady(false);
                setStartupCheck({ status: "ready" });
              }}
              className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text border px-3 py-1.5 text-sm transition-colors duration-100"
            >
              Change server
            </button>
            <button
              type="button"
              onClick={() => {
                safeLocalStorageRemoveItem("server_url");
                clearStorageSelection();
                setSyncProvider("none");
                setServerReady(false);
                setStorageConfigured(false);
                setStartupCheck({ status: "ready" });
              }}
              className="rounded-button bg-primary px-3 py-1.5 text-sm font-medium text-white transition-opacity duration-100 hover:opacity-90"
            >
              Back to vault setup
            </button>
          </div>
        </div>
      </div>
    );
  }

  if (
    shouldShowNativeServerSetup({
      launchMode: launchResolution.mode,
      serverReady,
      tauriRuntime,
    })
  ) {
    return (
      <ServerSetupPage
        onConnected={(serverUrl) => {
          if (serverUrl) {
            safeLocalStorageSetItem("server_url", serverUrl);
          }
          setStorageMode("server");
          setSyncProvider("none");
          setServerReady(true);
          setStartupCheck({ status: "ready" });
        }}
        onBack={
          tauriRuntime
            ? () => {
                safeLocalStorageRemoveItem("server_url");
                clearStorageSelection();
                setSyncProvider("none");
                setServerReady(false);
                setStorageConfigured(false);
              }
            : undefined
        }
      />
    );
  }

  if (launchResolution.mode === "local" && startupCheck.status === "checking") {
    return (
      <div className="app-viewport app-background flex items-center justify-center">
        <div className="text-text-muted text-sm">Checking local storage…</div>
      </div>
    );
  }

  if (launchResolution.mode === "local" && startupCheck.status === "error") {
    return (
      <div className="app-viewport app-background flex items-center justify-center p-4">
        <div className="rounded-card border-border bg-surface shadow-card border p-4">
          <h1 className="text-text text-base font-medium">Storage check needs attention</h1>
          <p className="text-text-muted mt-2 max-w-md text-sm">
            Starlane Notes could not confirm that local storage is active and structurally safe. No
            local records were deleted.
          </p>
          {startupCheck.message && (
            <p className="text-text-muted mt-2 max-w-md text-xs">{startupCheck.message}</p>
          )}
          <div className="mt-4 flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setStartupCheckRetry((value) => value + 1)}
              className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text border px-3 py-1.5 text-sm transition-colors duration-100"
            >
              Retry check
            </button>
            <button
              type="button"
              onClick={() => setStartupCheck({ status: "ready" })}
              className="rounded-button bg-primary px-3 py-1.5 text-sm font-medium text-white transition-opacity duration-100"
            >
              Continue
            </button>
          </div>
        </div>
      </div>
    );
  }

  return <AppRouterHost mode={launchResolution.mode} />;
}
