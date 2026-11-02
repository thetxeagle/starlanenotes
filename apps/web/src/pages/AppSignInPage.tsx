import { useEffect, useMemo, useState } from "react";
import { Navigate } from "react-router-dom";
import { BrandLogo } from "@/components/BrandLogo";
import { useAuth } from "@/hooks/useAuth";
import {
  completeNativeAuthHandoffInBrowser,
  getBrowserNativeAuthHandoff,
  getNativeAuthReturnUrl,
  rememberBrowserNativeAuthHandoff,
} from "@/lib/nativeAuthHandoff";

type AppSignInStatus = "approving" | "approved" | "error";

export function AppSignInPage() {
  const { user, loading } = useAuth();
  const handoffId = useMemo(() => getBrowserNativeAuthHandoff(), []);
  const [status, setStatus] = useState<AppSignInStatus>("approving");
  const [error, setError] = useState("");

  useEffect(() => {
    rememberBrowserNativeAuthHandoff();
  }, []);

  useEffect(() => {
    if (!user || !handoffId) return;
    let active = true;

    void completeNativeAuthHandoffInBrowser()
      .then((approvedHandoffId) => {
        if (!active) return;
        setStatus("approved");
        window.setTimeout(() => {
          window.location.assign(getNativeAuthReturnUrl(approvedHandoffId));
        }, 100);
      })
      .catch((cause: unknown) => {
        if (!active) return;
        setStatus("error");
        setError(
          cause instanceof Error
            ? cause.message
            : "Unable to approve this app sign-in request.",
        );
      });

    return () => {
      active = false;
    };
  }, [handoffId, user]);

  if (loading) return null;

  if (!handoffId) {
    return (
      <AppSignInSurface
        title="App sign-in link invalid"
        description="Start sign-in again from the Starlane Notes app."
      />
    );
  }

  if (!user) {
    return (
      <Navigate
        to={`/login?native_handoff=${encodeURIComponent(handoffId)}`}
        replace
      />
    );
  }

  if (status === "error") {
    return (
      <AppSignInSurface
        title="Unable to sign in to the app"
        description={error}
      />
    );
  }

  return (
    <AppSignInSurface
      title={status === "approved" ? "App sign-in approved" : "Signing in to the app"}
      description={
        status === "approved"
          ? "Return to Starlane Notes to finish."
          : `Approving Starlane Notes for ${user.email}.`
      }
      returnUrl={status === "approved" ? getNativeAuthReturnUrl(handoffId) : undefined}
    />
  );
}

interface AppSignInSurfaceProps {
  title: string;
  description: string;
  returnUrl?: string;
}

export function AppSignInSurface({
  title,
  description,
  returnUrl,
}: AppSignInSurfaceProps) {
  return (
    <main className="app-viewport app-background flex items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <div className="mb-8">
          <BrandLogo size="lg" />
          <h1 className="mt-6 text-xl font-semibold text-text">{title}</h1>
          <p className="mt-2 text-sm leading-6 text-text-muted">{description}</p>
        </div>
        {returnUrl && (
          <a
            href={returnUrl}
            className="focus-ring flex h-9 w-full items-center justify-center rounded-button bg-primary px-6 text-sm font-medium text-white transition-opacity duration-100 hover:opacity-90"
          >
            Return to Starlane Notes
          </a>
        )}
      </div>
    </main>
  );
}
