import { api, getApiBase } from "@/lib/api";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { getSyncProvider } from "@/lib/storageProviders";

/** Fence composed display loads across waits; this is identity evidence, not permission. */
export function capturePageMetadataLoadGuard(): () => void {
  const original = {
    mode: getCurrentLaunchMode(),
    base: getApiBase(),
    token: api.getToken(),
    refresh: api.getRefreshToken(),
    provider: getSyncProvider(),
  };
  return () => {
    if (
      getCurrentLaunchMode() !== original.mode ||
      getApiBase() !== original.base ||
      api.getToken() !== original.token ||
      api.getRefreshToken() !== original.refresh ||
      getSyncProvider() !== original.provider
    )
      throw new Error("Account or workspace changed. Reopen page details in the current account.");
  };
}
