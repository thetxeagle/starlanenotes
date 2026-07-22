import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import test from "node:test";

const repoRoot = resolve(new URL("../..", import.meta.url).pathname);

function readRepoFile(path) {
  return readFileSync(join(repoRoot, path), "utf8");
}

function extractRequiredDocs() {
  const validatorSource = readRepoFile("scripts/docs/validate-docs-completeness.mjs");
  const requiredDocsBlock = validatorSource.match(/const requiredDocs = \[([\s\S]*?)\];/);

  assert.ok(requiredDocsBlock, "validate-docs-completeness.mjs should define requiredDocs");

  return [...requiredDocsBlock[1].matchAll(/"([^"]+)"/g)].map((match) => match[1]);
}

test("docs index links every required docs page", () => {
  const docsIndex = readRepoFile("docs/docs/index.md");
  const missingLinks = extractRequiredDocs().filter((doc) => doc !== "index.md" && !docsIndex.includes(doc));

  assert.deepEqual(missingLinks, []);
});
