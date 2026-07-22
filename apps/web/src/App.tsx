import { lazy, Suspense, useEffect, useState } from "react";
import { BrowserRouter, Routes, Route, Navigate, Outlet, useLocation } from "react-router-dom";
import { AuthProvider } from "@/components/AuthProvider";
import { useAuth, type AuthConfig, type User } from "@/hooks/useAuth";
import { api, isTauri, replayOfflineServerMutations } from "@/lib/api";
import { AppLayout } from "@/components/layout/AppLayout";
import { MobileAppLayout } from "@/components/layout/MobileAppLayout";
import { ServerSetupPage } from "@/pages/ServerSetupPage";
import { StorageModeSetupPage } from "@/pages/StorageModeSetupPage";
import { useTauriEvents } from "@/hooks/useTauriEvents";
import { useRuntimePlatform } from "@/hooks/useRuntimePlatform";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import {
  MobileNotificationsScreen,
  MobileProfileScreen,
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
import { clearLastViewedDocumentRoute, readLastViewedDocumentRoute } from "@/lib/lastViewedDocument";
import { getPreferredSpaceHomePath } from "@/lib/lastViewedSpace";
import { shouldResolveDesktopStartupSpaceFallback } from "@/lib/startupRouting";

// Heavy pages — lazy-loaded to split TipTap and admin code into separate chunks
const LoginPage = lazy(() => import("@/pages/LoginPage").then((m) => ({ default: m.LoginPage })));
const RegisterPage = lazy(() =>
  import("@/pages/RegisterPage").then((m) => ({ default: m.RegisterPage })),
);
const ResetPasswordPage = lazy(() =>
  import("@/pages/ResetPasswordPage").then((m) => ({ default: m.ResetPasswordPage })),
);
const DocumentPage = lazy(() =>
  import("@/pages/DocumentPage").then((m) => ({ default: m.DocumentPage })),
);
const AdminPage = lazy(() => import("@/pages/AdminPage").then((m) => ({ default: m.AdminPage })));
const SetupPage = lazy(() => import("@/pages/SetupPage").then((m) => ({ default: m.SetupPage })));
const GraphPage = lazy(() => import("@/pages/GraphPage").then((m) => ({ default: m.GraphPage })));
const JournalPage = lazy(() =>
  import("@/pages/JournalPage").then((m) => ({ default: m.JournalPage })),
);
const WorkflowQueuePage = lazy(() =>
  import("@/pages/WorkflowQueuePage").then((m) => ({ default: m.WorkflowQueuePage })),
);
const StarredDocumentsPage = lazy(() =>
  import("@/pages/StarredDocumentsPage").then((m) => ({ default: m.StarredDocumentsPage })),
);
const RecentDocumentsPage = lazy(() =>
  import("@/pages/RecentDocumentsPage").then((m) => ({ default: m.RecentDocumentsPage })),
);
const SpaceHomePage = lazy(() =>
  import("@/pages/SpaceHomePage").then((m) => ({ default: m.SpaceHomePage })),
);
const SpaceSettingsPage = lazy(() =>
  import("@/pages/SpaceSettingsPage").then((m) => ({ default: m.SpaceSettingsPage })),
);
const PublicPage = lazy(() => import("@/pages/PublicPage").then((m) => ({ default: m.PublicPage })));

function PageLoader() {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-text-muted text-sm">Loading…</div>
    </div>
  );
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
  return pathname !== "/profile" && pathname !== "/mobile/profile";
}

export function shouldUseMobileAppLayout(
  isMobileRuntime: boolean,
  isMobileViewport: boolean,
): boolean {
  return isMobileRuntime || isMobileViewport;
}

function ProtectedRoute() {
  const { user, loading, config } = useAuth();
  const { isMobileRuntime, isMobileViewport, loading: runtimeLoading } = useRuntimePlatform();
  const location = useLocation();

  if (loading || runtimeLoading) {
    return (
      <div className="app-viewport app-background flex items-center justify-center">
        <div className="text-text-muted text-sm">Loading...</div>
      </div>
    );
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

  if (shouldUseMobileAppLayout(isMobileRuntime, isMobileViewport)) {
    return (
      <MobileAppLayout>
        <Outlet />
      </MobileAppLayout>
    );
  }

  return (
    <AppLayout>
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
  const [checkingMobileHome, setCheckingMobileHome] = useState(() => location.pathname === "/" && mobileLayout);
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
    } else if (location.pathname === "/profile") {
      document.title = `Profile - ${siteName}`;
    } else if (!location.pathname.includes("/docs/")) {
      document.title = siteName;
    }
  }, [location.pathname, siteName]);

  return null;
}

function AppRoutes() {
  useTauriEvents();

  return (
    <ErrorBoundary>
      <AppTitleManager />
      <Suspense fallback={<PageLoader />}>
        <Routes>
          {/* Public pages intentionally stay outside authenticated app chrome. */}
          <Route path="/p/:slug" element={<PublicPage />} />

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
            <Route path="/spaces/:spaceId/docs" element={<SpaceHomePage />} />
            <Route path="/spaces/:spaceId/recent" element={<RecentDocumentsPage />} />
            <Route path="/spaces/:spaceId/docs/:docId" element={<DocumentPage />} />
            <Route path="/spaces/:spaceId/workflows" element={<WorkflowQueuePage />} />
            <Route path="/spaces/:spaceId/journal" element={<JournalPage />} />
            <Route path="/spaces/:spaceId/graph" element={<GraphPage />} />
            <Route path="/spaces/:spaceId/starred" element={<StarredDocumentsPage />} />
            <Route path="/spaces/:spaceId/settings" element={<SpaceSettingsPage />} />
            <Route path="/profile" element={<MobileProfileScreen scope="profile" />} />
            <Route path="/admin" element={<AdminPage />} />
            <Route path="/mobile/recent" element={<MobileRecentScreen />} />
            <Route path="/mobile/search" element={<MobileSearchScreen />} />
            <Route path="/mobile/notifications" element={<MobileNotificationsScreen />} />
            <Route path="/mobile/app-settings" element={<MobileProfileScreen scope="app" />} />
            <Route path="/mobile/profile" element={<MobileProfileScreen scope="profile" />} />
          </Route>
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </Suspense>
    </ErrorBoundary>
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
  const [serverReady, setServerReady] = useState(() => !tauriRuntime);
  const [startupCheck, setStartupCheck] = useState<{
    status: "checking" | "ready" | "error";
    message?: string;
  }>({ status: "checking" });
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
    if (runtimeLoading || storageConfigured || !tauriRuntime || safeLocalStorageGetItem("server_url")) {
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
          const message =
            error instanceof Error ? error.message : "Vault health check failed.";
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
          message:
            error instanceof Error ? error.message : "Local storage could not be checked.",
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
    setServerReady(false);
    setStartupCheck({ status: "checking", message: "Checking server connection…" });

    checkServerStartup(serverUrl)
      .then((result) => {
        if (cancelled) return;
        safeLocalStorageSetItem("server_url", result.serverUrl);
        setServerReady(true);
        setStartupCheck({ status: "ready" });
      })
      .catch((error) => {
        if (cancelled) return;
        if (hasOfflineServerWorkspace(serverUrl, api.getToken())) {
          setServerReady(true);
          setStartupCheck({
            status: "ready",
            message: "The linked server is unavailable. Opening the downloaded workspace offline.",
          });
          return;
        }
        const message =
          error instanceof Error ? error.message : "Starlane Notes could not reach the linked server.";
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
  }, [
    launchResolution.mode,
    runtimeLoading,
    startupCheckRetry,
    storageConfigured,
    tauriRuntime,
  ]);

  const applyInitialLocalVault = async (name: string, path?: string) => {
    if (!tauriRuntime) return;
    if (path) {
      await setVaultPath(path);
    } else {
      await createVaultFolder(name);
    }
  };

  if (runtimeLoading) {
    return (
      <div className="app-viewport app-background flex items-center justify-center">
        <div className="text-text-muted text-sm">Loading...</div>
      </div>
    );
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
        <div className="rounded-card border border-border bg-surface p-4 shadow-card">
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
              className="rounded-button border border-border px-3 py-1.5 text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
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
              className="rounded-button border border-border px-3 py-1.5 text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
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
        <div className="rounded-card border border-border bg-surface p-4 shadow-card">
          <h1 className="text-text text-base font-medium">Storage check needs attention</h1>
          <p className="text-text-muted mt-2 max-w-md text-sm">
            Starlane Notes could not confirm that local storage is active and structurally safe.
            No local records were deleted.
          </p>
          {startupCheck.message && (
            <p className="text-text-muted mt-2 max-w-md text-xs">{startupCheck.message}</p>
          )}
          <div className="mt-4 flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setStartupCheckRetry((value) => value + 1)}
              className="rounded-button border border-border px-3 py-1.5 text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
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

  return (
    <BrowserRouter>
      <AuthProvider mode={launchResolution.mode}>
        <AppRoutes />
      </AuthProvider>
    </BrowserRouter>
  );
}
