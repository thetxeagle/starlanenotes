import { beforeEach, afterEach, it, expect, vi } from "vitest";
import { api } from "./api";
import { setCurrentLaunchMode } from "./launchMode";
import { setSyncProvider } from "./storageProviders";
import {
  discoverPendingUploads,
  retainedPendingUploadCleanups,
  cleanupPendingUpload,
  parsePendingUploadDiscovery,
  parsePendingUploadCleanupResult,
} from "./pendingUploadCleanup";
const attempt = "11111111-2222-3333-4444-555555555555",
  operation = "66666666-2222-3333-4444-555555555555";
const row = {
  attempt_id: attempt,
  producer_sha256: "a".repeat(64),
  claim_sha256: "b".repeat(64),
  producer_kind: "original_direct",
  original_operation_id: operation,
};
const listing = { availability: "available", reason: null, attempts: [row] };
const complete = {
  attempt_id: attempt,
  outcome: "physically_completed",
  reason: null,
  disposition_sha256: "c".repeat(64),
};
const token = (sid = "original") =>
  `e30.${btoa(JSON.stringify({ sub: "actor", sid, token_type: "access" }))}.fixture`;
beforeEach(() => {
  const store = new Map<string, string>();
  const storage = {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => {
      store.set(k, v);
    },
    removeItem: (k: string) => {
      store.delete(k);
    },
    get length() {
      return store.size;
    },
    key: (i: number) => [...store.keys()][i] ?? null,
  };
  vi.stubGlobal("localStorage", storage);
  vi.stubGlobal("window", {
    localStorage: storage,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  });
  vi.stubGlobal("navigator", { locks: { request: (_n: string, fn: () => unknown) => fn() } });
  setCurrentLaunchMode("server");
  setSyncProvider("none");
  api.setToken(token());
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
async function selection() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => Response.json(listing)),
  );
  return (await discoverPendingUploads("space", "page", () => {})).attempts[0];
}
it("unavailable is not proved absence", () => {
  expect(
    parsePendingUploadDiscovery({
      availability: "not_available",
      reason: "active_writer",
      attempts: [],
    }),
  ).toMatchObject({ availability: "not_available" });
  expect(() =>
    parsePendingUploadDiscovery({ ...listing, availability: "not_available" }),
  ).toThrow();
});
it.each([
  { ...listing, extra: true },
  { ...listing, attempts: [row, row] },
  { ...listing, attempts: [{ ...row, producer_sha256: "A".repeat(64) }] },
  { ...listing, attempts: Array.from({ length: 129 }, () => row) },
])("refuses contradictory/unbounded evidence", (v) =>
  expect(() => parsePendingUploadDiscovery(v)).toThrow(),
);
it("strict matching completion required", () => {
  expect(parsePendingUploadCleanupResult(complete, attempt).outcome).toBe("physically_completed");
  expect(() =>
    parsePendingUploadCleanupResult({ ...complete, attempt_id: operation }, attempt),
  ).toThrow();
  expect(() =>
    parsePendingUploadCleanupResult({ ...complete, disposition_sha256: null }, attempt),
  ).toThrow();
});
it("writes exact original evidence before HTTP", async () => {
  const s = await selection();
  const request = vi.fn(async (url: string, options?: RequestInit) => {
    expect(localStorage.getItem("starlane_pending_upload_cleanup_v1")).toContain(attempt);
    expect(url).toContain(`/${attempt}/cleanup`);
    expect(JSON.parse(options!.body as string)).toEqual({
      producer_sha256: row.producer_sha256,
      claim_sha256: row.claim_sha256,
    });
    return Response.json(complete);
  });
  vi.stubGlobal("fetch", request);
  await cleanupPendingUpload(s, () => {});
  expect(retainedPendingUploadCleanups("space", "page", () => {}).attempts).toHaveLength(0);
  expect(request).toHaveBeenCalledTimes(1);
});
it("lost ACK retries same original from durable journal", async () => {
  const s = await selection();
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => {
      throw new Error("lost ACK");
    }),
  );
  await expect(cleanupPendingUpload(s, () => {})).rejects.toThrow("lost ACK");
  const retained = retainedPendingUploadCleanups("space", "page", () => {}).attempts[0];
  expect(retained.row).toEqual(row);
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => Response.json(complete)),
  );
  await cleanupPendingUpload(retained, () => {});
  expect(retainedPendingUploadCleanups("space", "page", () => {}).attempts).toHaveLength(0);
});
it("foreign session cannot rebind or dispatch", async () => {
  const s = await selection();
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => {
      throw new Error("lost");
    }),
  );
  await expect(cleanupPendingUpload(s, () => {})).rejects.toThrow();
  api.setToken(token("foreign"));
  const request = vi.fn();
  vi.stubGlobal("fetch", request);
  expect(retainedPendingUploadCleanups("space", "page", () => {})).toEqual({
    attempts: [],
    otherContextCount: 1,
  });
  await expect(cleanupPendingUpload(s, () => {})).rejects.toThrow();
  expect(request).not.toHaveBeenCalled();
});
it("forged handle refused", async () => {
  const s = await selection();
  const request = vi.fn();
  vi.stubGlobal("fetch", request);
  await expect(cleanupPendingUpload(structuredClone(s), () => {})).rejects.toThrow(
    "Select an original",
  );
  expect(request).not.toHaveBeenCalled();
});
it("no Web Locks refuses before HTTP", async () => {
  const s = await selection();
  vi.stubGlobal("navigator", {});
  const request = vi.fn();
  vi.stubGlobal("fetch", request);
  await expect(cleanupPendingUpload(s, () => {})).rejects.toThrow("cross-window");
  expect(request).not.toHaveBeenCalled();
});
it("post-response session drift preserves unknown outcome", async () => {
  const s = await selection();
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => {
      api.setToken(token("changed"));
      return Response.json(complete);
    }),
  );
  await expect(cleanupPendingUpload(s, () => {})).rejects.toThrow();
  expect(localStorage.getItem("starlane_pending_upload_cleanup_v1")).toContain(attempt);
});
it("streamed oversize refused", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(new Uint8Array(65537))),
  );
  await expect(discoverPendingUploads("space", "page", () => {})).rejects.toThrow("bound");
});

it.each(["pending", "retained_review"])(
  "retains same original for manual %s retry",
  async (outcome) => {
    const s = await selection();
    const reply = {
      attempt_id: attempt,
      outcome,
      reason: "protected_reference",
      disposition_sha256: null,
    };
    const request = vi.fn(async () => Response.json(reply));
    vi.stubGlobal("fetch", request);
    await cleanupPendingUpload(s, () => {});
    const saved = retainedPendingUploadCleanups("space", "page", () => {}).attempts[0];
    await cleanupPendingUpload(saved, () => {});
    expect(request.mock.calls[0]).toEqual(request.mock.calls[1]);
    expect(saved.row).toEqual(row);
  },
);
it("journal failure refuses before dispatch", async () => {
  const s = await selection();
  localStorage.setItem("starlane_pending_upload_cleanup_v1", "corrupt");
  const request = vi.fn();
  vi.stubGlobal("fetch", request);
  await expect(cleanupPendingUpload(s, () => {})).rejects.toThrow();
  expect(request).not.toHaveBeenCalled();
});
it("journal substitution during request cannot remove evidence", async () => {
  const s = await selection();
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => {
      const saved = JSON.parse(localStorage.getItem("starlane_pending_upload_cleanup_v1")!);
      saved[0].documentId = "foreign-page";
      localStorage.setItem("starlane_pending_upload_cleanup_v1", JSON.stringify(saved));
      return Response.json(complete);
    }),
  );
  await expect(cleanupPendingUpload(s, () => {})).rejects.toThrow("receipt changed");
  expect(localStorage.getItem("starlane_pending_upload_cleanup_v1")).toContain("foreign-page");
});
it("page invalidation after dispatch keeps original receipt", async () => {
  const s = await selection();
  let current = true;
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => {
      current = false;
      return Response.json(complete);
    }),
  );
  await expect(
    cleanupPendingUpload(s, () => {
      if (!current) throw new Error("page changed");
    }),
  ).rejects.toThrow("page changed");
  expect(localStorage.getItem("starlane_pending_upload_cleanup_v1")).toContain(attempt);
});
