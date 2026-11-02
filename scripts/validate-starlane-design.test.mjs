import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
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

test("requires the canonical Starlane governance artifacts in governed repositories", () => {
  withFixture(
    {
      "AGENTS.md": "Use repo-docs/guidelines/starlane-interface-craft.md and a documented, verified exception.",
      "apps/web/src/Example.tsx": "export function Example() { return <main />; }",
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 8);
      assert.ok(
        failures.every((failure) => /design-governance artifact is missing/.test(failure.message)),
      );
      assert.ok(
        failures.some(
          (failure) =>
            failure.file === ".codex/skills/starlane-interface-design/SKILL.md",
        ),
      );
    },
  );
});

test("accepts a complete Starlane governance contract", () => {
  withFixture(
    {
      "AGENTS.md": "Use repo-docs/guidelines/starlane-interface-craft.md and a documented, verified exception.",
      "CODEX.md": "Use starlane-interface-design and starlane-interface-review.",
      "repo-docs/guidelines/starlane-suite-interface-contract.md": "suite",
      "repo-docs/guidelines/starlane-brand-baseline.md": "brand",
      "repo-docs/guidelines/starlane-design-system.md": "system",
      "repo-docs/guidelines/starlane-interface-craft.md": "craft",
      "repo-docs/guidelines/starlane-color-reference.md": "color",
      "repo-docs/guidelines/starlane-icon-playbook.md": "icon",
      ".codex/skills/starlane-interface-design/SKILL.md":
        "name: starlane-interface-design\nRead references/contract-map.md",
      ".codex/skills/starlane-interface-review/SKILL.md":
        "name: starlane-interface-review\nRead references/review-matrix.md",
      "apps/web/src/Example.tsx": "export function Example() { return <main />; }",
    },
    (repoRoot) => assert.deepEqual(validateStarlaneDesign({ repoRoot }), []),
  );
});

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

test("requires the shared Select primitive to avoid native browser rendering", () => {
  withFixture(
    {
      "apps/web/src/components/ui/input.tsx": `
        export function Input() {
          return <input />;
        }
      `,
      "apps/web/src/components/ui/select.tsx": `
        export function Select() { return <select />; }
      `,
      "apps/web/src/components/ui/menu-select.tsx": `
        export function MenuSelect() { return <div />; }
      `,
      "apps/web/src/components/ui/textarea.tsx": `
        export function Textarea() {
          return <textarea />;
        }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 1);
      assert.match(failures[0].message, /canonical Select primitive/);
    },
  );
});

test("accepts the anchored Select and keyboard menu contract", () => {
  withFixture(
    {
      "apps/web/src/components/ui/select.tsx": `
        import { MenuSelect } from "./menu-select";
        export function Select() {
          const emitChange = () => null;
          return <span data-starlane-select-control="true"><MenuSelect aria-label="Select option" /></span>;
        }
      `,
      "apps/web/src/components/ui/menu-select.tsx": `
        import { PopoverContent } from "./popover";
        export function MenuSelect() {
          return <PopoverContent role="listbox" aria-selected={false} onKeyDown={(event) => {
            if (event.key === "ArrowDown" || event.key === "ArrowUp") return;
            if (event.key === "Home" || event.key === "End") return;
          }} onClick={() => setMenuOpen(false)} />;
        }
      `,
    },
    (repoRoot) => assert.deepEqual(validateStarlaneDesign({ repoRoot }), []),
  );
});

test("rejects visually unstyled code-block metadata and edit-mode Copy", () => {
  withFixture(
    {
      "apps/web/src/components/editor/CodeBlockNodeView.tsx": `
        import { Input } from "@/components/ui/input";
        import { Select } from "@/components/ui/select";

        export function CodeBlockNodeView() {
          return (
            <div>
              <Select className="h-7 text-xs" aria-label="Code language" />
              <Input className="h-7 text-xs" aria-label="Code block name" />
              <button aria-label={\`Copy plain text code\`}>Copy</button>
            </div>
          );
        }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 1);
      assert.match(failures[0].message, /programmatically named, fully styled/);
    },
  );
});

test("accepts tokenized code-block language menus with Copy reserved for read-only view", () => {
  withFixture(
    {
      "apps/web/src/components/editor/CodeBlockNodeView.tsx": `
        import { Input } from "@/components/ui/input";
        import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

        export function CodeBlockNodeView() {
          return (
            <div>
              <div data-code-block-edit-controls="true">
                <div className="code-block-metadata-field">
                  <Popover>
                    <PopoverTrigger>
                      <button
                        aria-label="Code language"
                        aria-haspopup="menu"
                        className="border-border bg-surface-raised text-text"
                      />
                    </PopoverTrigger>
                    <PopoverContent aria-label="Code languages">
                      <button
                        role="menuitemradio"
                        className="bg-primary/15 text-primary"
                      />
                    </PopoverContent>
                  </Popover>
                </div>
                <div className="code-block-metadata-field">
                  <Input
                    className="bg-surface-raised h-10 min-w-0 pl-9 text-sm"
                    aria-label="Code block name"
                  />
                </div>
              </div>
              ) : (
              <button aria-label={\`Copy plain text code\`}>Copy</button>
            </div>
          );
        }
      `,
    },
    (repoRoot) => {
      assert.deepEqual(validateStarlaneDesign({ repoRoot }), []);
    },
  );
});

test("rejects toolbar, tooltip, and button-group owners that diverge from shared control chrome", () => {
  withFixture(
    {
      "apps/web/src/components/ui/control-chrome.ts": `
        export const CONTROL_CHROME_SURFACE_CLASS = "bg-bg";
      `,
      "apps/web/src/components/ui/tooltip.tsx": `
        export function Tooltip() { return <div className="bg-surface-raised" />; }
      `,
      "apps/web/src/components/editor/Editor.tsx": `
        export function Editor() { return <div role="toolbar" />; }
      `,
      "apps/web/src/components/layout/Sidebar.tsx": `
        export function Sidebar() { return <div role="group" />; }
      `,
      "apps/web/src/pages/DocumentPage.tsx": `
        export function DocumentPage() { return <div role="group" />; }
      `,
      "apps/web/src/components/ThemeModeSwitch.tsx": `
        export function ThemeModeSwitch() { return <div role="group" />; }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 2);
      assert.match(failures[0].message, /shared Starlane control-chrome/);
      assert.match(failures[1].message, /Editor toolbars must share/);
    },
  );
});

test("requires the shared touch-action recipe in mobile editor chrome", () => {
  const validator = readFileSync(
    resolve(import.meta.dirname, "validate-starlane-design.mjs"),
    "utf8",
  );

  assert.match(validator, /CONTROL_CHROME_TOUCH_ACTION_CLASS/);
  assert.match(
    validator,
    /MobileEditorToolbar\.tsx"[\s\S]*CONTROL_CHROME_TOUCH_ACTION_CLASS/,
  );
});

test("rejects divergent editor toolbars that bypass the contextual toolbar contract", () => {
  withFixture(
    {
      "apps/web/src/components/editor/Editor.tsx": `
        export const TABLE_EDITOR_TOOLBAR_SURFACE_CLASS = "one-off-table-toolbar";
        export function Editor() {
          return <div role="toolbar" aria-label="Text formatting" />;
        }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 1);
      assert.match(failures[0].message, /Editor toolbars must share/);
    },
  );
});

test("rejects browser-native dialog APIs in application source", () => {
  withFixture(
    {
      "apps/web/src/BadDialog.tsx": `
        export function BadDialog() {
          window.confirm("Delete this page?");
          return null;
        }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 1);
      assert.match(failures[0].message, /Browser-native/);
    },
  );
});

test("rejects native HTML dialog elements in application source", () => {
  withFixture(
    {
      "apps/web/src/BadNativeDialog.tsx": `
        export function BadNativeDialog() {
          return <dialog open>Unsanctioned native dialog</dialog>;
        }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 1);
      assert.match(failures[0].message, /Native HTML dialog elements/);
    },
  );
});

test("rejects hand-rolled application dialogs outside approved shared overlays", () => {
  withFixture(
    {
      "apps/web/src/BadDialog.tsx": `
        export function BadDialog() {
          return <div role="dialog">Unsanctioned overlay</div>;
        }
      `,
      "apps/web/src/BadAlertDialog.tsx": `
        export function BadAlertDialog() {
          return <div role="alertdialog">Unsanctioned destructive overlay</div>;
        }
      `,
      "apps/web/src/components/NotificationBell.tsx": `
        export function NotificationBell() {
          return <section role="dialog">Notifications</section>;
        }
      `,
      "apps/web/src/OverlayOwner.tsx": `
        export function OverlayOwner(target) {
          return target.closest("[role='dialog']");
        }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 2);
      assert.ok(
        failures.every((failure) => /Hand-rolled application dialogs/.test(failure.message)),
      );
    },
  );
});

test("requires application menus to be named and directionally keyboard-navigable", () => {
  withFixture(
    {
      "apps/web/src/BadMenu.tsx": `
        export function BadMenu() {
          return <div role="menu"><button role="menuitem">Open</button></div>;
        }
      `,
      "apps/web/src/UnrelatedLabelMenu.tsx": `
        export function UnrelatedLabelMenu() {
          const onKeyDown = (event) => ["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key);
          return <><p aria-label="Not the menu">Label</p><div role="menu" onKeyDown={onKeyDown} /></>;
        }
      `,
      "apps/web/src/GoodMenu.tsx": `
        import { useMenuKeyboardNavigation } from "./components/ui/useMenuKeyboardNavigation";
        export function GoodMenu() {
          const onKeyDown = useMenuKeyboardNavigation(null);
          return <div role="menu" aria-label="Actions" onKeyDown={onKeyDown}><button role="menuitem">Open</button></div>;
        }
      `,
      "apps/web/src/components/ui/mobile-action-sheet.tsx": `
        import { useMenuKeyboardNavigation } from "./useMenuKeyboardNavigation";
        export function MobileActionSheet() {
          const onKeyDown = useMenuKeyboardNavigation(null);
          return <div role="menu" aria-label="Actions" onKeyDown={onKeyDown} />;
        }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 3);
      assert.match(failures[0].message, /accessible name/);
      assert.match(failures[1].message, /directional keyboard navigation/);
      assert.match(failures[2].message, /accessible name/);
    },
  );
});

test("rejects caller-owned menu geometry outside the documented selection exception", () => {
  withFixture(
    {
      "apps/web/src/BadPositionedMenu.tsx": `
        export function BadPositionedMenu() {
          const menuPosition = anchor.getBoundingClientRect();
          const onKeyDown = useMenuKeyboardNavigation(null);
          return <div role="menu" aria-label="Actions" style={{ top: menuPosition.bottom, left: menuPosition.left }} onKeyDown={onKeyDown} />;
        }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 1);
      assert.match(failures[0].message, /caller-owned viewport geometry/);
    },
  );
});

test("rejects decorative uppercase letter-spaced eyebrow labels", () => {
  withFixture(
    {
      "apps/web/src/BadEyebrow.tsx": `
        export function BadEyebrow() {
          return <div className="text-[10px] uppercase tracking-wide">Section</div>;
        }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 1);
      assert.match(failures[0].message, /eyebrow labels/);
    },
  );
});

test("requires the global primary-token keyboard focus contract when app styles exist", () => {
  withFixture(
    {
      "apps/web/src/styles/globals.css": "button { color: inherit; }",
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 1);
      assert.match(failures[0].message, /focus-visible ring/);
    },
  );
});

test("rejects a permanent shared focus ring on unfocused controls", () => {
  withFixture(
    {
      "apps/web/src/styles/globals.css": `
        button:focus-visible {
          @apply ring-primary ring-offset-bg outline-none ring-2;
        }

        .focus-ring {
          @apply ring-primary ring-offset-bg outline-none ring-2;
        }
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 1);
      assert.match(failures[0].message, /without a permanent ring/);
    },
  );
});

test("requires the self-hosted Font Trio Product typography contract", () => {
  withFixture(
    {
      "apps/web/src/styles/globals.css": `
        button:focus-visible {
          @apply ring-primary ring-offset-bg outline-none;
        }
      `,
      "apps/web/tailwind.config.ts": `
        export default { theme: { extend: { fontFamily: { sans: ["system-ui"] } } } };
      `,
    },
    (repoRoot) => {
      const failures = validateStarlaneDesign({ repoRoot });
      assert.equal(failures.length, 1);
      assert.match(failures[0].message, /Font Trio Product/);
    },
  );
});

test("public contributor mode requires its kit and still rejects application design drift", () => {
  const files = {
    "AGENTS.md": "Public agent guidance",
    "CONTRIBUTING.md": "Contribution guide",
    "contributor-kit/reference/interface-contract.md": "Interface contract",
    ".agents/skills/starlane-interface-design/SKILL.md": "Design skill",
    ".agents/skills/starlane-interface-review/SKILL.md": "Review skill",
    "apps/web/src/Example.tsx": "export function Example() { return <main />; }",
  };
  withFixture(files, (repoRoot) => {
    assert.deepEqual(validateStarlaneDesign({ repoRoot, publicContributor: true }), []);
    writeFileSync(join(repoRoot, "apps/web/src/Example.tsx"), 'window.alert("bad");');
    assert.ok(validateStarlaneDesign({ repoRoot, publicContributor: true })
      .some((failure) => /Browser-native/.test(failure.message)));
    rmSync(join(repoRoot, "contributor-kit/reference/interface-contract.md"));
    assert.ok(validateStarlaneDesign({ repoRoot, publicContributor: true })
      .some((failure) => /Public contributor contract is missing/.test(failure.message)));
    assert.ok(validateStarlaneDesign({ repoRoot })
      .some((failure) => /canonical Starlane design-governance artifact is missing/.test(failure.message)));
  });
});
