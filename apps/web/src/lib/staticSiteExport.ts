import MarkdownIt from "markdown-it";
import type {
  LocalVaultSnapshot,
  VaultAsset,
  VaultDocument,
  VaultPropertyValue,
} from "@/lib/localVault";

export interface StaticSiteExportFile {
  filename: string;
  html: string;
  document_count: number;
}

export interface StaticSiteBundleFile {
  path: string;
  content: string | Uint8Array;
  media_type: string;
}

export interface StaticSiteExportBundle {
  filename: string;
  document_count: number;
  asset_count: number;
  skipped_asset_count: number;
  files: StaticSiteBundleFile[];
}

export interface StaticSiteExportOptions {
  title?: string;
  description?: string;
  generatedAt?: string;
  theme?: StaticSiteTheme;
}

export type StaticSiteTheme = "light" | "dark" | "paper";

const markdown = new MarkdownIt({
  html: false,
  linkify: true,
  typographer: true,
});

export function buildStaticSiteHtml(
  snapshot: LocalVaultSnapshot,
  options: StaticSiteExportOptions = {},
): StaticSiteExportFile {
  const generatedAt = options.generatedAt ?? new Date().toISOString();
  const title = options.title ?? inferSiteTitle(snapshot);
  const description = options.description?.trim() ?? "";
  const theme = options.theme ?? "light";
  const themeTokens = getStaticSiteThemeTokens(theme);
  const documents = [...snapshot.documents].sort(compareDocuments(snapshot));
  const folderById = new Map(snapshot.folders.map((folder) => [folder.id, folder]));
  const spaceById = new Map(snapshot.spaces.map((space) => [space.id, space]));
  const tagsByDocument = buildTagsByDocument(snapshot);
  const propertiesByDocument = buildPropertiesByDocument(snapshot);

  const articles = documents
    .map((document) => {
      const space = spaceById.get(document.space_id);
      const folder = document.folder_id ? folderById.get(document.folder_id) : undefined;
      const tags = tagsByDocument.get(document.id) ?? [];
      const properties = propertiesByDocument.get(document.id) ?? [];
      const articleId = makeAnchorId(document);
      return `
        <article id="${articleId}" class="document">
          <header>
            <p class="eyebrow">${escapeHtml(space?.name ?? "Unknown space")}${folder ? ` / ${escapeHtml(folder.name)}` : ""}</p>
            <h2>${escapeHtml(document.title)}</h2>
            <p class="meta">Updated ${formatDate(document.updated_at)}${document.created_at ? ` / Created ${formatDate(document.created_at)}` : ""}</p>
            ${tags.length > 0 ? `<div class="tags">${tags.map(renderTag).join("")}</div>` : ""}
            ${properties.length > 0 ? `<dl class="properties">${properties.map(renderProperty).join("")}</dl>` : ""}
          </header>
          <div class="content">${markdown.render(document.content || "")}</div>
        </article>
      `;
    })
    .join("\n");

  const navItems = documents
    .map((document) => {
      const space = spaceById.get(document.space_id);
      return `<li><a href="#${makeAnchorId(document)}">${escapeHtml(document.title)}</a><span>${escapeHtml(space?.name ?? "Unknown space")}</span></li>`;
    })
    .join("\n");

  return {
    filename: `${slugify(title) || "starlane-site"}.html`,
    document_count: documents.length,
    html: `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  ${description ? `<meta name="description" content="${escapeHtmlAttribute(description)}" />` : ""}
  <title>${escapeHtml(title)}</title>
  <style>
    :root { color-scheme: ${themeTokens.colorScheme}; --text: ${themeTokens.text}; --muted: ${themeTokens.muted}; --line: ${themeTokens.line}; --bg: ${themeTokens.bg}; --surface: ${themeTokens.surface}; --accent: ${themeTokens.accent}; --code-bg: ${themeTokens.codeBg}; }
    * { box-sizing: border-box; }
    body { margin: 0; background: var(--bg); color: var(--text); font: 16px/1.55 Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    a { color: var(--accent); text-decoration: none; }
    a:hover { text-decoration: underline; }
    .shell { display: grid; grid-template-columns: minmax(14rem, 20rem) minmax(0, 1fr); min-height: 100vh; }
    aside { border-right: 1px solid var(--line); background: var(--surface); padding: 2rem; position: sticky; top: 0; height: 100vh; overflow: auto; }
    main { max-width: 56rem; width: 100%; padding: 2.5rem; }
    h1 { font-size: 1.7rem; line-height: 1.2; margin: 0 0 .5rem; }
    h2 { font-size: 1.55rem; line-height: 1.25; margin: 0; }
    .generated, .site-description, .meta, .eyebrow, nav span { color: var(--muted); font-size: .82rem; }
    .site-description { margin: .75rem 0 0; }
    nav ul { list-style: none; margin: 1.5rem 0 0; padding: 0; display: grid; gap: .7rem; }
    nav li { display: grid; gap: .1rem; }
    .document { border-bottom: 1px solid var(--line); padding: 0 0 2.5rem; margin: 0 0 2.5rem; }
    .document header { display: grid; gap: .5rem; margin-bottom: 1.25rem; }
    .tags { display: flex; flex-wrap: wrap; gap: .4rem; }
    .tag { border: 1px solid var(--line); border-radius: 999px; padding: .15rem .55rem; font-size: .78rem; }
    .properties { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: .25rem .8rem; margin: .25rem 0 0; font-size: .85rem; }
    .properties dt { color: var(--muted); }
    .properties dd { margin: 0; }
    .content :first-child { margin-top: 0; }
    .content img { max-width: 100%; }
    .content pre { overflow: auto; border: 1px solid var(--line); border-radius: .5rem; padding: 1rem; background: var(--code-bg); }
    .content code { font-family: "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .9em; }
    @media (max-width: 760px) {
      .shell { display: block; }
      aside { position: static; height: auto; border-right: 0; border-bottom: 1px solid var(--line); }
      main { padding: 1.5rem; }
    }
  </style>
</head>
<body class="theme-${theme}">
  <div class="shell">
    <aside>
      <h1>${escapeHtml(title)}</h1>
      ${description ? `<p class="site-description">${escapeHtml(description)}</p>` : ""}
      <p class="generated">${documents.length} document${documents.length === 1 ? "" : "s"} / generated ${formatDate(generatedAt)}</p>
      <nav aria-label="Documents"><ul>${navItems}</ul></nav>
    </aside>
    <main>${articles || `<p class="generated">No documents were available in this export.</p>`}</main>
  </div>
</body>
</html>`,
  };
}

export function buildStaticSiteBundle(
  snapshot: LocalVaultSnapshot,
  options: StaticSiteExportOptions = {},
): StaticSiteExportBundle {
  const generatedAt = options.generatedAt ?? new Date().toISOString();
  const title = options.title ?? inferSiteTitle(snapshot);
  const description = options.description?.trim() ?? "";
  const theme = options.theme ?? "light";
  const themeTokens = getStaticSiteThemeTokens(theme);
  const documents = [...snapshot.documents].sort(compareDocuments(snapshot));
  const folderById = new Map(snapshot.folders.map((folder) => [folder.id, folder]));
  const spaceById = new Map(snapshot.spaces.map((space) => [space.id, space]));
  const tagsByDocument = buildTagsByDocument(snapshot);
  const propertiesByDocument = buildPropertiesByDocument(snapshot);
  const usedPaths = new Set<string>();
  const documentPaths = new Map<string, string>();
  const extractedAssets = buildStaticSiteAssetFiles(snapshot.assets, usedPaths);
  const skippedAssetsByDocument = buildSkippedAssetsByDocument(
    documents,
    extractedAssets.skippedAssets,
  );

  for (const document of documents) {
    documentPaths.set(document.id, makeUniqueDocumentPath(document, usedPaths));
  }

  const navItems = documents
    .map((document) => {
      const space = spaceById.get(document.space_id);
      const path = documentPaths.get(document.id) ?? "#";
      return `<li><a href="${escapeHtmlAttribute(path)}">${escapeHtml(document.title)}</a><span>${escapeHtml(space?.name ?? "Unknown space")}</span></li>`;
    })
    .join("\n");

  const files: StaticSiteBundleFile[] = [
    {
      path: "styles.css",
      media_type: "text/css;charset=utf-8",
      content: buildStaticSiteCss(themeTokens),
    },
    {
      path: "index.html",
      media_type: "text/html;charset=utf-8",
      content: `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  ${description ? `<meta name="description" content="${escapeHtmlAttribute(description)}" />` : ""}
  <title>${escapeHtml(title)}</title>
  <link rel="stylesheet" href="styles.css" />
</head>
<body class="theme-${theme}">
  <div class="shell">
    <aside>
      <h1>${escapeHtml(title)}</h1>
      ${description ? `<p class="site-description">${escapeHtml(description)}</p>` : ""}
      <p class="generated">${documents.length} document${documents.length === 1 ? "" : "s"} / generated ${formatDate(generatedAt)}</p>
      <nav aria-label="Documents"><ul>${navItems}</ul></nav>
    </aside>
    <main>${documents.length === 0 ? `<p class="generated">No documents were available in this export.</p>` : `<p class="generated">Select a document from the index.</p>`}</main>
  </div>
</body>
</html>`,
    },
  ];

  files.push({
    path: "site.json",
    media_type: "application/json;charset=utf-8",
    content: buildStaticSiteBundleManifest({
      title,
      description,
      generatedAt,
      theme,
      documents,
      documentPaths,
      folderById,
      spaceById,
      tagsByDocument,
      propertiesByDocument,
      skippedAssetsByDocument,
      assets: extractedAssets.manifestAssets,
      skippedAssets: extractedAssets.skippedAssets,
    }),
  });

  for (const document of documents) {
    const space = spaceById.get(document.space_id);
    const folder = document.folder_id ? folderById.get(document.folder_id) : undefined;
    const tags = tagsByDocument.get(document.id) ?? [];
    const properties = propertiesByDocument.get(document.id) ?? [];
    files.push({
      path: documentPaths.get(document.id) ?? makeUniqueDocumentPath(document, usedPaths),
      media_type: "text/html;charset=utf-8",
      content: `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  ${description ? `<meta name="description" content="${escapeHtmlAttribute(description)}" />` : ""}
  <title>${escapeHtml(document.title)} - ${escapeHtml(title)}</title>
  <link rel="stylesheet" href="../styles.css" />
</head>
<body class="theme-${theme}">
  <main class="document-page">
    <p class="breadcrumb"><a href="../index.html">${escapeHtml(title)}</a></p>
    <article class="document">
      <header>
        <p class="eyebrow">${escapeHtml(space?.name ?? "Unknown space")}${folder ? ` / ${escapeHtml(folder.name)}` : ""}</p>
        <h1>${escapeHtml(document.title)}</h1>
        <p class="meta">Updated ${formatDate(document.updated_at)}${document.created_at ? ` / Created ${formatDate(document.created_at)}` : ""}</p>
        ${tags.length > 0 ? `<div class="tags">${tags.map(renderTag).join("")}</div>` : ""}
        ${properties.length > 0 ? `<dl class="properties">${properties.map(renderProperty).join("")}</dl>` : ""}
      </header>
      <div class="content">${markdown.render(rewriteContentAssetUrls(document.content || "", extractedAssets.urlMap, "../"))}</div>
    </article>
  </main>
</body>
</html>`,
    });
  }

  files.push(...extractedAssets.files);
  files.push({
    path: "integrity.json",
    media_type: "application/json;charset=utf-8",
    content: buildStaticSiteIntegrityManifest({
      generatedAt,
      files,
    }),
  });
  files.push({
    path: "deploy-local.sh",
    media_type: "text/x-shellscript;charset=utf-8",
    content: buildStaticSiteLocalDeployScript(files),
  });
  files.push({
    path: "deploy-rsync.sh",
    media_type: "text/x-shellscript;charset=utf-8",
    content: buildStaticSiteRsyncDeployScript(files),
  });
  files.push({
    path: "deploy.json",
    media_type: "application/json;charset=utf-8",
    content: buildStaticSiteDeployPlan({
      title,
      generatedAt,
      documentCount: documents.length,
      assetCount: extractedAssets.files.length,
      skippedAssetCount: extractedAssets.skippedAssets.length,
      skippedAssets: extractedAssets.skippedAssets,
      files,
      deployHookPaths: ["deploy-local.sh", "deploy-rsync.sh"],
    }),
  });

  return {
    filename: `${slugify(title) || "starlane-site"}.zip`,
    document_count: documents.length,
    asset_count: extractedAssets.files.length,
    skipped_asset_count: extractedAssets.skippedAssets.length,
    files,
  };
}

export function formatStaticSiteBundleSummary(bundle: StaticSiteExportBundle, label = "Static ZIP export"): string {
  const skippedAssets =
    bundle.skipped_asset_count > 0
      ? `, ${bundle.skipped_asset_count} skipped asset${bundle.skipped_asset_count === 1 ? "" : "s"} listed in site.json/deploy.json`
      : "";
  return `${label} prepared with ${bundle.document_count} document${bundle.document_count === 1 ? "" : "s"}, ${bundle.asset_count} extracted asset${bundle.asset_count === 1 ? "" : "s"}${skippedAssets}, site.json, integrity.json, deploy.json, deploy-local.sh, and deploy-rsync.sh.`;
}

export function buildStaticSiteZipBlob(bundle: StaticSiteExportBundle): Blob {
  const archive = buildStaticSiteZipArchive(bundle);
  const body = archive.buffer.slice(
    archive.byteOffset,
    archive.byteOffset + archive.byteLength,
  ) as ArrayBuffer;
  return new Blob([body], { type: "application/zip" });
}

export function buildStaticSiteZipArchive(bundle: StaticSiteExportBundle): Uint8Array {
  const encoder = new TextEncoder();
  const localChunks: Uint8Array[] = [];
  const centralChunks: Uint8Array[] = [];
  const entries: Array<{
    nameBytes: Uint8Array;
    dataBytes: Uint8Array;
    crc: number;
    localOffset: number;
  }> = [];
  let localSize = 0;

  for (const file of bundle.files) {
    const nameBytes = encoder.encode(file.path);
    const dataBytes = encodeBundleContent(file.content, encoder);
    const crc = crc32(dataBytes);
    const localHeader = makeZipLocalHeader(nameBytes, dataBytes, crc);
    entries.push({ nameBytes, dataBytes, crc, localOffset: localSize });
    localChunks.push(localHeader, nameBytes, dataBytes);
    localSize += localHeader.length + nameBytes.length + dataBytes.length;
  }

  let centralSize = 0;
  for (const entry of entries) {
    const centralHeader = makeZipCentralHeader(entry);
    centralChunks.push(centralHeader, entry.nameBytes);
    centralSize += centralHeader.length + entry.nameBytes.length;
  }

  const endRecord = makeZipEndRecord(entries.length, centralSize, localSize);
  return concatBytes([...localChunks, ...centralChunks, endRecord], localSize + centralSize + endRecord.length);
}

function buildStaticSiteAssetFiles(
  assets: VaultAsset[],
  usedPaths: Set<string>,
): {
  files: StaticSiteBundleFile[];
  manifestAssets: Array<{
    id: string;
    name: string;
    path: string;
    media_type: string;
    size: number;
  }>;
  skippedAssets: StaticSiteSkippedAsset[];
  urlMap: Map<string, string>;
} {
  const files: StaticSiteBundleFile[] = [];
  const manifestAssets: Array<{
    id: string;
    name: string;
    path: string;
    media_type: string;
    size: number;
  }> = [];
  const skippedAssets: StaticSiteSkippedAsset[] = [];
  const urlMap = new Map<string, string>();

  for (const asset of assets) {
    const content = decodeDataUrlAsset(asset.url);
    if (!content) {
      skippedAssets.push({
        id: asset.id,
        name: asset.name,
        source_url: asset.url,
        source_references: getAssetSourceReferences(asset),
        reason: inferSkippedAssetReason(asset.url),
      });
      continue;
    }

    const path = makeUniqueAssetPath(asset, usedPaths);
    const mediaType = asset.mime_type || inferMediaTypeFromAssetName(asset.name);
    files.push({
      path,
      content,
      media_type: mediaType,
    });
    manifestAssets.push({
      id: asset.id,
      name: asset.name,
      path,
      media_type: mediaType,
      size: content.byteLength,
    });
    for (const reference of getAssetSourceReferences(asset)) {
      urlMap.set(reference, path);
    }
  }

  return { files, manifestAssets, skippedAssets, urlMap };
}

type StaticSiteSkippedAsset = {
  id: string;
  name: string;
  source_url?: string;
  source_references?: string[];
  reason: "remote_url_not_embedded" | "invalid_data_url";
};

function getAssetSourceReferences(asset: VaultAsset): string[] {
  const references = new Set<string>();
  for (const value of [
    asset.url,
    asset.vault_path,
    asset.vault_path ? getPathBasename(asset.vault_path) : undefined,
    `starlane://asset/${asset.id}`,
    `starlane://asset/${asset.name}`,
    `starlane://asset/${encodeURIComponent(asset.name)}`,
  ]) {
    const trimmed = value?.trim();
    if (trimmed) references.add(trimmed);
  }
  return [...references];
}

function getPathBasename(path: string): string | undefined {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1];
}

function decodeDataUrlAsset(url: string): Uint8Array | null {
  const match = /^data:([^;,]+)?(;base64)?,(.*)$/i.exec(url);
  if (!match) return null;

  const isBase64 = Boolean(match[2]);
  const payload = match[3] ?? "";
  try {
    if (isBase64) {
      const binary = globalThis.atob(payload);
      const bytes = new Uint8Array(binary.length);
      for (let index = 0; index < binary.length; index += 1) {
        bytes[index] = binary.charCodeAt(index);
      }
      return bytes;
    }
    return new TextEncoder().encode(decodeURIComponent(payload));
  } catch {
    return null;
  }
}

function inferSkippedAssetReason(url: string): StaticSiteSkippedAsset["reason"] {
  return url.trim().toLowerCase().startsWith("data:") ? "invalid_data_url" : "remote_url_not_embedded";
}

function buildSkippedAssetsByDocument(
  documents: VaultDocument[],
  skippedAssets: StaticSiteSkippedAsset[],
): Map<string, StaticSiteSkippedAsset[]> {
  const result = new Map<string, StaticSiteSkippedAsset[]>();
  if (skippedAssets.length === 0) return result;

  for (const document of documents) {
    const matches = skippedAssets.filter((asset) =>
      (asset.source_references ?? [asset.source_url]).some(
        (reference) => reference && document.content.includes(reference),
      ),
    );
    if (matches.length > 0) result.set(document.id, matches);
  }

  return result;
}

function rewriteContentAssetUrls(
  content: string,
  assetUrlMap: Map<string, string>,
  relativePrefix: string,
): string {
  let rewritten = content;
  for (const [sourceUrl, targetPath] of assetUrlMap.entries()) {
    rewritten = rewritten.split(sourceUrl).join(`${relativePrefix}${targetPath}`);
  }
  return rewritten;
}

function buildStaticSiteBundleManifest(input: {
  title: string;
  description: string;
  generatedAt: string;
  theme: StaticSiteTheme;
  documents: VaultDocument[];
  documentPaths: Map<string, string>;
  folderById: Map<string, { id: string; name: string }>;
  spaceById: Map<string, { id: string; name: string }>;
  tagsByDocument: Map<string, Array<{ name: string; color: string }>>;
  propertiesByDocument: Map<string, Array<{ name: string; value: string }>>;
  skippedAssetsByDocument: Map<string, StaticSiteSkippedAsset[]>;
  assets: Array<{
    id: string;
    name: string;
    path: string;
    media_type: string;
    size: number;
  }>;
  skippedAssets: StaticSiteSkippedAsset[];
}): string {
  return `${JSON.stringify(
    {
      schema_version: 1,
      title: input.title,
      description: input.description || undefined,
      generated_at: input.generatedAt,
      theme: input.theme,
      document_count: input.documents.length,
      asset_count: input.assets.length,
      skipped_asset_count: input.skippedAssets.length,
      documents: input.documents.map((document) => {
        const space = input.spaceById.get(document.space_id);
        const folder = document.folder_id ? input.folderById.get(document.folder_id) : undefined;
        return {
          id: document.id,
          title: document.title,
          path: input.documentPaths.get(document.id),
          space_id: document.space_id,
          space_name: space?.name,
          folder_id: document.folder_id,
          folder_name: folder?.name,
          created_at: document.created_at,
          updated_at: document.updated_at,
          tags: input.tagsByDocument.get(document.id) ?? [],
          properties: input.propertiesByDocument.get(document.id) ?? [],
          skipped_assets: input.skippedAssetsByDocument.get(document.id) ?? [],
        };
      }),
      assets: input.assets,
      skipped_assets: input.skippedAssets,
    },
    null,
    2,
  )}\n`;
}

function buildStaticSiteDeployPlan(input: {
  title: string;
  generatedAt: string;
  documentCount: number;
  assetCount: number;
  skippedAssetCount: number;
  skippedAssets: StaticSiteSkippedAsset[];
  files: StaticSiteBundleFile[];
  deployHookPaths?: string[];
}): string {
  return `${JSON.stringify(
    {
      schema_version: 1,
      title: input.title,
      generated_at: input.generatedAt,
      strategy: "static_zip_manual_upload",
      publish_root: ".",
      entrypoint: "index.html",
      manifest: "site.json",
      integrity_manifest: "integrity.json",
      deploy_hooks: input.deployHookPaths?.length
        ? input.deployHookPaths.map((path) =>
          path === "deploy-local.sh"
            ? {
                path,
                kind: "local_directory_copy",
                description:
                  "Copies generated static site files into a local publish directory argument.",
                command: `sh ${path} <publish-directory>`,
              }
            : {
                path,
                kind: "ssh_rsync",
                description:
                  "Uploads generated files to an explicit SSH destination supplied by the operator environment.",
                command: `STARLANE_STATIC_DEPLOY_TARGET=user@example.com:/var/www/site sh ${path}`,
                required_environment: ["STARLANE_STATIC_DEPLOY_TARGET"],
                credentials: "Use SSH agent or SSH configuration outside this export bundle.",
              },
        )
        : [],
      document_count: input.documentCount,
      asset_count: input.assetCount,
      skipped_asset_count: input.skippedAssetCount,
      skipped_assets: input.skippedAssets,
      upload_files: input.files
        .filter((file) => !input.deployHookPaths?.includes(file.path))
        .map((file) => ({
          path: file.path,
          media_type: file.media_type,
          size: estimateBundleContentSize(file.content),
        })),
      guardrails: [
        "Review site.json before publishing.",
        "Upload files with their relative paths preserved.",
        "No actual credentials or deployment destination is stored in this plan.",
      ],
    },
    null,
    2,
  )}\n`;
}

function buildStaticSiteLocalDeployScript(files: StaticSiteBundleFile[]): string {
  const publishablePaths = files
    .map((file) => file.path)
    .filter((path) => path !== "deploy.json" && path !== "deploy-local.sh" && path !== "deploy-rsync.sh")
    .sort();

  const quotedPaths = publishablePaths.map((path) => `"${escapeShellDoubleQuoted(path)}"`).join(" ");

  return `#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
  echo "Usage: sh deploy-local.sh <publish-directory>" >&2
  exit 64
fi

target_dir="$1"
mkdir -p "$target_dir"

for source_path in ${quotedPaths}; do
  if [ ! -f "$source_path" ]; then
    echo "Missing export file: $source_path" >&2
    exit 66
  fi
  destination_path="$target_dir/$source_path"
  mkdir -p "$(dirname "$destination_path")"
  cp "$source_path" "$destination_path"
done

echo "Published ${publishablePaths.length} static export file${publishablePaths.length === 1 ? "" : "s"} to $target_dir"
`;
}

function buildStaticSiteRsyncDeployScript(files: StaticSiteBundleFile[]): string {
  const publishablePaths = files
    .map((file) => file.path)
    .filter((path) => path !== "deploy.json" && path !== "deploy-local.sh" && path !== "deploy-rsync.sh")
    .sort();
  const quotedPaths = publishablePaths.map((path) => `"./${escapeShellDoubleQuoted(path)}"`).join(" ");

  return `#!/bin/sh
set -eu

if [ "$#" -ne 0 ]; then
  echo "Usage: STARLANE_STATIC_DEPLOY_TARGET=user@example.com:/var/www/site sh deploy-rsync.sh" >&2
  exit 64
fi

target="\${STARLANE_STATIC_DEPLOY_TARGET:-}"
remote_host="\${target%%:*}"
remote_path="\${target#*:}"

case "$target" in
  ""|-*|*[[:space:]]*)
    echo "STARLANE_STATIC_DEPLOY_TARGET must be a non-empty SSH destination without whitespace." >&2
    exit 64
    ;;
esac

case "$remote_host" in
  ""|*/*)
    echo "STARLANE_STATIC_DEPLOY_TARGET must use host:/absolute/publish-path." >&2
    exit 64
    ;;
esac

case "$remote_path" in
  /*) ;;
  *)
    echo "STARLANE_STATIC_DEPLOY_TARGET must use host:/absolute/publish-path." >&2
    exit 64
    ;;
esac

if ! command -v rsync >/dev/null 2>&1; then
  echo "rsync is required for deploy-rsync.sh." >&2
  exit 69
fi

for source_path in ${quotedPaths}; do
  if [ ! -f "$source_path" ]; then
    echo "Missing export file: $source_path" >&2
    exit 66
  fi
done

# Credentials and host verification are intentionally delegated to the caller's SSH agent/config.
# This bundle contains only static files and the destination name supplied at invocation time.
rsync --archive --compress --relative -- ${quotedPaths} "$target/"

echo "Published ${publishablePaths.length} static export file${publishablePaths.length === 1 ? "" : "s"} to $target"
`;
}

function escapeShellDoubleQuoted(value: string): string {
  return value.replace(/(["\\$`])/g, "\\$1");
}

function buildStaticSiteIntegrityManifest(input: {
  generatedAt: string;
  files: StaticSiteBundleFile[];
}): string {
  const encoder = new TextEncoder();
  return `${JSON.stringify(
    {
      schema_version: 1,
      generated_at: input.generatedAt,
      checksum_algorithm: "crc32",
      file_count: input.files.length,
      files: input.files.map((file) => {
        const bytes = encodeBundleContent(file.content, encoder);
        return {
          path: file.path,
          media_type: file.media_type,
          size: bytes.byteLength,
          checksum_crc32: crc32(bytes).toString(16).padStart(8, "0"),
        };
      }),
    },
    null,
    2,
  )}\n`;
}

function inferSiteTitle(snapshot: LocalVaultSnapshot): string {
  if (snapshot.spaces.length === 1) return snapshot.spaces[0].name;
  return "Starlane Notes Export";
}

function getStaticSiteThemeTokens(theme: StaticSiteTheme) {
  switch (theme) {
    case "dark":
      return {
        colorScheme: "dark",
        text: "#e6edf7",
        muted: "#95a3b8",
        line: "#2c3648",
        bg: "#111827",
        surface: "#172033",
        accent: "#60a5fa",
        codeBg: "#0b1120",
      };
    case "paper":
      return {
        colorScheme: "light",
        text: "#24211c",
        muted: "#746a5c",
        line: "#ded4c2",
        bg: "#fbf7ef",
        surface: "#fffdf8",
        accent: "#216869",
        codeBg: "#f1eadc",
      };
    case "light":
    default:
      return {
        colorScheme: "light",
        text: "#172033",
        muted: "#65708a",
        line: "#d8dee9",
        bg: "#fbfcfe",
        surface: "#ffffff",
        accent: "#2563eb",
        codeBg: "#f6f8fb",
      };
  }
}

function compareDocuments(snapshot: LocalVaultSnapshot) {
  const spaceById = new Map(snapshot.spaces.map((space) => [space.id, space.name]));
  return (a: VaultDocument, b: VaultDocument) => {
    const spaceCompare = (spaceById.get(a.space_id) ?? "").localeCompare(
      spaceById.get(b.space_id) ?? "",
    );
    if (spaceCompare !== 0) return spaceCompare;
    return a.title.localeCompare(b.title);
  };
}

function buildTagsByDocument(snapshot: LocalVaultSnapshot): Map<string, Array<{ name: string; color: string }>> {
  const tagById = new Map((snapshot.tags ?? []).map((tag) => [tag.id, tag]));
  const result = new Map<string, Array<{ name: string; color: string }>>();
  for (const docTag of snapshot.doc_tags ?? []) {
    const tag = tagById.get(docTag.tag_id);
    if (!tag) continue;
    result.set(docTag.document_id, [
      ...(result.get(docTag.document_id) ?? []),
      { name: tag.name, color: tag.color },
    ]);
  }
  return result;
}

function buildPropertiesByDocument(
  snapshot: LocalVaultSnapshot,
): Map<string, Array<{ name: string; value: string }>> {
  const schemaById = new Map((snapshot.property_schemas ?? []).map((schema) => [schema.id, schema]));
  const result = new Map<string, Array<{ name: string; value: string }>>();
  const values = [...(snapshot.property_values ?? [])].sort(comparePropertyValues(snapshot));
  for (const value of values) {
    if (!value.value) continue;
    const schema = schemaById.get(value.property_id);
    if (!schema) continue;
    result.set(value.document_id, [
      ...(result.get(value.document_id) ?? []),
      { name: schema.name, value: value.value },
    ]);
  }
  return result;
}

function comparePropertyValues(snapshot: LocalVaultSnapshot) {
  const schemaById = new Map((snapshot.property_schemas ?? []).map((schema) => [schema.id, schema]));
  return (a: VaultPropertyValue, b: VaultPropertyValue) =>
    (schemaById.get(a.property_id)?.sort_order ?? 0) -
    (schemaById.get(b.property_id)?.sort_order ?? 0);
}

function renderTag(tag: { name: string; color: string }): string {
  return `<span class="tag" style="border-color: ${escapeHtmlAttribute(tag.color)}">${escapeHtml(tag.name)}</span>`;
}

function renderProperty(property: { name: string; value: string }): string {
  return `<dt>${escapeHtml(property.name)}</dt><dd>${escapeHtml(property.value)}</dd>`;
}

function makeAnchorId(document: VaultDocument): string {
  return `${slugify(document.title) || "document"}-${slugify(document.id)}`;
}

function makeUniqueDocumentPath(document: VaultDocument, usedPaths: Set<string>): string {
  const base = `${slugify(document.title) || "document"}-${slugify(document.id) || "doc"}`;
  let path = `documents/${base}.html`;
  let suffix = 2;
  while (usedPaths.has(path)) {
    path = `documents/${base}-${suffix}.html`;
    suffix += 1;
  }
  usedPaths.add(path);
  return path;
}

function makeUniqueAssetPath(asset: VaultAsset, usedPaths: Set<string>): string {
  const extension = getAssetExtension(asset);
  const nameWithoutExtension = extension
    ? asset.name.replace(new RegExp(`${escapeRegExp(extension)}$`, "i"), "")
    : asset.name;
  const base = slugify(nameWithoutExtension) || slugify(asset.id) || "asset";
  let path = `assets/${base}${extension}`;
  let suffix = 2;
  while (usedPaths.has(path)) {
    path = `assets/${base}-${suffix}${extension}`;
    suffix += 1;
  }
  usedPaths.add(path);
  return path;
}

function getAssetExtension(asset: VaultAsset): string {
  const nameExtension = /\.[a-z0-9]{1,12}$/i.exec(asset.name.trim())?.[0]?.toLowerCase();
  if (nameExtension) return nameExtension;

  switch (asset.mime_type) {
    case "image/jpeg":
      return ".jpg";
    case "image/png":
      return ".png";
    case "image/gif":
      return ".gif";
    case "image/webp":
      return ".webp";
    case "image/svg+xml":
      return ".svg";
    case "application/pdf":
      return ".pdf";
    case "text/plain":
      return ".txt";
    default:
      return ".bin";
  }
}

function inferMediaTypeFromAssetName(name: string): string {
  const extension = /\.[a-z0-9]{1,12}$/i.exec(name.trim())?.[0]?.toLowerCase();
  switch (extension) {
    case ".jpg":
    case ".jpeg":
      return "image/jpeg";
    case ".png":
      return "image/png";
    case ".gif":
      return "image/gif";
    case ".webp":
      return "image/webp";
    case ".svg":
      return "image/svg+xml";
    case ".pdf":
      return "application/pdf";
    case ".txt":
      return "text/plain;charset=utf-8";
    default:
      return "application/octet-stream";
  }
}

function buildStaticSiteCss(themeTokens: ReturnType<typeof getStaticSiteThemeTokens>): string {
  return `:root { color-scheme: ${themeTokens.colorScheme}; --text: ${themeTokens.text}; --muted: ${themeTokens.muted}; --line: ${themeTokens.line}; --bg: ${themeTokens.bg}; --surface: ${themeTokens.surface}; --accent: ${themeTokens.accent}; --code-bg: ${themeTokens.codeBg}; }
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--text); font: 16px/1.55 Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
.shell { display: grid; grid-template-columns: minmax(14rem, 20rem) minmax(0, 1fr); min-height: 100vh; }
aside { border-right: 1px solid var(--line); background: var(--surface); padding: 2rem; position: sticky; top: 0; height: 100vh; overflow: auto; }
main { max-width: 56rem; width: 100%; padding: 2.5rem; }
.document-page { margin: 0 auto; }
h1 { font-size: 1.7rem; line-height: 1.2; margin: 0 0 .5rem; }
h2 { font-size: 1.55rem; line-height: 1.25; margin: 0; }
.breadcrumb, .generated, .site-description, .meta, .eyebrow, nav span { color: var(--muted); font-size: .82rem; }
.site-description { margin: .75rem 0 0; }
nav ul { list-style: none; margin: 1.5rem 0 0; padding: 0; display: grid; gap: .7rem; }
nav li { display: grid; gap: .1rem; }
.document { border-bottom: 1px solid var(--line); padding: 0 0 2.5rem; margin: 0 0 2.5rem; }
.document header { display: grid; gap: .5rem; margin-bottom: 1.25rem; }
.tags { display: flex; flex-wrap: wrap; gap: .4rem; }
.tag { border: 1px solid var(--line); border-radius: 999px; padding: .15rem .55rem; font-size: .78rem; }
.properties { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: .25rem .8rem; margin: .25rem 0 0; font-size: .85rem; }
.properties dt { color: var(--muted); }
.properties dd { margin: 0; }
.content :first-child { margin-top: 0; }
.content img { max-width: 100%; }
.content pre { overflow: auto; border: 1px solid var(--line); border-radius: .5rem; padding: 1rem; background: var(--code-bg); }
.content code { font-family: "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .9em; }
@media (max-width: 760px) {
  .shell { display: block; }
  aside { position: static; height: auto; border-right: 0; border-bottom: 1px solid var(--line); }
  main { padding: 1.5rem; }
}`;
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80);
}

function formatDate(value?: string): string {
  if (!value) return "unknown";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString("en", { year: "numeric", month: "short", day: "numeric" });
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function escapeHtmlAttribute(value: string): string {
  return escapeHtml(value).replace(/`/g, "&#96;");
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function encodeBundleContent(content: string | Uint8Array, encoder: TextEncoder): Uint8Array {
  return typeof content === "string" ? encoder.encode(content) : content;
}

function estimateBundleContentSize(content: string | Uint8Array): number {
  if (typeof content !== "string") return content.byteLength;
  return new TextEncoder().encode(content).byteLength;
}

function makeZipLocalHeader(nameBytes: Uint8Array, dataBytes: Uint8Array, crc: number): Uint8Array {
  const header = new Uint8Array(30);
  const view = new DataView(header.buffer);
  view.setUint32(0, 0x04034b50, true);
  view.setUint16(4, 20, true);
  view.setUint16(6, 0x0800, true);
  view.setUint16(8, 0, true);
  view.setUint16(10, 0, true);
  view.setUint16(12, 0, true);
  view.setUint32(14, crc, true);
  view.setUint32(18, dataBytes.length, true);
  view.setUint32(22, dataBytes.length, true);
  view.setUint16(26, nameBytes.length, true);
  view.setUint16(28, 0, true);
  return header;
}

function makeZipCentralHeader(entry: {
  nameBytes: Uint8Array;
  dataBytes: Uint8Array;
  crc: number;
  localOffset: number;
}): Uint8Array {
  const header = new Uint8Array(46);
  const view = new DataView(header.buffer);
  view.setUint32(0, 0x02014b50, true);
  view.setUint16(4, 20, true);
  view.setUint16(6, 20, true);
  view.setUint16(8, 0x0800, true);
  view.setUint16(10, 0, true);
  view.setUint16(12, 0, true);
  view.setUint16(14, 0, true);
  view.setUint32(16, entry.crc, true);
  view.setUint32(20, entry.dataBytes.length, true);
  view.setUint32(24, entry.dataBytes.length, true);
  view.setUint16(28, entry.nameBytes.length, true);
  view.setUint16(30, 0, true);
  view.setUint16(32, 0, true);
  view.setUint16(34, 0, true);
  view.setUint16(36, 0, true);
  view.setUint32(38, 0, true);
  view.setUint32(42, entry.localOffset, true);
  return header;
}

function makeZipEndRecord(entryCount: number, centralSize: number, centralOffset: number): Uint8Array {
  const record = new Uint8Array(22);
  const view = new DataView(record.buffer);
  view.setUint32(0, 0x06054b50, true);
  view.setUint16(4, 0, true);
  view.setUint16(6, 0, true);
  view.setUint16(8, entryCount, true);
  view.setUint16(10, entryCount, true);
  view.setUint32(12, centralSize, true);
  view.setUint32(16, centralOffset, true);
  view.setUint16(20, 0, true);
  return record;
}

function concatBytes(chunks: Uint8Array[], totalLength: number): Uint8Array {
  const result = new Uint8Array(totalLength);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result;
}

const CRC32_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i += 1) {
    let value = i;
    for (let bit = 0; bit < 8; bit += 1) {
      value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
    }
    table[i] = value >>> 0;
  }
  return table;
})();

function crc32(bytes: Uint8Array): number {
  let value = 0xffffffff;
  for (const byte of bytes) {
    value = CRC32_TABLE[(value ^ byte) & 0xff] ^ (value >>> 8);
  }
  return (value ^ 0xffffffff) >>> 0;
}
