import { useEffect, useRef, useState } from "react";
import {
  dataClient,
  type DocumentRecord,
  type DocumentPropertyValueRecord,
  type TagRecord,
  type TemplateInput,
} from "@/lib/dataClient";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  DialogShell,
  DialogHeader,
  DialogTitle,
  DialogCloseAction,
  DialogBody,
  DialogFooter,
} from "@/components/ui/dialog-shell";
import { MarkdownPreview } from "@/components/editor/MarkdownPreview";
import { withTemplatePresentation } from "@/lib/templates";
import { stripCustomCss } from "@/lib/documentContent";

function comparableJson(value: unknown): string {
  return JSON.stringify(value, (_key, item) =>
    item && typeof item === "object" && !Array.isArray(item)
      ? Object.fromEntries(
          Object.keys(item)
            .sort()
            .map((key) => [key, item[key]]),
        )
      : item,
  );
}

export function hasDocumentScopedTemplateAssets(content: string): boolean {
  // External portable links can have an /assets/ or /attachments/ path too.
  // Keep explicit Starlane file routes; exclude unrelated absolute web URLs.
  const references = content.replace(/https?:\/\/[^\s<>"')]+/gi, (url) =>
    /\/api\/v1\/files\/spaces\//i.test(url) ? url : "",
  );
  return /(?:starlane-attachment:|starlane-local-asset:|asset:\/\/|blob:|\/attachments\/|\/assets\/)/i.test(references);
}

/** Copy stored CSS without applying the rendering sanitizer to its recovery source. */
export function readOriginalTemplateCss(source: string): string | undefined {
  const markers = [...source.matchAll(/<!--\s*(\/?)starlane:custom-css\s*-->/gi)];
  const possibleMarkers = [...source.matchAll(/<!--\s*\/?starlane:custom-css\b/gi)];
  if (markers.length === 0 && possibleMarkers.length === 0) return undefined;
  if (markers.length !== 2 || possibleMarkers.length !== 2 || markers[0][1] || !markers[1][1]) {
    throw new Error("This page has ambiguous CSS markers. Preserve its original source and repair a separate copy before including presentation defaults in a template.");
  }
  return source.slice(markers[0].index! + markers[0][0].length, markers[1].index!);
}

export function buildPageTemplateSnapshot(
  document: DocumentRecord,
  source: string,
  tags: TagRecord[],
  properties: DocumentPropertyValueRecord[],
  options: {
    name: string;
    description: string;
    tags: boolean;
    properties: boolean;
    presentation: boolean;
  },
): TemplateInput {
  if (
    hasDocumentScopedTemplateAssets(source) ||
    (options.presentation && hasDocumentScopedTemplateAssets(document.cover_image ?? ""))
  ) {
    throw new Error(
      "This page contains document-owned assets. Remove those references in a separate template first; asset bytes cannot be copied into a template yet.",
    );
  }
  const originalCss = options.presentation ? readOriginalTemplateCss(source) : undefined;
  const match = source.match(/^---\n([\s\S]*?)\n---\n?/);
  const body = stripCustomCss(match ? source.slice(match[0].length) : source);
  const width = match?.[1].match(/^page_width:\s*["']?(wide|full)["']?\s*$/m)?.[1];
  const defaults = options.properties
    ? Object.fromEntries(
        properties
          .filter((item) => item.value !== null && !item.property_name.startsWith("__starlane_"))
          .map((item) => [item.property_name, item.value]),
      )
    : undefined;
  return {
    name: options.name.trim(),
    description: options.description.trim(),
    content: body,
    tags: options.tags ? tags.map((tag) => tag.name) : [],
    properties: withTemplatePresentation(
      originalCss !== undefined
        ? { ...defaults, __starlane_presentation: { custom_css: originalCss } }
        : defaults,
      options.presentation
        ? {
            icon: document.icon,
            cover_image: document.cover_image,
            custom_css: originalCss,
            page_width: width === "wide" || width === "full" ? width : undefined,
          }
        : {},
    ),
  };
}

export async function loadPublishedTemplateSnapshot(
  spaceId: string,
  documentId: string,
  client: Pick<typeof dataClient, "getDocument" | "getDocumentContent" | "listDocumentTags" | "listDocumentProperties"> = dataClient,
) {
  const document = await client.getDocument(spaceId, documentId);
  if (!document.published_at) throw new Error("Publish this page before saving it as a template. Private draft content is not copied.");
  const [content, tags, properties] = await Promise.all([
    client.getDocumentContent(spaceId, documentId),
    client.listDocumentTags(spaceId, documentId),
    client.listDocumentProperties(spaceId, documentId),
  ]);
  const confirmed = await client.getDocument(spaceId, documentId);
  if (document.published_at !== confirmed.published_at) throw new Error("The published page changed while loading. Reload its snapshot before saving a template.");
  return { document, content: content.content, tags, properties };
}

export function SaveAsTemplateDialog({
  spaceId,
  documentId,
  spaceName,
  onClose,
  onSaved,
}: {
  spaceId: string;
  documentId: string;
  spaceName: string;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  const [snapshot, setSnapshot] = useState<{
    document: DocumentRecord;
    content: string;
    tags: TagRecord[];
    properties: DocumentPropertyValueRecord[];
  } | null>(null);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [includeTags, setIncludeTags] = useState(false);
  const [includeProperties, setIncludeProperties] = useState(false);
  const [includePresentation, setIncludePresentation] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [reloadVersion, setReloadVersion] = useState(0);
  const operationId = useRef(globalThis.crypto?.randomUUID?.() ?? `template-${Date.now()}-${Math.random().toString(36).slice(2)}`);
  const submitted = useRef<TemplateInput | null>(null);
  useEffect(() => {
    let cancelled = false;
    loadPublishedTemplateSnapshot(spaceId, documentId)
      .then((loaded) => {
        if (cancelled) return;
        setSnapshot(loaded);
        setName(loaded.document.title);
      })
      .catch((cause) => {
        if (!cancelled)
          setError(cause instanceof Error ? cause.message : "Unable to load the published page.");
      });
    return () => {
      cancelled = true;
    };
  }, [spaceId, documentId, reloadVersion]);
  const save = async () => {
    if (!snapshot || busy || !name.trim()) return;
    setBusy(true);
    setError("");
    try {
      // Freeze the attempted operation. An ambiguous failure reconciles the same ID,
      // rather than creating a second template on retry.
      const input =
        submitted.current ??
        buildPageTemplateSnapshot(
          snapshot.document,
          snapshot.content,
          snapshot.tags,
          snapshot.properties,
          {
            name,
            description,
            tags: includeTags,
            properties: includeProperties,
            presentation: includePresentation,
          },
        );
      submitted.current = input;
      let id: string;
      try {
        id = (
          await dataClient.createTemplate(spaceId, { ...input, source_id: operationId.current })
        ).id;
      } catch (cause) {
        const existing = await dataClient
          .getTemplate(spaceId, operationId.current)
          .catch(() => null);
        if (
          !existing ||
          existing.name !== input.name ||
          existing.content !== input.content ||
          comparableJson(existing.properties ?? null) !== comparableJson(input.properties ?? null)
        )
          throw cause;
        id = existing.id;
      }
      onSaved(id);
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "Unable to save this template. Retry retains the same operation.",
      );
    } finally {
      setBusy(false);
    }
  };
  return (
    <DialogShell
      labelledBy="save-page-template-title"
      maxWidth="xl"
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <DialogHeader>
        <div>
          <DialogTitle id="save-page-template-title">Save as template</DialogTitle>
          <p className="text-text-muted mt-1 text-sm">
            Destination: {spaceName}. Your original page stays unchanged.
          </p>
        </div>
        <DialogCloseAction label="Close save as template" disabled={busy} onClick={onClose} />
      </DialogHeader>
      <DialogBody className="min-h-0 space-y-4 overflow-y-auto">
        {error && (
          <p role="alert" className="text-danger text-sm">
            {error}
          </p>
        )}
        {!snapshot && error && (
          <Button type="button" variant="secondary" onClick={() => { setError(""); setReloadVersion((version) => version + 1); }}>Reload published snapshot</Button>
        )}
        {!snapshot && !error && (
          <p role="status" className="text-text-muted text-sm">
            Loading published revision…
          </p>
        )}
        {snapshot && (
          <>
            <p className="text-text-muted text-sm">
              Published snapshot at {snapshot.document.published_at}. Unsaved edits and
              private drafts are excluded. Tags and property defaults reflect the page’s current
              metadata.
            </p>
            <label className="block text-sm">
              Template name
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                disabled={busy || Boolean(submitted.current)}
                maxLength={100}
              />
            </label>
            <label className="block text-sm">
              Description
              <Textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                disabled={busy || Boolean(submitted.current)}
                rows={2}
              />
            </label>
            <fieldset disabled={busy || Boolean(submitted.current)} className="space-y-2 text-sm">
              <legend className="mb-2 font-medium">Include defaults</legend>
              {[
                { label: "Tags", checked: includeTags, set: setIncludeTags },
                { label: "Properties", checked: includeProperties, set: setIncludeProperties },
                {
                  label: "Presentation (icon, cover, width, CSS)",
                  checked: includePresentation,
                  set: setIncludePresentation,
                },
              ].map((option) => (
                <label key={option.label} className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    className="accent-primary"
                    checked={option.checked}
                    onChange={(e) => option.set(e.target.checked)}
                  />
                  {option.label}
                </label>
              ))}
            </fieldset>
            <p className="text-text-muted text-xs">
              Permissions, comments, revision history and collaboration state are excluded.
              Document-owned attachments are unsupported; cross-space copying is unavailable.
            </p>
            <details>
              <summary className="cursor-pointer text-sm">Published content preview</summary>
              <MarkdownPreview
                content={stripCustomCss(snapshot.content.replace(/^---\n[\s\S]*?\n---\n?/, ""))}
                assetContext={{ space_id: spaceId, document_id: documentId }}
              />
            </details>
          </>
        )}
      </DialogBody>
      <DialogFooter>
        <Button variant="secondary" onClick={onClose} disabled={busy}>
          Cancel
        </Button>
        <Button onClick={() => void save()} disabled={busy || !snapshot || !name.trim()}>
          {busy ? "Saving…" : submitted.current ? "Retry save" : "Save template"}
        </Button>
      </DialogFooter>
    </DialogShell>
  );
}
