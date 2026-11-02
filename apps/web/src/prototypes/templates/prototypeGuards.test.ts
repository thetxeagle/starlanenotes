import { describe, expect, it, vi } from "vitest";

const storageWrite = vi.fn();
const apiWrite = vi.fn();
vi.mock("@/lib/dataClient", () => ({
  dataClient: { saveBrowserAsset: (...args: unknown[]) => storageWrite(...args) },
}));
vi.mock("@/lib/api", () => ({
  api: { fetch: (...args: unknown[]) => apiWrite(...args) },
}));

import { api } from "@/lib/api";
import { dataClient } from "@/lib/dataClient";
import { installTemplatePrototypeGuards } from "./prototypeGuards";

describe("standalone template editor isolation", () => {
  it("rejects editor asset persistence and backend actions before they reach real services", async () => {
    installTemplatePrototypeGuards();
    await expect(
      dataClient.saveBrowserAsset({
        name: "fixture.png",
        size: 0,
        url: "data:image/png;base64,fixture",
      }),
    ).rejects.toThrow("fixture prototype");
    await expect(
      api.fetch("/bookmarks/metadata", { method: "POST", body: { url: "https://example.com" } }),
    ).rejects.toThrow("fixture prototype");
    expect(storageWrite).not.toHaveBeenCalled();
    expect(apiWrite).not.toHaveBeenCalled();
  });
});
