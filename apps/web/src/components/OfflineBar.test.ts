import { describe, expect, it } from "vitest";
import { shouldShowOfflineMessage } from "./OfflineBar";

describe("shouldShowOfflineMessage", () => {
  it("shows offline messaging only for disconnected server workspaces", () => {
    expect(shouldShowOfflineMessage(false, "server")).toBe(true);
    expect(shouldShowOfflineMessage(true, "server")).toBe(false);
    expect(shouldShowOfflineMessage(false, "local")).toBe(false);
    expect(shouldShowOfflineMessage(true, "local")).toBe(false);
  });
});
