import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const sourceRoot = path.join(repoRoot, "docs", "docs");
const targetRoot = path.join(repoRoot, "apps", "docs", "src", "content", "docs");
const astroContentCache = path.join(repoRoot, "apps", "docs", "node_modules", ".astro", "data-store.json");

function titleFromMarkdown(markdown, fallback) {
  const heading = markdown.match(/^#\s+(.+)$/m)?.[1]?.trim();
  return heading || fallback.replace(/[-_]+/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function withFrontmatter(markdown, fallback) {
  const normalizedCodeFences = markdown.replace(/^```(?:env|netrc|caddy)\s*$/gm, "```text");
  if (normalizedCodeFences.startsWith("---\n")) {
    return normalizedCodeFences.replace(/^---\n([\s\S]*?)\n---/, (_match, frontmatter) => {
      const additions = [];
      if (!/^title:/m.test(frontmatter)) {
        additions.push(`title: ${JSON.stringify(titleFromMarkdown(normalizedCodeFences, fallback))}`);
      }
      // Starlight excludes any production page whose resolved draft value is not explicitly false.
      if (!/^draft:/m.test(frontmatter)) additions.push("draft: false");
      return `---\n${frontmatter}${additions.length ? `\n${additions.join("\n")}` : ""}\n---`;
    });
  }
  return `---\ntitle: ${JSON.stringify(titleFromMarkdown(normalizedCodeFences, fallback))}\ndraft: false\n---\n\n${normalizedCodeFences}`;
}

async function syncDirectory(source, target) {
  await mkdir(target, { recursive: true });
  for (const entry of await readdir(source, { withFileTypes: true })) {
    if (entry.name.startsWith(".")) continue;
    const sourcePath = path.join(source, entry.name);
    const targetPath = path.join(target, entry.name);
    if (entry.isDirectory()) {
      await syncDirectory(sourcePath, targetPath);
      continue;
    }
    if (!entry.isFile()) continue;
    if (entry.name.endsWith(".md")) {
      const markdown = await readFile(sourcePath, "utf8");
      await writeFile(targetPath, withFrontmatter(markdown, path.basename(entry.name, ".md")));
    } else {
      await cp(sourcePath, targetPath);
    }
  }
}

await rm(targetRoot, { recursive: true, force: true });
// The loader cache persists between workspace builds. Clear it before replacing the generated tree
// so Astro does not merge stale and fresh entries with duplicate IDs.
await rm(astroContentCache, { force: true });
await syncDirectory(sourceRoot, targetRoot);
console.log(`Synced Starlight content from ${path.relative(repoRoot, sourceRoot)}.`);
