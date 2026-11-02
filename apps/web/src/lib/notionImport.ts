import { buildPageCatalog, createPageTraversalBudget } from "./pageHierarchy";
import { createFolderTraversalBudget } from "./folderHierarchy";
import { unzip } from "fflate";
import type { DraftSaveSelection } from "./dataClient";

export const NOTION_IMPORT_LIMITS = {
  archiveBytes: 256 * 1024 * 1024,
  entryCount: 10_000,
  textEntryBytes: 50 * 1024 * 1024,
  textTotalBytes: 200 * 1024 * 1024,
  expandedBytes: 1024 * 1024 * 1024,
} as const;

export type NotionImportEntryKind = "page" | "database" | "asset" | "ignored" | "unsupported";

export interface NotionImportEntry {
  path: string;
  kind: NotionImportEntryKind;
  compressedBytes: number;
  uncompressedBytes: number;
}

export interface NotionImportPreflight {
  status: "ready" | "blocked";
  archiveName: string;
  rootFolder?: string;
  pageCount: number;
  databaseCount: number;
  assetCount: number;
  ignoredCount: number;
  unsupportedCount: number;
  emptyTextCount: number;
  totalExpandedBytes: number;
  blockers: string[];
  warnings: string[];
  entries: NotionImportEntry[];
}

export interface NotionImportFolderPlan {
  key: string;
  name: string;
  parentKey?: string;
  depth: number;
}

export interface NotionImportDocumentPlan {
  key: string;
  sourcePath: string;
  title: string;
  content: string;
  kind: "page" | "database";
  folderKey?: string;
  parentKey?: string;
  depth: number;
}

export interface NotionImportPlan {
  archiveName: string;
  folders: NotionImportFolderPlan[];
  documents: NotionImportDocumentPlan[];
  skippedPaths: string[];
  warnings: string[];
}

export interface NotionImportExecutionProgress {
  completed: number;
  total: number;
  label: string;
}

export interface NotionImportExecutionFailure {
  kind: "folder" | "document";
  key: string;
  label: string;
  reason: string;
}

export interface NotionImportExecutionResult {
  status: "completed" | "partial";
  createdFolderCount: number;
  reusedFolderCount: number;
  createdDocumentCount: number;
  renamedDocumentCount: number;
  documentIdsBySourcePath: Record<string, string>;
  documentDraftIdsBySourcePath: Record<string, string>;
  documentDraftSourcesBySourcePath?: Record<string, string>;
  draftSaveSelection?: DraftSaveSelection;
  failures: NotionImportExecutionFailure[];
}

export interface NotionImportFinalizeResult {
  updatedDocumentCount: number;
  rewrittenPageLinkCount: number;
  uploadedAssetCount: number;
  unresolvedPageLinkCount: number;
  skippedAssetCount: number;
  failures: NotionImportExecutionFailure[];
}

export interface NotionImportTargetClient {
  readonly requiresDraftSourceCas?: boolean;
  captureDraftSaveSelection?: () => DraftSaveSelection;
  verifyDraftSaveSelection?: (selection: DraftSaveSelection) => void;
  listFolders(spaceId: string): Promise<Array<{ id: string; name: string; parent_id?: string }>>;
  listDocuments(
    spaceId: string,
  ): Promise<Array<{ id: string; title: string; folder_id?: string; parent_id?: string }>>;
  createFolder(
    spaceId: string,
    name: string,
    options?: { parent_id?: string | null },
  ): Promise<{ id: string; name: string; parent_id?: string }>;
  createDocument(
    spaceId: string,
    input: { title: string; content: string; folder_id?: string | null; parent_id?: string | null },
  ): Promise<{ id: string; draft_id?: string; draft_source_digest?: string }>;
  saveDocumentDataAsset(input: {
    name: string;
    mime_type: string;
    data_url: string;
    space_id: string;
    document_id: string;
  }): Promise<{ id: string; url: string; filename?: string }>;
  updateDocumentDraft(
    spaceId: string,
    documentId: string,
    draftId: string,
    patch: { content: string },
    sourceDigest?: string,
  ): Promise<unknown>;
  updateDocumentContent(spaceId: string, documentId: string, content: string): Promise<void>;
}

interface ZipDirectoryEntry {
  path: string;
  compressedBytes: number;
  uncompressedBytes: number;
  compression: number;
  flags: number;
  directory: boolean;
  symlink: boolean;
}

const ZIP_EOCD_SIGNATURE = 0x06054b50;
const ZIP_CENTRAL_SIGNATURE = 0x02014b50;
const ZIP64_SENTINEL = 0xffffffff;
const ZIP_EOCD_BYTES = 22;
const ZIP_MAX_COMMENT_BYTES = 65_535;

/**
 * Inspects a Notion Markdown & CSV export before any Starlane mutation occurs.
 * The central directory is bounded first so an oversized or unsafe archive is
 * rejected before supported text entries are decompressed for UTF-8 validation.
 */
export async function preflightNotionExportArchive(
  archiveName: string,
  archiveBytes: Uint8Array,
): Promise<NotionImportPreflight> {
  const normalizedArchiveName = archiveName.trim() || "Notion export.zip";
  if (!normalizedArchiveName.toLowerCase().endsWith(".zip")) {
    throw new Error("Choose the .zip file produced by Notion's Markdown & CSV export.");
  }
  if (archiveBytes.byteLength === 0) throw new Error("The selected ZIP is empty.");
  if (archiveBytes.byteLength > NOTION_IMPORT_LIMITS.archiveBytes) {
    throw new Error("The selected ZIP exceeds Starlane's 256 MB browser import limit.");
  }

  const directoryEntries = readZipCentralDirectory(archiveBytes);
  const blockers: string[] = [];
  const warnings: string[] = [];
  const seenPaths = new Set<string>();
  const entries: NotionImportEntry[] = [];
  let totalExpandedBytes = 0;
  let textExpandedBytes = 0;

  if (directoryEntries.length > NOTION_IMPORT_LIMITS.entryCount) {
    blockers.push(
      `Archive contains ${directoryEntries.length.toLocaleString()} entries; the safe limit is ${NOTION_IMPORT_LIMITS.entryCount.toLocaleString()}.`,
    );
  }

  for (const entry of directoryEntries) {
    if (entry.directory) continue;
    totalExpandedBytes += entry.uncompressedBytes;
    if (entry.uncompressedBytes === ZIP64_SENTINEL || entry.compressedBytes === ZIP64_SENTINEL) {
      blockers.push("ZIP64 archives are not supported by the browser importer.");
      continue;
    }
    if ((entry.flags & 0x1) !== 0)
      blockers.push(`${entry.path}: encrypted ZIP entries are not supported.`);
    if (entry.symlink) blockers.push(`${entry.path}: symbolic links are not allowed in imports.`);
    if (entry.compression !== 0 && entry.compression !== 8) {
      blockers.push(`${entry.path}: unsupported ZIP compression method ${entry.compression}.`);
    }
    if (!isSafeArchivePath(entry.path)) {
      blockers.push(`${entry.path || "Unnamed entry"}: unsafe archive path.`);
      continue;
    }
    if (seenPaths.has(entry.path)) {
      blockers.push(`${entry.path}: duplicate archive path.`);
      continue;
    }
    seenPaths.add(entry.path);

    const kind = classifyNotionEntry(entry.path);
    if (kind === "page" || kind === "database") {
      textExpandedBytes += entry.uncompressedBytes;
      if (entry.uncompressedBytes > NOTION_IMPORT_LIMITS.textEntryBytes) {
        blockers.push(`${entry.path}: text entry exceeds the 50 MB limit.`);
      }
    }
    entries.push({
      path: entry.path,
      kind,
      compressedBytes: entry.compressedBytes,
      uncompressedBytes: entry.uncompressedBytes,
    });
  }

  if (totalExpandedBytes > NOTION_IMPORT_LIMITS.expandedBytes) {
    blockers.push("Expanded archive content exceeds the 1 GB safety limit.");
  }
  if (textExpandedBytes > NOTION_IMPORT_LIMITS.textTotalBytes) {
    blockers.push("Markdown and CSV content exceeds the 200 MB browser import limit.");
  }

  const pageCount = countKind(entries, "page");
  const databaseCount = countKind(entries, "database");
  const assetCount = countKind(entries, "asset");
  const ignoredCount = countKind(entries, "ignored");
  const unsupportedCount = countKind(entries, "unsupported");
  if (pageCount === 0 && databaseCount === 0) {
    blockers.push("No Markdown pages or CSV databases were found in this export.");
  }
  if (unsupportedCount > 0) {
    warnings.push(
      `${unsupportedCount} unsupported content file${unsupportedCount === 1 ? "" : "s"} will be skipped.`,
    );
  }
  if (ignoredCount > 0) {
    warnings.push(
      `${ignoredCount} hidden or Notion index file${ignoredCount === 1 ? "" : "s"} will be ignored.`,
    );
  }

  const uniqueBlockers = [...new Set(blockers)];
  let emptyTextCount = 0;
  if (uniqueBlockers.length === 0) {
    const extracted = await unzipTextEntries(archiveBytes);
    const decoder = new TextDecoder("utf-8", { fatal: true });
    for (const entry of entries) {
      if (entry.kind !== "page" && entry.kind !== "database") continue;
      const data = extracted[entry.path];
      if (!data) {
        uniqueBlockers.push(`${entry.path}: could not read the exported text.`);
        continue;
      }
      try {
        if (!decoder.decode(data).trim()) emptyTextCount += 1;
      } catch {
        uniqueBlockers.push(`${entry.path}: text is not valid UTF-8.`);
      }
    }
  }
  if (emptyTextCount > 0) {
    warnings.push(
      `${emptyTextCount} empty page or database file${emptyTextCount === 1 ? "" : "s"} will be skipped.`,
    );
  }

  return {
    status: uniqueBlockers.length === 0 ? "ready" : "blocked",
    archiveName: normalizedArchiveName,
    rootFolder: findCommonRoot(entries),
    pageCount,
    databaseCount,
    assetCount,
    ignoredCount,
    unsupportedCount,
    emptyTextCount,
    totalExpandedBytes,
    blockers: uniqueBlockers,
    warnings,
    entries,
  };
}

/** Builds a portable, deterministic creation plan without mutating the target space. */
export async function buildNotionImportPlan(
  archiveName: string,
  archiveBytes: Uint8Array,
  existingPreflight?: NotionImportPreflight,
): Promise<NotionImportPlan> {
  const preflight =
    existingPreflight ?? (await preflightNotionExportArchive(archiveName, archiveBytes));
  if (preflight.status === "blocked") {
    throw new Error(preflight.blockers[0] ?? "The Notion export is not safe to import.");
  }

  const extracted = await unzipTextEntries(archiveBytes);
  const decoder = new TextDecoder("utf-8", { fatal: true });
  const sourceDocuments = preflight.entries
    .filter((entry) => entry.kind === "page" || entry.kind === "database")
    .map((entry) => {
      const relativePath = removeArchiveRoot(entry.path, preflight.rootFolder);
      const raw = extracted[entry.path];
      if (!raw) throw new Error(`${entry.path}: exported text could not be read.`);
      return {
        entry,
        relativePath,
        sourceKey: removeFileExtension(relativePath),
        content: decoder.decode(raw),
      };
    })
    .sort(
      (left, right) =>
        left.sourceKey.split("/").length - right.sourceKey.split("/").length ||
        left.sourceKey.localeCompare(right.sourceKey),
    );
  const sourcePageKeys = new Set(sourceDocuments.map((document) => document.sourceKey));
  const folderPlans = new Map<string, NotionImportFolderPlan>();
  const folderBudget = createFolderTraversalBudget();
  const warnings = [...preflight.warnings];
  const skippedPaths: string[] = [];
  const plannedDocuments: NotionImportDocumentPlan[] = [];
  const plannedByKey = new Map<string, NotionImportDocumentPlan>();
  const pageBudget = createPageTraversalBudget();

  for (const source of sourceDocuments) {
    if (!source.content.trim()) {
      skippedPaths.push(source.entry.path);
      continue;
    }
    const sourceSegments = source.sourceKey.split("/").filter(Boolean);
    const sourceDirectorySegments = sourceSegments.slice(0, -1);
    const pageBackedAncestorKeys = sourceDirectorySegments
      .map((_, index) => sourceDirectorySegments.slice(0, index + 1).join("/"))
      .filter((key) => sourcePageKeys.has(key));
    const plainFolderSegments = sourceDirectorySegments.filter((_, index) => {
      const key = sourceDirectorySegments.slice(0, index + 1).join("/");
      return !sourcePageKeys.has(key);
    });
    let parentFolderKey: string | undefined;
    for (const [index, segment] of plainFolderSegments.entries()) {
      const name = normalizeNotionName(segment, "Imported");
      const key = [...plainFolderSegments.slice(0, index), segment]
        .map((part) => normalizeNotionName(part, "Imported").toLocaleLowerCase())
        .join("/");
      if (!folderPlans.has(key)) {
        folderBudget.visit(key);
        folderPlans.set(key, {
          key,
          name,
          parentKey: parentFolderKey,
          depth: index + 1,
        });
      }
      parentFolderKey = key;
    }

    const kind = source.entry.kind === "database" ? "database" : "page";
    const content = kind === "database" ? csvToMarkdown(source.content) : source.content;
    if (new TextEncoder().encode(content).length > 5 * 1024 * 1024) {
      skippedPaths.push(source.entry.path);
      warnings.push(
        `${source.entry.path}: converted content exceeds Starlane's 5 MB page limit and will be skipped.`,
      );
      continue;
    }
    const candidateParentKey = pageBackedAncestorKeys[pageBackedAncestorKeys.length - 1];
    const candidateParent = candidateParentKey ? plannedByKey.get(candidateParentKey) : undefined;
    const depth = candidateParent ? candidateParent.depth + 1 : 1;
    const sharesFolder = candidateParent?.folderKey === parentFolderKey;
    const parentKey = candidateParent && sharesFolder ? candidateParentKey : undefined;
    if (candidateParentKey && !candidateParent) {
      warnings.push(
        `${source.entry.path}: its exported parent page is empty or unavailable, so this page will be imported at the folder root.`,
      );
    } else if (candidateParentKey && !sharesFolder) {
      warnings.push(
        `${source.entry.path}: an intervening plain folder prevents a child-page relationship, so the page will be imported at that folder root.`,
      );
    }
    pageBudget.visit(source.sourceKey);
    const planned: NotionImportDocumentPlan = {
      key: source.sourceKey,
      sourcePath: source.entry.path,
      title: normalizeNotionName(sourceSegments[sourceSegments.length - 1], "Untitled"),
      content,
      kind,
      folderKey: parentFolderKey,
      parentKey,
      depth: parentKey ? depth : 1,
    };
    plannedDocuments.push(planned);
    plannedByKey.set(planned.key, planned);
  }

  return {
    archiveName: preflight.archiveName,
    folders: [...folderPlans.values()].sort((left, right) => left.depth - right.depth),
    documents: plannedDocuments.sort((left, right) => left.depth - right.depth),
    skippedPaths,
    warnings: [...new Set(warnings)],
  };
}

/** Prove the page creation graph and participating budget before any import mutation. */
export function validateNotionImportPagePlan(documents: NotionImportDocumentPlan[]): void {
  const budget = createPageTraversalBudget();
  const catalog = buildPageCatalog(
    documents.map((row) => ({
      id: row.key,
      space_id: "notion-import",
      folder_id: row.folderKey,
      parent_id: row.parentKey,
    })),
  );
  if (catalog.index.duplicateIds.size || catalog.order.length !== documents.length)
    throw new Error(
      "Notion page plan contains duplicate, unavailable or incompatible parent pages",
    );
  for (const document of documents) budget.visit(document.key);
}

/** Executes a reviewed plan and keeps independent roots moving after a recoverable failure. */
export async function executeNotionImportPlan(
  client: NotionImportTargetClient,
  spaceId: string,
  plan: NotionImportPlan,
  onProgress?: (progress: NotionImportExecutionProgress) => void,
): Promise<NotionImportExecutionResult> {
  validateNotionImportPagePlan(plan.documents);
  const total = plan.folders.length + plan.documents.length;
  let completed = 0;
  let createdFolderCount = 0;
  let reusedFolderCount = 0;
  let createdDocumentCount = 0;
  let renamedDocumentCount = 0;
  const draftSaveSelection = client.captureDraftSaveSelection?.();
  const verifySelection = () => {
    if (draftSaveSelection) client.verifyDraftSaveSelection?.(draftSaveSelection);
  };
  const failures: NotionImportExecutionFailure[] = [];
  const folderIds = new Map<string, string>();
  const documentIds = new Map<string, string>();
  const documentIdsBySourcePath: Record<string, string> = {};
  const documentDraftIdsBySourcePath: Record<string, string> = {};
  const documentDraftSourcesBySourcePath: Record<string, string> = {};
  const existingFolders = await client.listFolders(spaceId);
  verifySelection();
  const existingDocuments = await client.listDocuments(spaceId);
  verifySelection();
  const occupiedDocumentTitles = new Set(
    existingDocuments.map((document) =>
      documentLocationKey(document.folder_id, document.parent_id, document.title),
    ),
  );

  for (const folder of plan.folders) {
    const parentId = folder.parentKey ? folderIds.get(folder.parentKey) : undefined;
    if (folder.parentKey && !parentId) {
      failures.push({
        kind: "folder",
        key: folder.key,
        label: folder.name,
        reason: "Parent folder was not created.",
      });
    } else {
      const existing = existingFolders.find(
        (candidate) =>
          (candidate.parent_id ?? "") === (parentId ?? "") &&
          candidate.name.localeCompare(folder.name, undefined, { sensitivity: "base" }) === 0,
      );
      if (existing) {
        folderIds.set(folder.key, existing.id);
        reusedFolderCount += 1;
      } else {
        try {
          verifySelection();
          const created = await client.createFolder(spaceId, folder.name, {
            parent_id: parentId ?? null,
          });
          verifySelection();
          folderIds.set(folder.key, created.id);
          existingFolders.push(created);
          createdFolderCount += 1;
        } catch (error) {
          failures.push({
            kind: "folder",
            key: folder.key,
            label: folder.name,
            reason: getErrorMessage(error),
          });
        }
      }
    }
    completed += 1;
    onProgress?.({ completed, total, label: folder.name });
  }

  for (const document of plan.documents) {
    const folderId = document.folderKey ? folderIds.get(document.folderKey) : undefined;
    const parentId = document.parentKey ? documentIds.get(document.parentKey) : undefined;
    let failureReason: string | undefined;
    if (document.folderKey && !folderId) failureReason = "Destination folder was not created.";
    if (document.parentKey && !parentId) failureReason = "Parent page was not created.";
    if (failureReason) {
      failures.push({
        kind: "document",
        key: document.key,
        label: document.title,
        reason: failureReason,
      });
    } else {
      const title = getAvailableDocumentTitle(
        document.title,
        folderId,
        parentId,
        occupiedDocumentTitles,
      );
      if (title !== document.title) renamedDocumentCount += 1;
      try {
        verifySelection();
        const created = await client.createDocument(spaceId, {
          title,
          content: document.content,
          folder_id: folderId ?? null,
          parent_id: parentId ?? null,
        });
        verifySelection();
        documentIds.set(document.key, created.id);
        documentIdsBySourcePath[document.sourcePath] = created.id;
        if (created.draft_id) {
          documentDraftIdsBySourcePath[document.sourcePath] = created.draft_id;
          if (
            typeof created.draft_source_digest === "string" &&
            /^[0-9a-f]{64}$/.test(created.draft_source_digest)
          )
            documentDraftSourcesBySourcePath[document.sourcePath] = created.draft_source_digest;
        }
        occupiedDocumentTitles.add(documentLocationKey(folderId, parentId, title));
        createdDocumentCount += 1;
      } catch (error) {
        failures.push({
          kind: "document",
          key: document.key,
          label: title,
          reason: getErrorMessage(error),
        });
      }
    }
    completed += 1;
    onProgress?.({ completed, total, label: document.title });
  }

  return {
    status: failures.length === 0 ? "completed" : "partial",
    createdFolderCount,
    reusedFolderCount,
    createdDocumentCount,
    renamedDocumentCount,
    documentIdsBySourcePath,
    documentDraftIdsBySourcePath,
    documentDraftSourcesBySourcePath,
    ...(draftSaveSelection ? { draftSaveSelection } : {}),
    failures,
  };
}

/**
 * Repairs portable page links and document-owned assets after hierarchy creation.
 * A failed target remains as its original relative link and is reported rather
 * than being rewritten to an invalid Starlane destination.
 */
export async function finalizeNotionImportContent(
  client: NotionImportTargetClient,
  spaceId: string,
  plan: NotionImportPlan,
  archiveBytes: Uint8Array,
  execution: NotionImportExecutionResult,
  onProgress?: (progress: NotionImportExecutionProgress) => void,
): Promise<NotionImportFinalizeResult> {
  const selection = execution.draftSaveSelection;
  const verifySelection = () => {
    if (client.captureDraftSaveSelection && !selection)
      throw new Error(
        "Original import session is unavailable. Keep the imported pages and review them before repairing links.",
      );
    if (selection) client.verifyDraftSaveSelection?.(selection);
  };
  verifySelection();
  const successfulDocuments = plan.documents.filter(
    (document) => execution.documentIdsBySourcePath[document.sourcePath],
  );
  const archiveEntries = readZipCentralDirectory(archiveBytes)
    .filter((entry) => !entry.directory && isSafeArchivePath(entry.path))
    .map((entry) => ({ ...entry, kind: classifyNotionEntry(entry.path) }));
  const assetEntries = new Map(
    archiveEntries
      .filter((entry) => entry.kind === "asset")
      .map((entry) => [entry.path, entry] as const),
  );
  const referencedAssetPaths = new Set<string>();
  for (const document of successfulDocuments) {
    for (const link of findMarkdownLinks(document.content)) {
      if (link.destination.startsWith("#") || isExternalDestination(link.destination)) continue;
      const resolved = resolveArchiveReference(document.sourcePath, link.destination);
      const asset = resolved ? assetEntries.get(resolved) : undefined;
      if (resolved && asset && asset.uncompressedBytes <= 30 * 1024 * 1024) {
        referencedAssetPaths.add(resolved);
      }
    }
  }
  const extractedAssets = await unzipSelectedEntries(archiveBytes, referencedAssetPaths);
  verifySelection();
  const documentIdsByPath = new Map(Object.entries(execution.documentIdsBySourcePath));
  const documentTitlesByPath = new Map(
    plan.documents.map((document) => [document.sourcePath, document.title] as const),
  );
  const caseFoldedDocumentPaths = makeUnambiguousCaseFoldedPathMap(documentIdsByPath.keys());
  const caseFoldedAssetPaths = makeUnambiguousCaseFoldedPathMap(assetEntries.keys());
  const failures: NotionImportExecutionFailure[] = [];
  let updatedDocumentCount = 0;
  let rewrittenPageLinkCount = 0;
  let uploadedAssetCount = 0;
  let unresolvedPageLinkCount = 0;
  let skippedAssetCount = 0;
  let completed = 0;

  for (const document of successfulDocuments) {
    const documentId = execution.documentIdsBySourcePath[document.sourcePath];
    verifySelection();
    const uploadedUrls = new Map<string, Promise<string | undefined>>();
    const transformed = await replaceMarkdownLinks(document.content, async (link) => {
      if (link.destination.startsWith("#") || isExternalDestination(link.destination)) return;
      const resolvedReference = resolveArchiveReference(document.sourcePath, link.destination);
      if (!resolvedReference) return;
      const documentPath = resolveKnownPath(
        resolvedReference,
        documentIdsByPath,
        caseFoldedDocumentPaths,
      );
      if (!link.image && isPortableDocumentPath(resolvedReference)) {
        if (documentPath && !link.destination.includes("#")) {
          const targetId = documentIdsByPath.get(documentPath);
          const targetTitle = documentTitlesByPath.get(documentPath) ?? link.label;
          if (targetId) {
            rewrittenPageLinkCount += 1;
            return `[[${sanitizeWikiLabel(link.label || targetTitle)}|${targetId}]]`;
          }
        }
        unresolvedPageLinkCount += 1;
        return;
      }

      const assetPath = resolveKnownPath(resolvedReference, assetEntries, caseFoldedAssetPaths);
      if (!assetPath) return;
      const entry = assetEntries.get(assetPath);
      const bytes = extractedAssets[assetPath];
      if (!entry || !bytes || entry.uncompressedBytes > 30 * 1024 * 1024) {
        skippedAssetCount += 1;
        return;
      }
      let savedUrl = uploadedUrls.get(assetPath);
      if (!savedUrl) {
        verifySelection();
        savedUrl = saveImportedAsset(client, spaceId, documentId, assetPath, bytes)
          .then((url) => {
            verifySelection();
            uploadedAssetCount += 1;
            return url;
          })
          .catch((error) => {
            failures.push({
              kind: "document",
              key: document.key,
              label: document.title,
              reason: `${assetPath}: ${getErrorMessage(error)}`,
            });
            return undefined;
          });
        uploadedUrls.set(assetPath, savedUrl);
      }
      const url = await savedUrl;
      verifySelection();
      if (!url) return;
      return `${link.image ? "!" : ""}[${link.label}](${url}${link.suffix})`;
    });

    if (transformed !== document.content) {
      try {
        verifySelection();
        const draftId = execution.documentDraftIdsBySourcePath[document.sourcePath];
        if (draftId) {
          const source = execution.documentDraftSourcesBySourcePath?.[document.sourcePath];
          if (
            client.requiresDraftSourceCas !== false &&
            (!source || !/^[0-9a-f]{64}$/.test(source))
          )
            throw new Error(
              "Imported draft needs review before changing its content. Keep the original import and reload with an updated client.",
            );
          await client.updateDocumentDraft(
            spaceId,
            documentId,
            draftId,
            { content: transformed },
            source,
          );
        } else {
          await client.updateDocumentContent(spaceId, documentId, transformed);
        }
        verifySelection();
        updatedDocumentCount += 1;
      } catch (error) {
        failures.push({
          kind: "document",
          key: document.key,
          label: document.title,
          reason: `Content repair failed: ${getErrorMessage(error)}`,
        });
      }
    }
    completed += 1;
    onProgress?.({
      completed,
      total: successfulDocuments.length,
      label: document.title,
    });
  }

  return {
    updatedDocumentCount,
    rewrittenPageLinkCount,
    uploadedAssetCount,
    unresolvedPageLinkCount,
    skippedAssetCount,
    failures,
  };
}

function removeArchiveRoot(path: string, rootFolder?: string): string {
  if (!rootFolder) return path;
  const prefix = `${rootFolder}/`;
  return path.startsWith(prefix) ? path.slice(prefix.length) : path;
}

interface MarkdownLinkMatch {
  start: number;
  end: number;
  original: string;
  image: boolean;
  label: string;
  destination: string;
  suffix: string;
}

function findMarkdownLinks(markdown: string): MarkdownLinkMatch[] {
  const protectedRanges = findMarkdownCodeRanges(markdown);
  const matches: MarkdownLinkMatch[] = [];
  const pattern = /(!?)\[([^\]\n]*)\]\(([^)\n]+)\)/g;
  for (const match of markdown.matchAll(pattern)) {
    const start = match.index ?? 0;
    if (protectedRanges.some((range) => start >= range.start && start < range.end)) continue;
    const rawTarget = match[3].trim();
    let destination = rawTarget;
    let suffix = "";
    if (rawTarget.startsWith("<")) {
      const close = rawTarget.indexOf(">");
      if (close > 0) {
        destination = rawTarget.slice(1, close);
        suffix = rawTarget.slice(close + 1);
      }
    } else {
      const destinationMatch = /^(\S+)([\s\S]*)$/.exec(rawTarget);
      if (destinationMatch) {
        destination = destinationMatch[1];
        suffix = destinationMatch[2];
      }
    }
    matches.push({
      start,
      end: start + match[0].length,
      original: match[0],
      image: match[1] === "!",
      label: match[2],
      destination,
      suffix,
    });
  }
  return matches;
}

async function replaceMarkdownLinks(
  markdown: string,
  replace: (link: MarkdownLinkMatch) => Promise<string | undefined>,
): Promise<string> {
  const links = findMarkdownLinks(markdown);
  let cursor = 0;
  let output = "";
  for (const link of links) {
    output += markdown.slice(cursor, link.start);
    output += (await replace(link)) ?? link.original;
    cursor = link.end;
  }
  return output + markdown.slice(cursor);
}

function findMarkdownCodeRanges(markdown: string): Array<{ start: number; end: number }> {
  const ranges: Array<{ start: number; end: number }> = [];
  let offset = 0;
  let fence: { character: string; length: number; start: number } | undefined;
  for (const line of markdown.split(/(?<=\n)/)) {
    const fenceMatch = /^\s*(`{3,}|~{3,})/.exec(line);
    if (fenceMatch) {
      const marker = fenceMatch[1];
      if (!fence) {
        fence = { character: marker[0], length: marker.length, start: offset };
      } else if (marker[0] === fence.character && marker.length >= fence.length) {
        ranges.push({ start: fence.start, end: offset + line.length });
        fence = undefined;
      }
      offset += line.length;
      continue;
    }
    if (!fence) {
      const inlinePattern = /(`+)([^`]*?)\1/g;
      for (const match of line.matchAll(inlinePattern)) {
        const start = offset + (match.index ?? 0);
        ranges.push({ start, end: start + match[0].length });
      }
    }
    offset += line.length;
  }
  if (fence) ranges.push({ start: fence.start, end: markdown.length });
  return ranges;
}

function resolveArchiveReference(sourcePath: string, destination: string): string | undefined {
  const withoutFragment = destination.split("#", 1)[0].split("?", 1)[0];
  if (!withoutFragment || withoutFragment.startsWith("/")) return undefined;
  let decoded: string;
  try {
    decoded = decodeURIComponent(withoutFragment).replace(/\\/g, "/");
  } catch {
    return undefined;
  }
  const resolved: string[] = sourcePath.split("/").slice(0, -1);
  for (const segment of decoded.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") {
      if (resolved.length === 0) return undefined;
      resolved.pop();
    } else {
      resolved.push(segment);
    }
  }
  return resolved.join("/");
}

function isExternalDestination(destination: string): boolean {
  return /^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(destination);
}

function isPortableDocumentPath(path: string): boolean {
  return /\.(?:md|markdown|csv)$/i.test(path);
}

function makeUnambiguousCaseFoldedPathMap(paths: Iterable<string>): Map<string, string> {
  const candidates = new Map<string, string | null>();
  for (const path of paths) {
    const key = path.toLocaleLowerCase();
    candidates.set(key, candidates.has(key) ? null : path);
  }
  return new Map(
    [...candidates.entries()].filter((entry): entry is [string, string] => entry[1] !== null),
  );
}

function resolveKnownPath<T>(
  path: string,
  exact: Map<string, T>,
  caseFolded: Map<string, string>,
): string | undefined {
  if (exact.has(path)) return path;
  return caseFolded.get(path.toLocaleLowerCase());
}

function sanitizeWikiLabel(label: string): string {
  return (
    label
      .replace(/[\[\]|\r\n]/g, " ")
      .replace(/\s+/g, " ")
      .trim() || "Untitled"
  );
}

async function saveImportedAsset(
  client: NotionImportTargetClient,
  spaceId: string,
  documentId: string,
  assetPath: string,
  bytes: Uint8Array,
): Promise<string> {
  const rawName = assetPath.split("/").pop() || "attachment";
  let name = rawName;
  try {
    name = decodeURIComponent(rawName);
  } catch {
    // Keep the literal archive filename when percent decoding is incomplete.
  }
  const saved = await client.saveDocumentDataAsset({
    name,
    mime_type: inferMimeType(name),
    data_url: `data:${inferMimeType(name)};base64,${bytesToBase64(bytes)}`,
    space_id: spaceId,
    document_id: documentId,
  });
  return saved.filename ? saved.url : `starlane://asset/${saved.id}`;
}

function inferMimeType(name: string): string {
  const extension = name.split(".").pop()?.toLocaleLowerCase();
  return (
    (
      {
        avif: "image/avif",
        csv: "text/csv",
        gif: "image/gif",
        jpeg: "image/jpeg",
        jpg: "image/jpeg",
        json: "application/json",
        md: "text/markdown",
        mp3: "audio/mpeg",
        mp4: "video/mp4",
        pdf: "application/pdf",
        png: "image/png",
        svg: "image/svg+xml",
        txt: "text/plain",
        webm: "video/webm",
        webp: "image/webp",
        zip: "application/zip",
      } as Record<string, string>
    )[extension ?? ""] ?? "application/octet-stream"
  );
}

function bytesToBase64(bytes: Uint8Array): string {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let output = "";
  for (let index = 0; index < bytes.length; index += 3) {
    const first = bytes[index];
    const second = bytes[index + 1];
    const third = bytes[index + 2];
    output += alphabet[first >> 2];
    output += alphabet[((first & 3) << 4) | ((second ?? 0) >> 4)];
    output += second === undefined ? "=" : alphabet[((second & 15) << 2) | ((third ?? 0) >> 6)];
    output += third === undefined ? "=" : alphabet[third & 63];
  }
  return output;
}

function unzipSelectedEntries(
  data: Uint8Array,
  paths: Set<string>,
): Promise<Record<string, Uint8Array>> {
  if (paths.size === 0) return Promise.resolve({});
  return new Promise((resolve, reject) => {
    unzip(data, { filter: (entry) => paths.has(entry.name) }, (error, extracted) => {
      if (error) {
        reject(new Error(`Unable to read exported assets: ${error.message}`));
        return;
      }
      resolve(extracted);
    });
  });
}

function removeFileExtension(path: string): string {
  return path.replace(/\.(?:md|markdown|csv)$/i, "");
}

function normalizeNotionName(value: string | undefined, fallback: string): string {
  let decoded = value ?? "";
  try {
    decoded = decodeURIComponent(decoded);
  } catch {
    // Preserve the literal exported name when a percent sequence is incomplete.
  }
  const withoutId = decoded.replace(
    /\s+(?:[0-9a-f]{32}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/i,
    "",
  );
  return (withoutId.trim() || fallback).slice(0, 500);
}

export function csvToMarkdown(csv: string): string {
  const rows = parseCsv(csv);
  while (rows.length > 0 && rows[rows.length - 1].every((cell) => !cell.trim())) rows.pop();
  if (rows.length === 0) return "_No rows were exported from this Notion database._";
  const columnCount = Math.max(...rows.map((row) => row.length), 1);
  const header = Array.from({ length: columnCount }, (_, index) => {
    const value = rows[0]?.[index]?.replace(/^\uFEFF/, "").trim();
    return escapeMarkdownTableCell(value || `Column ${index + 1}`);
  });
  const body = rows
    .slice(1)
    .map((row) =>
      Array.from({ length: columnCount }, (_, index) => escapeMarkdownTableCell(row[index] ?? "")),
    );
  return [
    `| ${header.join(" | ")} |`,
    `| ${header.map(() => "---").join(" | ")} |`,
    ...body.map((row) => `| ${row.join(" | ")} |`),
  ].join("\n");
}

function parseCsv(csv: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  for (let index = 0; index < csv.length; index += 1) {
    const character = csv[index];
    if (quoted) {
      if (character === '"' && csv[index + 1] === '"') {
        field += '"';
        index += 1;
      } else if (character === '"') {
        quoted = false;
      } else {
        field += character;
      }
      continue;
    }
    if (character === '"' && field.length === 0) {
      quoted = true;
    } else if (character === ",") {
      row.push(field);
      field = "";
    } else if (character === "\n" || character === "\r") {
      if (character === "\r" && csv[index + 1] === "\n") index += 1;
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
    } else {
      field += character;
    }
  }
  if (field || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}

function escapeMarkdownTableCell(value: string): string {
  return value.trim().replace(/\\/g, "\\\\").replace(/\|/g, "\\|").replace(/\r?\n/g, "<br>");
}

function documentLocationKey(
  folderId: string | undefined,
  parentId: string | undefined,
  title: string,
): string {
  return `${folderId ?? ""}:${parentId ?? ""}:${title.trim().toLocaleLowerCase()}`;
}

function getAvailableDocumentTitle(
  title: string,
  folderId: string | undefined,
  parentId: string | undefined,
  occupied: Set<string>,
): string {
  if (!occupied.has(documentLocationKey(folderId, parentId, title))) return title;
  let suffix = 1;
  while (suffix < 10_000) {
    const candidate =
      suffix === 1 ? `${title} (Notion import)` : `${title} (Notion import ${suffix})`;
    if (!occupied.has(documentLocationKey(folderId, parentId, candidate))) return candidate;
    suffix += 1;
  }
  throw new Error(`Unable to choose an available title for ${title}.`);
}

function getErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Unknown import error.";
}

function readZipCentralDirectory(data: Uint8Array): ZipDirectoryEntry[] {
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  const minimumOffset = Math.max(0, data.byteLength - ZIP_EOCD_BYTES - ZIP_MAX_COMMENT_BYTES);
  let endOffset = -1;
  for (let offset = data.byteLength - ZIP_EOCD_BYTES; offset >= minimumOffset; offset -= 1) {
    if (view.getUint32(offset, true) === ZIP_EOCD_SIGNATURE) {
      endOffset = offset;
      break;
    }
  }
  if (endOffset < 0) throw new Error("The selected file is not a readable ZIP archive.");

  const diskNumber = view.getUint16(endOffset + 4, true);
  const centralDisk = view.getUint16(endOffset + 6, true);
  const entryCount = view.getUint16(endOffset + 10, true);
  const centralSize = view.getUint32(endOffset + 12, true);
  const centralOffset = view.getUint32(endOffset + 16, true);
  if (diskNumber !== 0 || centralDisk !== 0)
    throw new Error("Multi-disk ZIP archives are not supported.");
  if (entryCount === 0xffff || centralSize === ZIP64_SENTINEL || centralOffset === ZIP64_SENTINEL) {
    throw new Error("ZIP64 archives are not supported by the browser importer.");
  }
  if (centralOffset + centralSize > endOffset || centralOffset > data.byteLength) {
    throw new Error("The ZIP central directory is incomplete or corrupt.");
  }

  const decoder = new TextDecoder("utf-8", { fatal: true });
  const entries: ZipDirectoryEntry[] = [];
  let offset = centralOffset;
  for (let index = 0; index < entryCount; index += 1) {
    if (offset + 46 > data.byteLength || view.getUint32(offset, true) !== ZIP_CENTRAL_SIGNATURE) {
      throw new Error("The ZIP central directory is incomplete or corrupt.");
    }
    const versionMadeBy = view.getUint16(offset + 4, true);
    const flags = view.getUint16(offset + 8, true);
    const compression = view.getUint16(offset + 10, true);
    const compressedBytes = view.getUint32(offset + 20, true);
    const uncompressedBytes = view.getUint32(offset + 24, true);
    const nameLength = view.getUint16(offset + 28, true);
    const extraLength = view.getUint16(offset + 30, true);
    const commentLength = view.getUint16(offset + 32, true);
    const externalAttributes = view.getUint32(offset + 38, true);
    const nextOffset = offset + 46 + nameLength + extraLength + commentLength;
    if (nextOffset > data.byteLength) throw new Error("The ZIP contains a truncated entry name.");
    let path: string;
    try {
      path = decoder
        .decode(data.subarray(offset + 46, offset + 46 + nameLength))
        .replace(/\\/g, "/");
    } catch {
      throw new Error("The ZIP contains a filename that is not valid UTF-8.");
    }
    const unixMode = versionMadeBy >> 8 === 3 ? (externalAttributes >>> 16) & 0xffff : 0;
    entries.push({
      path,
      compressedBytes,
      uncompressedBytes,
      compression,
      flags,
      directory: path.endsWith("/") || (externalAttributes & 0x10) !== 0,
      symlink: (unixMode & 0xf000) === 0xa000,
    });
    offset = nextOffset;
  }
  return entries;
}

function isSafeArchivePath(path: string): boolean {
  if (!path || path.length > 1024 || path.includes("\0")) return false;
  if (path.startsWith("/") || /^[a-zA-Z]:\//.test(path)) return false;
  return path.split("/").every((segment) => segment !== "..");
}

function classifyNotionEntry(path: string): NotionImportEntryKind {
  const segments = path.split("/").filter(Boolean);
  const basename = segments[segments.length - 1]?.toLowerCase() ?? "";
  if (segments.some((segment) => segment.startsWith(".")) || segments.includes("__MACOSX")) {
    return "ignored";
  }
  if (basename === "index.html") return "ignored";
  if (/\.(md|markdown)$/i.test(basename)) return "page";
  if (/\.csv$/i.test(basename)) return "database";
  if (/\.(html?|opml|epub|zip)$/i.test(basename)) return "unsupported";
  return "asset";
}

function countKind(entries: NotionImportEntry[], kind: NotionImportEntryKind): number {
  return entries.reduce((count, entry) => count + (entry.kind === kind ? 1 : 0), 0);
}

function findCommonRoot(entries: NotionImportEntry[]): string | undefined {
  const relevant = entries.filter((entry) => entry.kind !== "ignored");
  const firstSegments = relevant.map((entry) => entry.path.split("/").filter(Boolean));
  if (firstSegments.length === 0 || firstSegments.some((segments) => segments.length < 2))
    return undefined;
  const root = firstSegments[0][0];
  return firstSegments.every((segments) => segments[0] === root) ? root : undefined;
}

function unzipTextEntries(data: Uint8Array): Promise<Record<string, Uint8Array>> {
  return new Promise((resolve, reject) => {
    unzip(
      data,
      {
        filter: (entry) => /\.(md|markdown|csv)$/i.test(entry.name),
      },
      (error, extracted) => {
        if (error) {
          reject(new Error(`Unable to read the Notion export: ${error.message}`));
          return;
        }
        resolve(extracted);
      },
    );
  });
}
