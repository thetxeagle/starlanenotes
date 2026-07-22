import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import test from "node:test";

const scriptPath = fileURLToPath(new URL("./sync-gitea-wiki.mjs", import.meta.url));

function withFixture(files, callback) {
  const root = mkdtempSync(join(tmpdir(), "starlane-wiki-render-"));
  try {
    const docsRoot = join(root, "docs");
    const wikiOutput = join(root, "wiki");
    for (const [path, contents] of Object.entries(files)) {
      const fullPath = join(docsRoot, path);
      mkdirSync(dirname(fullPath), { recursive: true });
      writeFileSync(fullPath, contents, "utf8");
    }
    callback({ docsRoot, wikiOutput });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("renders markdown and assets while skipping hidden OS metadata files", () => {
  withFixture(
    {
      "index.md": "# Home\n\n[Guide](guide.md)\n",
      "guide.md": "# Guide\n",
      "images/logo.txt": "logo",
      ".DS_Store": "finder metadata",
      "images/.DS_Store": "finder metadata",
      "images/Thumbs.db": "windows metadata",
      "images/.hidden.txt": "hidden asset",
    },
    ({ docsRoot, wikiOutput }) => {
      execFileSync(process.execPath, [scriptPath], {
        env: {
          ...process.env,
          DOCS_SOURCE_DIR: docsRoot,
          WIKI_OUTPUT_DIR: wikiOutput,
        },
        encoding: "utf8",
      });

      assert.equal(existsSync(join(wikiOutput, "Home.md")), true);
      assert.equal(existsSync(join(wikiOutput, "Guide.md")), true);
      assert.equal(existsSync(join(wikiOutput, "images/logo.txt")), true);
      assert.equal(existsSync(join(wikiOutput, ".DS_Store")), false);
      assert.equal(existsSync(join(wikiOutput, "images/.DS_Store")), false);
      assert.equal(existsSync(join(wikiOutput, "images/Thumbs.db")), false);
      assert.equal(existsSync(join(wikiOutput, "images/.hidden.txt")), false);
    },
  );
});
