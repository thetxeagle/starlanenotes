#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync, cpSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const docsSource = resolve(repoRoot, process.env.DOCS_SOURCE_DIR || "docs/docs");
const wikiOutput = resolve(repoRoot, process.env.WIKI_OUTPUT_DIR || "artifacts/gitea-wiki");
const shouldPush = process.argv.includes("--push");
const ignoredFileNames = new Set([".DS_Store", "Thumbs.db"]);

function run(command, args, options = {}) {
  const output = execFileSync(command, args, {
    cwd: options.cwd || repoRoot,
    encoding: "utf8",
    stdio: options.stdio || ["ignore", "pipe", "pipe"],
  });
  return typeof output === "string" ? output.trim() : "";
}

function listFiles(dir) {
  const entries = readdirSync(dir, { withFileTypes: true });
  return entries.flatMap((entry) => {
    if (entry.name.startsWith(".") || ignoredFileNames.has(entry.name)) return [];
    const fullPath = join(dir, entry.name);
    if (entry.isDirectory()) return listFiles(fullPath);
    return [fullPath];
  });
}

function titleize(value) {
  return value
    .replace(/\.md$/i, "")
    .split(/[-_]/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join("-");
}

function pageNameFor(relativePath) {
  if (relativePath === "index.md") return "Home";
  return relativePath
    .split("/")
    .map(titleize)
    .join("-");
}

function pageTitle(markdown, relativePath) {
  const h1 = markdown.match(/^#\s+(.+)$/m);
  if (h1) return h1[1].trim();
  return pageNameFor(relativePath).replaceAll("-", " ");
}

function isExternalLink(target) {
  return /^(?:[a-z][a-z0-9+.-]*:|#)/i.test(target);
}

function normalizeDocTarget(currentRelativePath, target) {
  const [targetPath, anchor = ""] = target.split("#");
  const normalized = path.posix.normalize(path.posix.join(path.posix.dirname(currentRelativePath), targetPath));
  return { normalized, anchor };
}

function convertMarkdownLinks(markdown, currentRelativePath, pageMap) {
  return markdown.replace(/(!?\[[^\]]*]\()([^)\s]+\.md(?:#[^)]+)?)(\))/g, (match, prefix, target, suffix) => {
    if (prefix.startsWith("!")) return match;
    if (isExternalLink(target)) return match;

    const { normalized, anchor } = normalizeDocTarget(currentRelativePath, target);
    const pageName = pageMap.get(normalized);
    if (!pageName) return match;

    return `${prefix}${pageName}${anchor ? `#${anchor}` : ""}${suffix}`;
  });
}

function copyAssetFiles(files) {
  for (const file of files) {
    const rel = path.posix.normalize(relative(docsSource, file).split(path.sep).join("/"));
    if (rel.endsWith(".md")) continue;
    const target = join(wikiOutput, rel);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(file, target);
  }
}

function renderWiki() {
  if (!existsSync(docsSource)) {
    throw new Error(`Docs source directory does not exist: ${docsSource}`);
  }

  const allFiles = listFiles(docsSource);
  const markdownFiles = allFiles
    .filter((file) => file.endsWith(".md"))
    .map((file) => path.posix.normalize(relative(docsSource, file).split(path.sep).join("/")))
    .sort();
  const pageMap = new Map(markdownFiles.map((file) => [file, pageNameFor(file)]));
  const seenNames = new Map();

  rmSync(wikiOutput, { recursive: true, force: true });
  mkdirSync(wikiOutput, { recursive: true });

  const pages = [];
  for (const relativePath of markdownFiles) {
    const sourcePath = join(docsSource, relativePath);
    const raw = readFileSync(sourcePath, "utf8");
    const pageName = pageMap.get(relativePath);
    const outputName = `${pageName}.md`;

    if (seenNames.has(outputName)) {
      throw new Error(`Wiki page collision: ${relativePath} and ${seenNames.get(outputName)} both render to ${outputName}`);
    }
    seenNames.set(outputName, relativePath);

    const rendered = [
      "<!-- Generated from repository docs. Edit the source file, not this wiki page. -->",
      "",
      convertMarkdownLinks(raw, relativePath, pageMap).trimEnd(),
      "",
    ].join("\n");
    writeFileSync(join(wikiOutput, outputName), rendered);
    pages.push({ title: pageTitle(raw, relativePath), relativePath, pageName });
  }

  copyAssetFiles(allFiles);
  writeFileSync(join(wikiOutput, "_Sidebar.md"), renderSidebar(pages));
  return pages.length;
}

function renderSidebar(pages) {
  const lines = [
    "<!-- Generated from repository docs. Edit the source files, not this wiki page. -->",
    "",
    "# Starlane Notes",
    "",
  ];

  for (const page of pages) {
    if (page.relativePath === "index.md") continue;
    lines.push(`- [${page.title}](${page.pageName})`);
  }

  lines.push("");
  return lines.join("\n");
}

function wikiRemoteUrl() {
  const explicit = process.env.WIKI_REMOTE_URL;
  if (explicit) return explicit;

  const origin = run("git", ["config", "--get", "remote.origin.url"]);
  if (origin.endsWith(".wiki.git")) return origin;
  if (origin.endsWith(".git")) return origin.replace(/\.git$/, ".wiki.git");
  return `${origin}.wiki.git`;
}

function pushWiki() {
  const remote = wikiRemoteUrl();
  const workDir = join(tmpdir(), `starlane-notes-wiki-${Date.now()}`);
  const renderedFiles = readdirSync(wikiOutput);

  rmSync(workDir, { recursive: true, force: true });
  try {
    run("git", ["clone", remote, workDir], { stdio: "inherit" });
  } catch (error) {
    console.error(`Failed to clone Gitea wiki remote: ${remote}`);
    console.error("Confirm the repository wiki is enabled/initialized and the wiki credentials can write to it.");
    throw error;
  }
  run("git", ["config", "user.name", "Starlane Notes Wiki Sync"], { cwd: workDir });
  run("git", ["config", "user.email", "starlane-notes-wiki-sync@users.noreply.gitea.2eagles.xyz"], { cwd: workDir });

  for (const entry of readdirSync(workDir)) {
    if (entry === ".git") continue;
    rmSync(join(workDir, entry), { recursive: true, force: true });
  }

  for (const entry of renderedFiles) {
    cpSync(join(wikiOutput, entry), join(workDir, entry), { recursive: true });
  }

  run("git", ["add", "-A"], { cwd: workDir, stdio: "inherit" });
  const status = run("git", ["status", "--short"], { cwd: workDir });
  if (!status) {
    console.log("Gitea wiki is already up to date.");
    return;
  }

  run("git", ["commit", "-m", "docs: sync repository docs to wiki"], { cwd: workDir, stdio: "inherit" });
  run("git", ["push", "origin", "HEAD"], { cwd: workDir, stdio: "inherit" });
}

const pageCount = renderWiki();
console.log(`Rendered ${pageCount} wiki pages to ${relative(repoRoot, wikiOutput)}.`);

if (shouldPush) {
  pushWiki();
}
