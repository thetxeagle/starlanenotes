import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";
import { validateStarlaneDesign } from "./validate-starlane-design.mjs";

function withFixture(files, callback) {
  const root = mkdtempSync(join(tmpdir(), "starlane-design-"));
  try {
    for (const [path, contents] of Object.entries(files)) {
      const fullPath = join(root, path);
      mkdirSync(dirname(fullPath), { recursive: true });
      writeFileSync(fullPath, contents, "utf8");
    }
    callback(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("rejects raw text-like form controls outside shared UI primitives", () => {
  withFixture(
    {
      "apps/web/src/BadControls.tsx": `
        export function BadControls() {
          return (
            <>
              <input aria-label="Name" />
              <input type="email" />
              <select aria-label="Role" />
              <textarea aria-label="Notes" />
            </>
          );
        }
      `,
      "apps/web/src/AllowedControls.tsx": `
        export function AllowedControls() {
          return (
            <>
              <input type="checkbox" />
              <input type="file" />
              <input type="color" />
            </>
          );
        }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 4);
      assert.deepEqual(
        failures.map((failure) => failure.message),
        [
          "Raw text-like inputs are banned in app UI; use the shared Input or SearchInput component.",
          "Raw text-like inputs are banned in app UI; use the shared Input or SearchInput component.",
          "Raw selects are banned in app UI; use the shared Select component.",
          "Raw textareas are banned in app UI; use the shared Textarea component.",
        ],
      );
    },
  );
});

test("ignores shared UI primitive source files", () => {
  withFixture(
    {
      "apps/web/src/components/ui/input.tsx": `
        export function Input() {
          return <input />;
        }
      `,
      "apps/web/src/components/ui/select.tsx": `
        export function Select() {
          return <select />;
        }
      `,
      "apps/web/src/components/ui/textarea.tsx": `
        export function Textarea() {
          return <textarea />;
        }
      `,
    },
    (repoRoot) => {
      assert.deepEqual(validateStarlaneDesign({ repoRoot }), []);
    },
  );
});
