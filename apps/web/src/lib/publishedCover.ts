import type { PublishedDocumentMetadata } from "@starlane-notes/shared-types";
import { portableAttachmentReferences } from "./portableAttachmentReferences";
/** Covers are scalar references, never inferred from a filename or latest family. */
export function coverAttachmentReferences(cover: string | null | undefined, serverBase: string): Map<string, string> {
  if (!cover) return new Map();
  if (typeof cover !== "string" || cover.length > 65536) throw new Error("Cover reference exceeds bound");
  if (cover.startsWith("starlane-attachment://")) {
    const id = cover.slice("starlane-attachment://".length);
    if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id)) throw new Error("Cover requires an exact immutable version");
    return new Map([[cover,id]]);
  }
  if (cover.includes("/api/v1/files/")) {
    const references = portableAttachmentReferences(`![cover](${cover})`,serverBase);
    if (references.size !== 1 || !references.has(cover)) throw new Error("Cover requires exact supported attachment ownership");
    return references;
  }
  return new Map(); // External/local image URLs stay presentation, not server byte authority.
}
export function selectedPublishedMetadata(value: unknown, title: string): PublishedDocumentMetadata {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Archive selected published metadata required");
  const v=value as Record<string, unknown>;
  if (Object.keys(v).length !== 3 || v.title !== title || typeof v.title !== "string" ||
      !(v.icon === null || typeof v.icon === "string") || !(v.cover_image === null || typeof v.cover_image === "string"))
    throw new Error("Archive selected published metadata differs");
  if ([v.title,v.icon,v.cover_image].some(x=>typeof x === "string" && new TextEncoder().encode(x).length>65536)) throw new Error("Published metadata exceeds bound");
  return {title:v.title,icon:v.icon as string|null,cover_image:v.cover_image as string|null};
}
