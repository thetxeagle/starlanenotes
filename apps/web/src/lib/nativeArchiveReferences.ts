import type { LocalVaultSnapshot } from "./localVault";
import { nativeJsonValueEquals } from "./nativeLocalDatabase";
import MarkdownIt from "markdown-it";
import installedReferenceRule from "markdown-it/lib/rules_block/reference.mjs";
import installedAutolinkRule from "markdown-it/lib/rules_inline/autolink.mjs";

/** Exact raw reference projection bound by the joint native import receipt. */
export interface ArchiveAssetVersion {
  index: number;
  id: string;
  url: string;
  vault_path: string;
  size: number;
}
export interface ReferenceChange {
  collection: "documents" | "drafts" | "templates";
  index: number;
  field: "content" | "cover_image";
  /** JavaScript UTF-16 code units, NOT UTF-8 bytes. */
  offset_unit: "utf16_code_units";
  start: number;
  end: number;
  original: string;
  replacement: string;
}
type Row = Record<string, unknown>;
type Span = { start: number; end: number; original: string; replacement: string };
const object = (value: unknown): value is Row => !!value && typeof value === "object" && !Array.isArray(value);
function fail(): never { throw new Error("Archive asset references are ambiguous or unsupported; retain the original import"); }

function referenceSpans(source: string, aliases: ReadonlyMap<string, string>, portableFences = true): Span[] {
  const spans: Span[] = [];
  // markdown-it normalizes CRLF before parsing. Keep original UTF-16 code-unit position
  // ownership instead of treating normalized offsets as raw source offsets.
  const normalizedOrigins = (raw: string, base = 0) => {
    const origins: number[] = [];
    for (let position = 0; position < raw.length; position++) {
      origins.push(base + position);
      if (raw[position] === "\r" && raw[position + 1] === "\n") position++;
    }
    origins.push(base + raw.length);
    return origins;
  };
  const blockOrigins = normalizedOrigins(source);
  const markdown = new MarkdownIt({ html: false, linkify: false });
  type DefinitionCapture = { source: string; origins: number[]; span?: Span };
  const referenceRules = markdown.block.ruler.getRules("");
  if (referenceRules.filter((rule) => rule === installedReferenceRule).length !== 1) fail(); // Module identity survives minification; no function-name lookup.
  const captures: (DefinitionCapture | undefined)[] = [];
  const destinationParser = markdown.helpers.parseLinkDestination;
  // Instance-local helper facade: never mutate markdown-it's shared helpers.
  Object.defineProperty(markdown, "helpers", { value: {
    ...markdown.helpers,
    parseLinkDestination: (text: string, start: number, maximum: number) => {
      const parsed = destinationParser(text, start, maximum);
      const capture = captures[captures.length - 1];
      const replacement = parsed.ok ? aliases.get(parsed.str) : undefined;
      if (capture && replacement !== undefined) {
        if (!capture.source.startsWith(text)) fail();
        const angle = text[start] === "<";
        const first = start + (angle ? 1 : 0);
        const last = parsed.pos - (angle ? 1 : 0);
        const origin = capture.origins[first];
        if (origin === undefined || last <= first || capture.origins.slice(first, last).some((value, index) => value !== origin + index)) fail();
        capture.span = { start: origin, end: origin + last - first, original: text.slice(first, last), replacement };
      }
      return parsed;
    },
  } });
  markdown.block.ruler.at("reference", (state, startLine, endLine, silent) => {
    const existing = new Set(Object.keys(state.env.references ?? {}));
    const capture: DefinitionCapture | undefined = silent ? undefined : { source: "", origins: [] };
    if (capture) {
      // Actual container-adjusted StateBlock coordinates, before quote/list restoration.
      for (let line = startLine; line < state.lineMax; line++) {
        const start = state.bMarks[line] + state.tShift[line];
        const end = Math.min(state.src.length, state.eMarks[line] + 1);
        capture.source += state.src.slice(start, end);
        for (let position = start; position < end; position++) capture.origins.push(blockOrigins[position]);
      }
    }
    captures.push(capture);
    let accepted: boolean;
    try { accepted = installedReferenceRule(state, startLine, endLine, silent); }
    finally { captures.pop(); }
    // Only the first actually accepted definition owns the reference. Duplicate
    // definitions cannot borrow that entry's href/label proof.
    if (accepted && capture?.span && Object.keys(state.env.references ?? {}).some((label) => !existing.has(label))) spans.push(capture.span);
    return accepted;
  });
  const verifier = new MarkdownIt({ html: false, linkify: false });
  const inline = new MarkdownIt({ html: false, linkify: false });
  let inlineOrigins: number[] = [];
  if (inline.inline.ruler.getRules("").filter(rule => rule === installedAutolinkRule).length !== 1) fail();
  inline.inline.ruler.at("autolink", (state, silent) => {
    const start = state.pos;
    const before = state.tokens.length;
    const accepted = installedAutolinkRule(state, silent);
    if (!accepted || silent) return accepted;
    const endpoint = state.tokens.slice(before).find(token => token.type === "link_open" && token.markup === "autolink")?.attrGet("href");
    const original = state.src.slice(start + 1, state.pos - 1);
    const replacement = aliases.get(original);
    if (replacement !== undefined) {
      if (state.src[start] !== "<" || state.src[state.pos - 1] !== ">" || endpoint !== inline.normalizeLink(original)) fail();
      const first = inlineOrigins[start + 1], last = inlineOrigins[state.pos - 1];
      if (first === undefined || last === undefined || source.slice(first, last) !== original) fail();
      spans.push({ start: first, end: last, original, replacement });
    }
    return accepted;
  });
  inline.inline.ruler.before("link", "archive_reference_span", (state, silent) => {
    if (silent) return false;
    const image = state.src[state.pos] === "!" && state.src[state.pos + 1] === "[";
    const bracket = image ? state.pos + 1 : state.pos;
    if (state.src[bracket] !== "[") return false;
    const labelEnd = inline.helpers.parseLinkLabel(state, bracket, !image);
    if (labelEnd < 0 || state.src[labelEnd + 1] !== "(") return false;
    let start = labelEnd + 2;
    while (/\s/.test(state.src[start] ?? "") && start < state.posMax) start++;
    const destination = inline.helpers.parseLinkDestination(state.src, start, state.posMax);
    if (!destination.ok) return false;
    let closing = destination.pos;
    while (/\s/.test(state.src[closing] ?? "") && closing < state.posMax) closing++;
    if (state.src[closing] !== ")") {
      const title = inline.helpers.parseLinkTitle(state.src, closing, state.posMax);
      if (!title.ok) return false;
      closing = title.pos;
      while (/\s/.test(state.src[closing] ?? "") && closing < state.posMax) closing++;
    }
    if (state.src[closing] !== ")") return false;
    const fragment = state.src.slice(state.pos, closing + 1);
    const tokens = verifier.parseInline(fragment, {})[0]?.children ?? [];
    const href = verifier.normalizeLink(destination.str);
    if (!tokens.some((token) => image ? token.type === "image" && token.attrGet("src") === href : token.type === "link_open" && token.attrGet("href") === href)) return false;
    const replacement = aliases.get(destination.str);
    if (replacement !== undefined) {
      const angle = state.src[start] === "<";
      const end = destination.pos - (angle ? 1 : 0);
      start += angle ? 1 : 0;
      const rawStart = inlineOrigins[start];
      const rawEnd = inlineOrigins[end];
      if (rawStart === undefined || rawEnd === undefined) fail();
      spans.push({ start: rawStart, end: rawEnd, original: source.slice(rawStart, rawEnd), replacement });
    }
    return false; // The actual existing parser still consumes the link/image.
  });
  const lines = source.match(/[^\n]*(?:\n|$)/g)?.filter(Boolean) ?? [];
  const starts = [0];
  for (const line of lines) starts.push(starts[starts.length - 1] + line.length);
  const environment: { references?: Record<string, { href: string }> } = {};
  const tokens = markdown.parse(source, environment);
  const parsed = new Set<string>();
  for (const token of tokens) {
    if (!token.map) continue;
    const [first, last] = token.map;
    if (token.type === "inline") {
      const key = `${first}:${last}`;
      if (parsed.has(key)) continue;
      parsed.add(key);
      const raw = source.slice(starts[first], starts[last]);
      inlineOrigins = normalizedOrigins(raw, starts[first]);
      const rawTokens = inline.parseInline(raw, environment)[0]?.children ?? [];
      const endpointCounts = (children: typeof rawTokens) => {
        const counts = new Map<string, number>();
        for (const child of children) {
          const value = child.type === "image" ? child.attrGet("src") : child.type === "link_open" ? child.attrGet("href") : null;
          if (value && [...aliases.keys()].some((alias) => markdown.normalizeLink(alias) === value)) {
            const endpoint = JSON.stringify([value, child.type, child.markup]);
            counts.set(endpoint, (counts.get(endpoint) ?? 0) + 1);
          }
        }
        return JSON.stringify([...counts].sort(([a], [b]) => a.localeCompare(b)));
      };
      // Raw offsets are used only when original-prefix parsing consumes the SAME
      // relevant destinations as the real container-adjusted inline token stream.
      if (endpointCounts(rawTokens) !== endpointCounts(token.children ?? [])) fail();
    } else if (portableFences && token.type === "fence" && /^(video|gallery|carousel)$/i.test(token.info.trim())) {
      for (let line = first + 1; line < last - 1; line++) {
        const text = lines[line];
        if (/^video$/i.test(token.info.trim())) {
          const match = text.match(/^(\s*(?:src|poster):\s*)(.*?)(\s*(?:\r?\n)?)$/i);
          if (!match) continue;
          const replacement = aliases.get(match[2]);
          if (replacement !== undefined) fail(); // Current PortableBlocks rejects native asset:// sources.
        } else if (/^\s*!\[[^\]]*\]\([^\s)]+(?:\s+"[^"]*")?\)\s*$/.test(text)) {
          const before = spans.length;
          inlineOrigins = normalizedOrigins(text, starts[line]);
          inline.parseInline(text, {});
          if (spans.length !== before) fail(); // No renderer-compatible native portable source yet.
        }
      }
    }
  }
  const unique = [...new Map(spans.map((span) => [`${span.start}:${span.end}`, span])).values()].sort((a, b) => a.start - b.start);
  if (unique.some((span) => source.slice(span.start, span.end) !== span.original)) fail();
  if (unique.some((span, index) => index > 0 && span.start < unique[index - 1].end)) fail();
  return unique;
}

/** Rewrite only destinations proven by the existing raw-span Markdown parser.
 * Ambiguity is refused only when an actual accepted reference uses that alias.
 * Plain text, code, titles and duplicate rejected definitions retain exact bytes.
 */
export function rewriteExactAssetReferences(
  source: string,
  aliases: ReadonlyMap<string, string>,
  ambiguous: ReadonlySet<string> = new Set(),
): string {
  if (ambiguous.size > 0 && referenceSpans(source, new Map([...ambiguous].map(alias => [alias, alias])), false).length > 0) {
    throw new Error("Export asset reference is ambiguous; choose an exact immutable asset ID");
  }
  // Static Markdown export renders these fences as code. Native archive callers
  // keep the default portable renderer refusal above, unchanged.
  const spans = referenceSpans(source, aliases, false);
  let rewritten = source;
  for (const span of [...spans].reverse()) {
    rewritten = rewritten.slice(0, span.start) + span.replacement + rewritten.slice(span.end);
  }
  return rewritten;
}

export function prepareArchiveAssetReferences<T extends object>(snapshot: T, versions: readonly ArchiveAssetVersion[]): { snapshot: T; changes: ReferenceChange[] } {
  const input: unknown = snapshot;
  if (!object(input)) fail();
  const checked: Row = input;
  if (checked.assets === undefined && versions.length === 0) return { snapshot: { ...snapshot }, changes: [] };
  if (!Array.isArray(checked.assets)) fail();
  const originalAssets: unknown[] = checked.assets;
  const aliases = new Map<string, string>();
  const seen = new Set<number>();
  const assets = [...originalAssets];
  const bind = (old: unknown, next: string) => {
    if (typeof old !== "string" || !old) return;
    if (aliases.has(old) && aliases.get(old) !== next) fail();
    aliases.set(old, next);
  };
  for (const version of versions) {
    const asset = assets[version.index];
    if (!Number.isInteger(version.index) || seen.has(version.index) || !object(asset) || asset.id !== version.id || asset.source !== "tauri" || !version.url.startsWith("asset://localhost/") || !version.vault_path || !Number.isSafeInteger(version.size) || version.size < 0) fail();
    seen.add(version.index);
    bind(asset.url, version.url);
    bind(asset.vault_path, version.url);
    assets[version.index] = { ...asset, url: version.url, vault_path: version.vault_path, size: version.size };
  }
  // Alias collisions with a different, uninstalled asset are equally ambiguous.
  originalAssets.forEach((asset, index) => {
    if (!object(asset) || seen.has(index)) return;
    if ([asset.url, asset.vault_path].some((alias) => typeof alias === "string" && aliases.has(alias))) fail();
  });
  const result: Row = { ...checked, assets };
  const changes: ReferenceChange[] = [];
  for (const collection of ["documents", "drafts", "templates"] as const) {
    const rows = checked[collection];
    if (rows === undefined) continue;
    if (!Array.isArray(rows)) fail();
    result[collection] = rows.map((row, index) => {
      if (!object(row)) fail();
      const next = { ...row };
      if (typeof row.content === "string") {
        const spans = referenceSpans(row.content, aliases);
        let content = row.content;
        for (const span of [...spans].reverse()) content = content.slice(0, span.start) + span.replacement + content.slice(span.end);
        next.content = content;
        changes.push(...spans.map((span) => ({ collection, index, field: "content" as const, offset_unit: "utf16_code_units" as const, ...span })));
      }
      if (typeof row.cover_image === "string" && aliases.has(row.cover_image)) {
        const replacement = aliases.get(row.cover_image)!;
        next.cover_image = replacement;
        changes.push({ collection, index, field: "cover_image", offset_unit: "utf16_code_units", start: 0, end: row.cover_image.length, original: row.cover_image, replacement });
      }
      return next;
    });
  }
  return { snapshot: result as unknown as T, changes };
}

export interface NativeArchiveBinding { provider: string; database_path: string; database_instance_id: string }
export interface NativeArchivePlan {
  schema_version: 1;
  operation_id: string;
  plan_digest: string;
  original_snapshot_json: string;
  versions: (ArchiveAssetVersion & { sha256: string })[];
  binding: NativeArchiveBinding;
  expected_revision: string | null;
  destination_snapshot: LocalVaultSnapshot | null;
}
export interface NativeArchiveReceipt {
  schema_version: 1;
  canonical_committed: true;
  operation_id: string;
  request_fingerprint: string;
  binding: NativeArchiveBinding;
  canonical_revision: string;
  candidate: LocalVaultSnapshot;
  reference_changes: ReferenceChange[];
  versions: NativeArchivePlan["versions"];
  current_snapshot?: LocalVaultSnapshot | null;
}
export interface NativeArchiveInventoryRow { operation_id: string; request_fingerprint: string; canonical_committed: boolean; retained_abandoned: boolean; intent: unknown }
export interface NativeArchiveInventory { binding: NativeArchiveBinding | null; operations: NativeArchiveInventoryRow[]; next_cursor: string | null }
export interface NativeArchiveInspection {
  binding: NativeArchiveBinding;
  operation_id: string;
  request_fingerprint: string;
  intent: unknown;
  receipt: NativeArchiveReceipt | null;
  resolution: { retained_abandoned: true; evidence_retained: true } | null;
  current_snapshot: LocalVaultSnapshot | null;
}
const hash = (value: unknown): value is string => typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
function bindingValid(value: unknown): value is NativeArchiveBinding {
  return object(value) && typeof value.provider === "string" && typeof value.database_path === "string" && !!value.database_path && hash(value.database_instance_id);
}
export function assertNativeArchivePlan(value: unknown, prepared: { handle: string; snapshot_json: string }, provider: string): asserts value is NativeArchivePlan {
  if (!object(value) || value.schema_version !== 1 || value.operation_id !== prepared.handle || value.original_snapshot_json !== prepared.snapshot_json || !hash(value.plan_digest) || !bindingValid(value.binding) || value.binding.provider !== provider || !Array.isArray(value.versions) || !(value.expected_revision === null || typeof value.expected_revision === "string") || !(value.destination_snapshot === null || object(value.destination_snapshot))) fail();
  const original: unknown = JSON.parse(prepared.snapshot_json);
  if (!object(original)) fail();
  const assets = original.assets === undefined ? [] : original.assets;
  if (!Array.isArray(assets)) fail();
  const expected = assets.map((asset, index) => object(asset) && asset.source === "tauri" ? index : -1).filter(index => index >= 0);
  const seen = new Set<number>();
  for (const version of value.versions) {
    if (!object(version) || typeof version.index !== "number" || !Number.isInteger(version.index) || seen.has(version.index) || !expected.includes(version.index)) fail();
    const asset: unknown = assets[version.index];
    if (!object(asset) || asset.id !== version.id || !hash(version.sha256) || typeof version.size !== "number" || !Number.isSafeInteger(version.size) || version.size < 0 || typeof version.vault_path !== "string" || !version.vault_path || typeof version.url !== "string" || !version.url.startsWith("asset://localhost/")) fail();
    seen.add(version.index);
  }
  if (expected.length !== seen.size) fail();
}
export function assertNativeArchiveInventory(value: unknown, expected?: NativeArchiveBinding | null): asserts value is NativeArchiveInventory {
  if (!object(value) || !(value.binding === null || bindingValid(value.binding)) || (expected !== undefined && !nativeJsonValueEquals(value.binding, expected)) || !Array.isArray(value.operations) || !(value.next_cursor === null || typeof value.next_cursor === "string")) fail();
  const ids = new Set<string>();
  for (const row of value.operations) {
    if (!object(row) || typeof row.operation_id !== "string" || !row.operation_id || ids.has(row.operation_id) || !hash(row.request_fingerprint) || typeof row.canonical_committed !== "boolean" || typeof row.retained_abandoned !== "boolean" || !object(row.intent)) fail();
    ids.add(row.operation_id);
  }
  if (value.next_cursor !== null && !ids.has(value.next_cursor)) fail();
}
export function assertNativeArchiveReceipt(value: unknown, plan: NativeArchivePlan, request: { candidate: unknown; reference_changes: unknown }): asserts value is NativeArchiveReceipt {
  if (!object(value) || value.schema_version !== 1 || value.canonical_committed !== true || value.operation_id !== plan.operation_id || !hash(value.request_fingerprint) || !nativeJsonValueEquals(value.binding, plan.binding) || typeof value.canonical_revision !== "string" || !value.canonical_revision || !nativeJsonValueEquals(value.candidate, request.candidate) || !nativeJsonValueEquals(value.reference_changes, request.reference_changes) || !nativeJsonValueEquals(value.versions, plan.versions)) fail();
}
