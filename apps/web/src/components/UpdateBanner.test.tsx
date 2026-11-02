import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  applyUpdateAvailableStatus,
  claimUpdateInstall,
  clampUpdateProgress,
  installDesktopUpdate,
  releaseTauriCore,
  releaseUpdateInstall,
  retainUpdateListener,
  UpdateDownloadProgress,
} from "./UpdateBanner";

describe("UpdateBanner", () => {
  it("immediately removes a listener that resolves after disposal", () => {
    let removed = 0;
    const listeners: Array<() => void> = [];

    const retained = retainUpdateListener(
      () => {
        removed += 1;
      },
      () => true,
      listeners,
    );

    expect(retained).toBe(false);
    expect(removed).toBe(1);
    expect(listeners).toHaveLength(0);
  });

  it("keeps install failures visible and retryable", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/UpdateBanner.tsx"), "utf8");

    expect(source).toContain('setStatus("error")');
    expect(source).toContain('role="alert"');
    expect(source).toContain("Retry update");
  });

  it("surfaces updater setup failures with a dedicated retry", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/UpdateBanner.tsx"), "utf8");

    expect(source).toContain('setErrorMessage("Update service could not be started.")');
    expect(source).toContain('setSetupAttempt((attempt) => attempt + 1)');
    expect(source).toContain('"Retry setup"');
  });

  it("fails instead of hanging when the desktop updater bridge is unavailable", async () => {
    await expect(installDesktopUpdate(undefined)).rejects.toThrow(
      "Desktop updater bridge is unavailable.",
    );
  });

  it("invokes the desktop updater bridge when it is ready", async () => {
    const calls: string[] = [];
    const core = {
      invoke: async <T,>(command: string) => {
        calls.push(command);
        return undefined as T;
      },
    };

    await expect(installDesktopUpdate(core)).resolves.toBeUndefined();
    expect(calls).toEqual(["install_update"]);
  });

  it("releases only the updater bridge owned by the disposing setup", () => {
    const oldCore = { invoke: async <T,>() => undefined as T };
    const newCore = { invoke: async <T,>() => undefined as T };
    const registry: Record<string, unknown> = { __tauriCore: newCore };

    expect(releaseTauriCore(registry, oldCore)).toBe(false);
    expect(registry.__tauriCore).toBe(newCore);
    expect(releaseTauriCore(registry, newCore)).toBe(true);
    expect(registry).not.toHaveProperty("__tauriCore");
  });

  it("keeps every updater action out of implicit form submission", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/UpdateBanner.tsx"), "utf8");

    expect(source.match(/type="button"/g)).toHaveLength(5);
  });

  it("does not regress an active or completed download on repeated availability events", () => {
    expect(applyUpdateAvailableStatus("idle")).toBe("available");
    expect(applyUpdateAvailableStatus("error")).toBe("available");
    expect(applyUpdateAvailableStatus("downloading")).toBe("downloading");
    expect(applyUpdateAvailableStatus("ready")).toBe("ready");
  });

  it("claims desktop installation synchronously", () => {
    const lock = { pending: false };

    expect(claimUpdateInstall(lock)).toBe(true);
    expect(claimUpdateInstall(lock)).toBe(false);
    releaseUpdateInstall(lock);
    expect(claimUpdateInstall(lock)).toBe(true);
  });

  it("clamps updater progress to the supported percentage range", () => {
    expect(clampUpdateProgress(-5)).toBe(0);
    expect(clampUpdateProgress(42.6)).toBe(43);
    expect(clampUpdateProgress(120)).toBe(100);
  });

  it("exposes download progress with native progressbar semantics", () => {
    const html = renderToStaticMarkup(<UpdateDownloadProgress progress={64} />);

    expect(html).toContain('role="progressbar"');
    expect(html).toContain('aria-label="Update download progress"');
    expect(html).toContain('aria-valuenow="64"');
    expect(html).toContain("64%");
  });
});
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
