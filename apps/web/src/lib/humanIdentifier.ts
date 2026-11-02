/** Readable reference metadata; it never replaces a permanent document ID. */
export function normalizeHumanIdentifier(value: unknown): string | null {
  if (value === null) return null;
  if (typeof value !== "string") throw new Error("Page identifier must be a string or null");
  const normalized = value.trim();
  if (!normalized) return null;
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(normalized))
    throw new Error(
      "Page identifier must contain 1–64 ASCII letters, digits, dots, underscores or hyphens and start with a letter or digit",
    );
  return normalized;
}

export function humanIdentifierKey(value: unknown): string | null {
  return normalizeHumanIdentifier(value)?.toUpperCase() ?? null;
}

type IdentifierDocument = { id: string; space_id: string; human_identifier?: unknown };

/** Validate all lifecycle states before any canonical snapshot replacement. */
export function normalizeDocumentIdentifiers<T extends IdentifierDocument>(
  documents: readonly T[],
): T[] {
  const assigned = new Map<string, Set<string>>();
  return documents.map((document) => {
    if (
      !Object.prototype.hasOwnProperty.call(document, "human_identifier") ||
      document.human_identifier === undefined
    )
      return document;
    const display = normalizeHumanIdentifier(document.human_identifier);
    const key = display?.toUpperCase();
    if (key) {
      const space = assigned.get(document.space_id) ?? new Set<string>();
      if (space.has(key))
        throw new Error(
          "Page identifier is already reserved in this space, including archived or trashed pages",
        );
      space.add(key);
      assigned.set(document.space_id, space);
    }
    return { ...document, human_identifier: display };
  });
}

export function validateDocumentIdentifiers(documents: readonly IdentifierDocument[]): void {
  normalizeDocumentIdentifiers(documents);
}

export interface DocumentIdentifierSource {
  page_updated_at: string;
  published_at: string | null;
  human_identifier: string | null;
}

export interface DocumentIdentifierIntent {
  id: string;
  space_id: string;
  human_identifier: string | null;
  source: DocumentIdentifierSource;
}

export function parseDocumentIdentifierIntent(value: unknown): DocumentIdentifierIntent {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("Identifier operation must be an object");
  const payload = value as Record<string, unknown>;
  if (
    Object.keys(payload).some(
      (key) => !["id", "space_id", "human_identifier", "source"].includes(key),
    )
  )
    throw new Error("Identifier operation contains unrelated fields");
  if (
    typeof payload.id !== "string" ||
    !payload.id ||
    typeof payload.space_id !== "string" ||
    !payload.space_id
  )
    throw new Error("Identifier operation requires exact page and space IDs");
  if (!payload.source || typeof payload.source !== "object" || Array.isArray(payload.source))
    throw new Error("Identifier operation requires its original page source");
  const source = payload.source as Record<string, unknown>;
  if (
    Object.keys(source).some(
      (key) => !["page_updated_at", "published_at", "human_identifier"].includes(key),
    )
  )
    throw new Error("Identifier source contains unrelated fields");
  if (
    typeof source.page_updated_at !== "string" ||
    !Number.isFinite(Date.parse(source.page_updated_at)) ||
    !(
      source.published_at === null ||
      (typeof source.published_at === "string" && Number.isFinite(Date.parse(source.published_at)))
    )
  )
    throw new Error("Identifier operation requires valid explicit source timestamps");
  const display = normalizeHumanIdentifier(payload.human_identifier);
  const original = normalizeHumanIdentifier(source.human_identifier);
  if (source.human_identifier !== original)
    throw new Error("Identifier source must retain its exact normalized display");
  return {
    id: payload.id,
    space_id: payload.space_id,
    human_identifier: display,
    source: {
      page_updated_at: source.page_updated_at,
      published_at: source.published_at,
      human_identifier: original,
    },
  };
}
