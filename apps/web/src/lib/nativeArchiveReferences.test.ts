import { describe, expect, it } from "vitest";
import type { LocalVaultSnapshot } from "@/lib/localVault";
import { prepareArchiveAssetReferences } from "./nativeArchiveReferences";

const old = "asset://localhost/%2Fold%20vault%2Fimage.png";
const url = "asset://localhost/%2Fvault%2Fversion-1.png";
const asset = { id: "raw:asset", name: "image.png", source: "tauri", url: old, vault_path: "/old vault/image.png", size: 3, created_at: " raw time ", future: { raw: true } };
const version = { index: 0, id: asset.id, url, vault_path: "/vault/version-1.png", size: 5 };
const snapshot = (content: string) => ({ assets: [asset], documents: [{ id: "page", content, updated_at: "unchanged", cover_image: old }], future: { note: old }, sync_state: { queued_operations: [{ id: "q", payload: { body: old }, server_context: { account_id: "original" } }] } });

describe("prepared native archive reference spans (joint commit not yet consumed)", () => {
  it("patches only recognized destination bytes and exact cover fields", () => {
    const body = `prose ${old}\n![photo](<${old}> "caption")\n[download](${old} 'title')\n\`${old}\`\n\n\`\`\`text\n![code](${old})\n\`\`\`\n`;
    const original = snapshot(body);
    const saved = JSON.stringify(original);
    const result = prepareArchiveAssetReferences(original, [version]);
    expect(result.snapshot.documents[0].content).toBe(body.replace(`(<${old}>`, `(<${url}>`).replace(`(${old} 'title')`, `(${url} 'title')`));
    expect(result.snapshot.documents[0].cover_image).toBe(url);
    expect(result.snapshot.future).toEqual(original.future);
    expect(result.snapshot.sync_state).toBe(original.sync_state);
    expect(result.snapshot.assets[0]).toEqual({ ...asset, ...{ url, vault_path: version.vault_path, size: 5 } });
    expect(JSON.stringify(original)).toBe(saved);
    expect(result.changes).toHaveLength(3);
  });

  it("covers every declared draft/template body while preserving timestamps and unknown fields", () => {
    const original = { ...snapshot(""), drafts: [{ id: "d", content: `![x](${old})`, cover_image: old, owner_id: "u", updated_at: "raw" }], templates: [{ id: "t", content: `[x](<${asset.vault_path}>)`, properties: { literal: old }, tags: [old], future: old }] };
    const result = prepareArchiveAssetReferences(original, [version]);
    expect(result.snapshot.drafts[0]).toEqual({ ...original.drafts[0], content: `![x](${url})`, cover_image: url });
    expect(result.snapshot.templates[0]).toEqual({ ...original.templates[0], content: `[x](<${url}>)` });
  });

  it("uses the actual LocalVaultSnapshot drafts collection and preserves an unknown dual-key field", () => {
    const original: LocalVaultSnapshot & { document_drafts: unknown; future: unknown } = {
      schema_version: 1, spaces: [], folders: [],
      documents: [{ id: "page", space_id: "s", title: "Original", content: `![x](${old})`, updated_at: " raw page time " }],
      drafts: [{ id: "draft", document_id: "page", space_id: "s", owner_id: "original-owner", title: "Draft", content: `[x](${old})`, cover_image: old, base_updated_at: " raw base ", created_at: " raw created ", updated_at: " raw updated " }],
      assets: [{ ...asset, source: "tauri" }],
      templates: [{ id: "template", space_id: "s", name: " Raw name ", content: `![x](${old})`, tags: [old], properties: { literal: old }, created_at: " raw created ", updated_at: " raw updated " }],
      document_drafts: [{ id: "unknown-family", content: old, future: [old] }],
      future: { raw: old },
      sync_state: { state: "idle", updated_at: "raw", queued_operations: [{ id: "original-q", entity_type: "document", entity_id: "page", action: "update_content", queued_at: "raw", attempts: 0, payload: { body: old }, replay_provider: "google-drive", server_context: { server_base: "https://original.test/api/v1", account_id: "original-account", session_scope: "original-session" } }] },
    };
    const bytes = JSON.stringify(original);
    const plainSnapshot: LocalVaultSnapshot = original;
    const plainResult = prepareArchiveAssetReferences(plainSnapshot, [version]);
    expect(plainResult.snapshot.drafts![0].content).toBe(`[x](${url})`);
    const result = prepareArchiveAssetReferences(original, [version]);
    expect(result.snapshot.drafts![0]).toEqual({ ...original.drafts![0], content: `[x](${url})`, cover_image: url });
    expect(result.snapshot.documents[0].content).toBe(`![x](${url})`);
    expect(result.snapshot.templates[0]).toEqual({ ...original.templates[0], content: `![x](${url})` });
    expect(result.snapshot.document_drafts).toBe(original.document_drafts);
    expect(result.snapshot.sync_state).toBe(original.sync_state);
    expect(result.snapshot.future).toBe(original.future);
    expect(result.changes.some((change) => change.collection === "drafts")).toBe(true);
    expect(JSON.stringify(original)).toBe(bytes);
  });

  it("maps reference definitions but preserves same-label code and near-prefix destinations", () => {
    const body = `[x][photo]\n\n[photo]: <${old}> "caption"\n\n    [photo]: <${old}>\n\n![other](${old}.bak)\n`;
    expect(prepareArchiveAssetReferences(snapshot(body), [version]).snapshot.documents[0].content).toBe(body.replace(`[photo]: <${old}> "caption"`, `[photo]: <${url}> "caption"`));
  });

  it.each([
    `[photo]:\n  <${old}> \"caption\"\n\n[x][photo]\n`,
    `> [photo]:\n>   <${old}> \"caption\"\n>\n> [x][photo]\n`,
    `- [photo]:\n    <${old}> \"caption\"\n\n[x][photo]\n`,
    `[ph\\*oto]: <${old}>\n\n[x][ph\\*oto]\n`,
    `[photo]:\r\n  <${old}>\r\n\r\n[x][photo]\r\n`,
  ])("binds accepted multiline/container/escaped-label definitions to original spans: %s", (body) => {
    const result = prepareArchiveAssetReferences(snapshot(body), [version]);
    expect(result.snapshot.documents[0].content).toBe(body.replace(old, url));
    expect(result.changes.filter((change) => change.field === "content")).toHaveLength(1);
  });

  it("does not borrow first-definition ownership for shadowed duplicates or code", () => {
    const body = `[photo]: <${old}> \"first\"\n[photo]: <${old}> \"shadowed\"\n\n[x][photo]\n\n\`\`\`text\n[photo]: <${old}>\n\`\`\`\n`;
    expect(prepareArchiveAssetReferences(snapshot(body), [version]).snapshot.documents[0].content).toBe(body.replace(`<${old}> \"first\"`, `<${url}> \"first\"`));
    const firstExternal = `[photo]: https://example.test/first\n[photo]: <${old}>\n\n[x][photo]`;
    expect(prepareArchiveAssetReferences(snapshot(firstExternal), [version]).snapshot.documents[0].content).toBe(firstExternal);
  });

  it("maps actual inline container destinations or rejects unsupported raw-prefix grammar without output", () => {
    const body = `> ![photo](${old})\n\n- [download](\n  <${old}> \"title\")\n`;
    const result = prepareArchiveAssetReferences(snapshot(body), [version]);
    expect(result.snapshot.documents[0].content).toBe(body.split(old).join(url));
  });

  it("rejects real container-inline destinations when raw-prefix parsing cannot prove the same endpoint", () => {
    const body = `> [x](\n> <${old}>)\n`;
    const original = snapshot(body);
    const bytes = JSON.stringify(original);
    expect(() => prepareArchiveAssetReferences(original, [version])).toThrow(/unsupported/);
    expect(JSON.stringify(original)).toBe(bytes);
  });

  it("reports UTF-16 spans with Unicode prefixes and preserves CRLF outside destinations", () => {
    const body = `🦅 café\r\n\r\n[photo]: <${old}> \"标题\"\r\n\r\n![图][photo]\r\n`;
    const result = prepareArchiveAssetReferences(snapshot(body), [version]);
    const change = result.changes.find((value) => value.field === "content")!;
    expect(change.offset_unit).toBe("utf16_code_units");
    expect(change.start).toBe(body.indexOf(old));
    expect(change.end).toBe(body.indexOf(old) + old.length);
    expect(body.slice(change.start, change.end)).toBe(old);
    expect(new TextEncoder().encode(body.slice(0, change.start)).length).not.toBe(change.start);
    expect(result.snapshot.documents[0].content).toBe(body.replace(old, url));
  });

  it("maps parser-accepted native autolinks and preserves unrelated autolinks", () => {
    const original = snapshot(`<${old}>`);
    const bytes = JSON.stringify(original);
    const mapped = prepareArchiveAssetReferences(original, [version]);
    expect(mapped.snapshot.documents[0].content).toBe(`<${url}>`);
    expect(mapped.changes).toEqual([
      { collection:"documents", index:0, field:"content", offset_unit:"utf16_code_units", original:old, replacement:url, start:1, end:old.length+1 },
      { collection:"documents", index:0, field:"cover_image", offset_unit:"utf16_code_units", original:old, replacement:url, start:0, end:old.length },
    ]);
    expect(mapped.snapshot.documents[0].cover_image).toBe(url);
    expect(JSON.stringify(original)).toBe(bytes);
    const unrelated = `<https://example.test/normal> and prose ${old}`;
    expect(prepareArchiveAssetReferences(snapshot(unrelated), [version]).snapshot.documents[0].content).toBe(unrelated);
  });

  it("rejects shared literal aliases across distinct installed or skipped assets before returning", () => {
    const other = { ...asset, id: "other" };
    const original = { ...snapshot(`![x](${old})`), assets: [asset, other] };
    const bytes = JSON.stringify(original);
    expect(() => prepareArchiveAssetReferences(original, [version])).toThrow(/ambiguous/);
    expect(() => prepareArchiveAssetReferences(original, [version, { ...version, index: 1, id: "other", url: `${url}-other` }])).toThrow(/ambiguous/);
    expect(JSON.stringify(original)).toBe(bytes);
  });

  it("does not guess basenames, external URLs, unknown HTML attributes or opaque drawing JSON", () => {
    const body = `![name](image.png)\n![external](https://example.test/image.png)\n<span data-future="${old}">text</span>\n\n\`\`\`json\n{"drawing":{"src":"${old}"}}\n\`\`\``;
    expect(prepareArchiveAssetReferences(snapshot(body), [version]).snapshot.documents[0].content).toBe(body);
  });

  it("retains mixed original indices and rejects duplicate/misbound projections", () => {
    const original = { ...snapshot(`![x](${old})`), assets: [{ id: "browser", source: "browser", url: "data:x" }, asset] };
    expect(prepareArchiveAssetReferences(original, [{ ...version, index: 1 }]).snapshot.assets[0]).toBe(original.assets[0]);
    expect(() => prepareArchiveAssetReferences(original, [version])).toThrow();
    expect(() => prepareArchiveAssetReferences(original, [{ ...version, index: 1 }, { ...version, index: 1 }])).toThrow();
  });

  it.each(["video", "gallery", "carousel"])("blocks %s remap until the actual portable renderer can consume native version URLs", (kind) => {
    const source = kind === "video" ? `src: ${old}` : `---\n![x](${old})`;
    expect(() => prepareArchiveAssetReferences(snapshot(`\`\`\`${kind}\n${source}\n\`\`\`\n`), [version])).toThrow(/unsupported/);
  });
});
