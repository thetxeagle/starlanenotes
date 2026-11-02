import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const cases: [string, string][] = [
  ["empty", ""], ["crlf", "\r\n.notice {font-weight:700}\r\n"],
  ["nul", "\n/* \0 original */ .notice {font-weight:700}\n"],
  ["blocked", "\n@import 'https://invalid.example/original'; .notice {background:url(https://invalid.example/x)}\n"],
  ["oversize", `\n/*${"x".repeat(20_100)}*/\n.notice {font-weight:700}\n`],
  ["guidance-in-css", "\r\n<!-- starlane-template:v1:guidance:eyJpZCI6ImNzcyIsInRleHQiOiJPcmlnaW5hbCJ9 -->\r\n.notice {font-weight:700}\r\n"],
];
async function freshClient() {
  vi.resetModules();
  (await import("./launchMode")).setCurrentLaunchMode("local");
  (await import("./storageProviders")).setSyncProvider("none");
  return (await import("./dataClient")).dataClient;
}

describe("raw page CSS in actual local draft persistence", () => {
  beforeEach(() => {
    const storage = new Map<string, string>();
    const localStorage = {
      get length() { return storage.size; },
      key: (index: number) => [...storage.keys()][index] ?? null,
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => { storage.set(key, value); },
      removeItem: (key: string) => { storage.delete(key); },
    };
    vi.stubGlobal("localStorage", localStorage);
    const events = new EventTarget();
    vi.stubGlobal("window", { localStorage, addEventListener: events.addEventListener.bind(events), removeEventListener: events.removeEventListener.bind(events), dispatchEvent: events.dispatchEvent.bind(events) });
  });
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
  it.each(cases)("saves, cold-reloads, exports/imports and publishes exact %s span", async (_name, css) => {
    const block = `<!-- starlane:custom-css -->${css}<!-- /starlane:custom-css -->`;
    const raw = `Original body Ω\n${block}\n`;
    let client = await freshClient();
    const page = await client.createDocument("local-space", { title: "CSS original", content: raw });
    const draftId = page.draft_id!;
    expect((await client.getDocumentDraft("local-space", page.id, draftId)).content).toBe(raw);
    const edited = `Changed body Ω\n${block}\n`;
    await client.updateDocumentDraft("local-space", page.id, draftId, { title: "Renamed", content: edited });
    client = await freshClient();
    expect((await client.getDocumentDraft("local-space", page.id, draftId)).content).toBe(edited);
    const portable = JSON.parse(JSON.stringify(await client.exportLocalVaultSnapshot()));
    expect(portable.drafts.find((draft: { id: string }) => draft.id === draftId).content).toBe(edited);
    await (await import("./dataClient")).__resetLocalDataForTests();
    await client.importLocalVaultSnapshot(portable);
    client = await freshClient();
    expect((await client.getDocumentDraft("local-space", page.id, draftId)).content).toBe(edited);
    await client.publishDocumentDraft("local-space", page.id, draftId);
    client = await freshClient();
    expect((await client.getDocumentContent("local-space", page.id)).content).toBe(edited);
    const published = JSON.parse(JSON.stringify(await client.exportLocalVaultSnapshot()));
    expect(published.documents.find((document: { id: string }) => document.id === page.id).content).toBe(edited);
    expect(published.drafts.some((draft: { id: string }) => draft.id === draftId)).toBe(false);
  });
});
