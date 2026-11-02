#!/usr/bin/env node
import { pathToFileURL } from "node:url";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const defaultRepoRoot = new URL("..", import.meta.url).pathname;
const defaultScanRoots = ["apps/web/src"];
const fileExtensions = new Set([".css", ".ts", ".tsx"]);

const bannedPatterns = [
  {
    pattern: /\bwindow\.(?:alert|confirm|prompt)\s*\(/,
    message:
      "Browser-native alert, confirm, and prompt dialogs are banned; use DialogShell, ConfirmDialog, or an approved shared dialog primitive.",
  },
  {
    pattern: /<dialog\b/,
    message:
      "Native HTML dialog elements are banned in app UI; use DialogShell, ConfirmDialog, or an approved shared dialog primitive.",
  },
  {
    pattern: /\bbackdrop-blur(?:-[\w[\]/.]+)?\b/,
    message: "Glassmorphism/backdrop blur is banned by the Starlane brand baseline.",
  },
  {
    pattern: /\bbg-gradient-|linear-gradient\(|radial-gradient\(|conic-gradient\(/,
    message: "Decorative gradients are banned in app UI.",
  },
  {
    pattern:
      /\buppercase\b.*\btracking-(?:wide|wider|widest)\b|\btracking-(?:wide|wider|widest)\b.*\buppercase\b/,
    message:
      "Decorative uppercase/letter-spaced eyebrow labels are banned; use ordinary readable Starlane UI text.",
  },
  {
    pattern: /\bshadow-(?:lg|xl|2xl|inner)\b|drop-shadow/,
    message: "Use Starlane shadow tokens only; large/drop shadows are banned.",
  },
  {
    pattern: /\bbg-background\b/,
    message:
      "`bg-background` is not part of the Starlane token set; use `bg-bg`, `bg-surface`, or `bg-surface-raised`.",
  },
  {
    pattern:
      /(?:^|[\s"'`])(?:[a-z-]+:)*(?:text|bg|border|ring|outline|decoration|accent|caret|fill|stroke)-(?:red|green|amber|emerald|sky|purple)-(?:50|100|200|300|400|500|600|700|800|900|950)(?:\/\d+)?\b/,
    message:
      "Hardcoded Tailwind status color ramps are banned; use Starlane semantic tokens such as `danger`, `success`, `warning`, `accent`, `primary`, or `ai`.",
  },
];

const ignoredPathFragments = [
  "/node_modules/",
  "/dist/",
  "/build/",
  "/coverage/",
  "/components/ui/",
  ".test.",
  ".spec.",
];

// These surfaces are not centered form dialogs: each has an intentional, tested
// interaction owner (drawer, sheet, notification inbox, or anchored palette).
// Every other application dialog must compose through DialogShell/ConfirmDialog.
const approvedSpecializedDialogPaths = new Set([
  "apps/web/src/components/NotificationBell.tsx",
  "apps/web/src/components/SearchModal.tsx",
  "apps/web/src/components/editor/SlashCommands.tsx",
  "apps/web/src/components/layout/MobileDocumentPanel.tsx",
  "apps/web/src/components/layout/MobileNavigationDrawer.tsx",
]);

// These selection-owned link previews still need a virtual anchor because their trigger is an
// arbitrary inline DOM link. Keep the exception explicit until they can use a real Radix trigger
// without moving the user's selection. New menu surfaces may not join this list without a focused
// migration note.
const approvedManualMenuGeometryPaths = new Map([
  [
    "apps/web/src/components/editor/Editor.tsx",
    "selection-owned link preview",
  ],
  [
    "apps/web/src/components/editor/MarkdownPreview.tsx",
    "selection-owned link preview",
  ],
]);

const allowedRawInputTypes = new Set(["checkbox", "color", "file", "hidden", "radio", "range"]);

const governanceArtifacts = [
  "repo-docs/guidelines/starlane-suite-interface-contract.md",
  "repo-docs/guidelines/starlane-brand-baseline.md",
  "repo-docs/guidelines/starlane-design-system.md",
  "repo-docs/guidelines/starlane-interface-craft.md",
  "repo-docs/guidelines/starlane-color-reference.md",
  "repo-docs/guidelines/starlane-icon-playbook.md",
  ".codex/skills/starlane-interface-design/SKILL.md",
  ".codex/skills/starlane-interface-review/SKILL.md",
];

function walk(dir) {
  const entries = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      entries.push(...walk(path));
    } else {
      entries.push(path);
    }
  }
  return entries;
}

function extensionOf(path) {
  const match = path.match(/\.[^.]+$/);
  return match ? match[0] : "";
}

function validateRawFormControls(rel, lines, failures) {
  lines.forEach((line, index) => {
    if (/^\s*<textarea\b/.test(line)) {
      failures.push({
        file: rel,
        line: index + 1,
        message: "Raw textareas are banned in app UI; use the shared Textarea component.",
        text: line.trim(),
      });
    }

    if (/^\s*<select\b/.test(line)) {
      failures.push({
        file: rel,
        line: index + 1,
        message: "Raw selects are banned in app UI; use the shared Select component.",
        text: line.trim(),
      });
    }

    if (!/^\s*<input\b/.test(line)) return;
    const controlSnippet = lines.slice(index, index + 6).join(" ");
    const typeMatch = controlSnippet.match(/\btype=(?:"([^"]+)"|'([^']+)'|\{?"([^"}]+)"\}?)/);
    const inputType = (typeMatch?.[1] ?? typeMatch?.[2] ?? typeMatch?.[3] ?? "text").toLowerCase();
    if (allowedRawInputTypes.has(inputType)) return;

    failures.push({
      file: rel,
      line: index + 1,
      message:
        "Raw text-like inputs are banned in app UI; use the shared Input or SearchInput component.",
      text: line.trim(),
    });
  });
}

function validateDialogOwnership(rel, contents, failures) {
  const dialogLine = contents
    .split(/\r?\n/)
    .findIndex(
      (line) =>
        /\brole=["'](?:alert)?dialog["']/.test(line) &&
        !line.includes("closest(") &&
        !line.includes("querySelector("),
    );
  if (dialogLine < 0) return;
  if (approvedSpecializedDialogPaths.has(rel)) return;

  failures.push({
    file: rel,
    line: dialogLine + 1,
    message:
      "Hand-rolled application dialogs are banned; use DialogShell or ConfirmDialog unless this is an approved specialized overlay with its own tested focus lifecycle.",
    text: "Use the shared Starlane dialog primitives.",
  });
}

function validateMenuContract(rel, contents, failures) {
  const menuMatches = [...contents.matchAll(/\brole=["']menu["']/g)];
  if (menuMatches.length === 0) return;

  for (const match of menuMatches) {
    const roleIndex = match.index ?? 0;
    const openingTagStart = contents.lastIndexOf("<", roleIndex);
    const openingTagEnd = contents.indexOf(">", roleIndex);
    const openingTag =
      openingTagStart >= 0 && openingTagEnd >= roleIndex
        ? contents.slice(openingTagStart, openingTagEnd + 1)
        : "";
    if (/\baria-(?:label|labelledby)=/.test(openingTag)) continue;

    failures.push({
      file: rel,
      line: contents.slice(0, roleIndex).split(/\r?\n/).length,
      message:
        "Application menus require an accessible name through aria-label or aria-labelledby.",
      text: 'Add aria-label or aria-labelledby to the role="menu" owner.',
    });
  }

  const hasDirectionalKeys = ["ArrowDown", "ArrowUp", "Home", "End"].every((key) =>
    contents.includes(key),
  );
  const hasKeyboardOwner =
    contents.includes("useMenuKeyboardNavigation") ||
    (hasDirectionalKeys && /\bonKeyDown=\{/.test(contents));
  if (hasKeyboardOwner) return;

  failures.push({
    file: rel,
    line: 1,
    message:
      "Application menus require shared directional keyboard navigation; use useMenuKeyboardNavigation or an equivalent Arrow/Home/End handler.",
    text: 'Add a named onKeyDown owner for the role="menu" surface.',
  });
}

function validateManualMenuGeometry(rel, contents, failures) {
  const hasMenuSurface = /\brole=["']menu["']/.test(contents);
  const hasManualGeometry =
    /style=\{\{[^}]*\b(?:top|left|bottom|right):/.test(contents) ||
    (/getBoundingClientRect\(\)/.test(contents) &&
      /\b(?:linkMenu|menuPosition|Menu)\b/.test(contents));
  if (!hasMenuSurface || !hasManualGeometry) return;
  const approvedReason = approvedManualMenuGeometryPaths.get(rel);
  if (approvedReason) {
    if (!contents.includes(approvedReason)) {
      failures.push({
        file: rel,
        line: 1,
        message:
          "Approved manual menu geometry requires an explicit source marker explaining why the selection-owned exception is necessary.",
        text: `Add the marker ${JSON.stringify(approvedReason)} or migrate the menu to the shared anchored primitive.`,
      });
    }
    return;
  }

  failures.push({
    file: rel,
    line: 1,
    message:
      "Floating menus may not combine role=menu with caller-owned viewport geometry; use the shared anchored Radix primitive.",
    text: "Remove manual top/left positioning and getBoundingClientRect-based menu placement.",
  });
}

function validateFocusRingContract(repoRoot, failures) {
  const globalsPath = join(repoRoot, "apps/web/src/styles/globals.css");
  if (!existsSync(globalsPath)) return;

  const globals = readFileSync(globalsPath, "utf8");
  const requiredTokens = ["button:focus-visible", "ring-primary", "ring-offset-bg", "outline-none"];

  const sharedFocusRing = globals.match(/\.focus-ring\s*\{([\s\S]*?)\}/);
  const sharedFocusVisibleRing = globals.match(/\.focus-ring:focus-visible\s*\{([\s\S]*?)\}/);
  const sharedFocusRingIsStateful =
    !sharedFocusRing ||
    (!/\bring-(?:primary|2|offset)\b/.test(sharedFocusRing[1]) &&
      Boolean(
        sharedFocusVisibleRing &&
        /\bring-primary\b/.test(sharedFocusVisibleRing[1]) &&
        /\bring-2\b/.test(sharedFocusVisibleRing[1]),
      ));

  if (!requiredTokens.every((token) => globals.includes(token)) || !sharedFocusRingIsStateful) {
    failures.push({
      file: "apps/web/src/styles/globals.css",
      line: 1,
      message:
        "Interactive controls require the shared primary-token focus-visible ring without a permanent ring on unfocused controls.",
      text: "Keep .focus-ring quiet by default and apply its visible ring only through :focus-visible.",
    });
  }
}

function validateTypographyContract(repoRoot, failures) {
  const globalsPath = join(repoRoot, "apps/web/src/styles/globals.css");
  const tailwindPath = join(repoRoot, "apps/web/tailwind.config.ts");
  if (!existsSync(globalsPath) || !existsSync(tailwindPath)) return;

  const globals = readFileSync(globalsPath, "utf8");
  const tailwind = readFileSync(tailwindPath, "utf8");
  const requiredGlobals = [
    "@fontsource/plus-jakarta-sans/latin-600.css",
    '--font-heading: "Plus Jakarta Sans"',
    '--font-body: "Inter"',
    '--font-mono: "JetBrains Mono"',
    "font-family: var(--font-heading)",
    "font-family: var(--font-body)",
    "font-family: var(--font-mono)",
  ];
  const requiredTailwind = [
    'heading: ["var(--font-heading)"]',
    'sans: ["var(--font-body)"]',
    'mono: ["var(--font-mono)"]',
  ];

  if (
    requiredGlobals.every((token) => globals.includes(token)) &&
    requiredTailwind.every((token) => tailwind.includes(token))
  ) {
    return;
  }

  failures.push({
    file: "apps/web/src/styles/globals.css",
    line: 1,
    message:
      "The shared Font Trio Product contract requires self-hosted Plus Jakarta Sans headings, Inter body text, and JetBrains Mono code tokens.",
    text: "Restore the --font-heading, --font-body, and --font-mono imports and Tailwind mappings.",
  });
}

function validateCodeBlockFieldContract(repoRoot, failures) {
  const componentPath = join(repoRoot, "apps/web/src/components/editor/CodeBlockNodeView.tsx");
  if (!existsSync(componentPath)) return;

  const source = readFileSync(componentPath, "utf8");
  const editControlsAt = source.indexOf('data-code-block-edit-controls="true"');
  const readOnlyBranchAt = source.indexOf(") : (", editControlsAt);
  const copyActionAt = source.indexOf("aria-label={`Copy", editControlsAt);
  const requiredTokens = [
    'import { Input } from "@/components/ui/input"',
    'import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"',
    "code-block-metadata-field",
    'aria-label="Code language"',
    'aria-haspopup="menu"',
    'aria-label="Code languages"',
    'role="menuitemradio"',
    'aria-label="Code block name"',
    "border-border bg-surface-raised text-text",
    "bg-primary/15 text-primary",
    "bg-surface-raised h-10 min-w-0 pl-9 text-sm",
  ];
  const hasStyledFields = requiredTokens.every((token) => source.includes(token));
  const copyIsReadOnly =
    editControlsAt >= 0 && readOnlyBranchAt > editControlsAt && copyActionAt > readOnlyBranchAt;

  if (hasStyledFields && copyIsReadOnly) return;

  failures.push({
    file: "apps/web/src/components/editor/CodeBlockNodeView.tsx",
    line: 1,
    message:
      "Editable code-block metadata must use programmatically named, fully styled Starlane fields, and Copy must remain in the read-only branch.",
    text: "Restore the tokenized code-language menu and code-name field contract instead of falling back to raw-looking browser chrome.",
  });
}

function validateControlChromeContract(repoRoot, failures) {
  const contractPath = join(repoRoot, "apps/web/src/components/ui/control-chrome.ts");
  if (!existsSync(contractPath)) return;

  const requiredByFile = new Map([
    [
      "apps/web/src/components/ui/control-chrome.ts",
      [
        "CONTROL_CHROME_SURFACE_CLASS",
        "CONTROL_CHROME_RAIL_CLASS",
        "CONTROL_CHROME_ACTION_CLASS",
        "CONTROL_CHROME_TOUCH_ACTION_CLASS",
        "CONTROL_CHROME_ACTIVE_ACTION_CLASS",
        "CONTROL_CHROME_TOOLTIP_CLASS",
        "starlane-control-chrome-surface",
        "starlane-control-chrome-action",
        "starlane-control-chrome-active",
      ],
    ],
    [
      "apps/web/src/styles/globals.css",
      [
        "--control-chrome-surface: var(--surface)",
        "--control-chrome-hover: var(--surface-raised)",
        "--control-chrome-active:",
        ".starlane-control-chrome-surface",
        ".starlane-control-chrome-action:hover",
        ".starlane-control-chrome-active",
      ],
    ],
    ["apps/web/src/components/ui/tooltip.tsx", ["CONTROL_CHROME_TOOLTIP_CLASS"]],
    [
      "apps/web/src/components/editor/Editor.tsx",
      ["CONTROL_CHROME_RAIL_CLASS", "CONTROL_CHROME_ACTION_CLASS"],
    ],
    [
      "apps/web/src/components/layout/Sidebar.tsx",
      ["CONTROL_CHROME_RAIL_CLASS", "CONTROL_CHROME_ACTION_CLASS"],
    ],
    [
      "apps/web/src/pages/DocumentPage.tsx",
      ["CONTROL_CHROME_RAIL_CLASS", "CONTROL_CHROME_ACTIVE_ACTION_CLASS"],
    ],
    [
      "apps/web/src/components/ThemeModeSwitch.tsx",
      ["CONTROL_CHROME_RAIL_CLASS", "CONTROL_CHROME_ACTIVE_ACTION_CLASS"],
    ],
    [
      "apps/web/src/components/editor/MobileEditorToolbar.tsx",
      ["CONTROL_CHROME_RAIL_CLASS", "CONTROL_CHROME_TOUCH_ACTION_CLASS"],
    ],
    [
      "apps/web/src/pages/GraphPage.tsx",
      [
        "CONTROL_CHROME_RAIL_CLASS",
        "CONTROL_CHROME_ACTION_CLASS",
        'aria-label="Graph zoom controls"',
        'aria-label="Graph viewport controls"',
      ],
    ],
    [
      "apps/web/src/components/SearchModal.tsx",
      ["CONTROL_CHROME_RAIL_CLASS", "CONTROL_CHROME_ACTION_CLASS"],
    ],
    [
      "apps/web/src/pages/WorkflowQueuePage.tsx",
      ["CONTROL_CHROME_RAIL_CLASS", "CONTROL_CHROME_ACTION_CLASS"],
    ],
  ]);

  const valid = [...requiredByFile].every(([relativePath, tokens]) => {
    const path = join(repoRoot, relativePath);
    if (!existsSync(path)) return false;
    const source = readFileSync(path, "utf8");
    return tokens.every((token) => source.includes(token));
  });

  if (valid) return;

  failures.push({
    file: "apps/web/src/components/ui/control-chrome.ts",
    line: 1,
    message:
      "Toolbars, tooltips, and compact button groups must compose the shared Starlane control-chrome surface and action contract.",
    text: "Restore the semantic control-chrome tokens and CONTROL_CHROME_* composition in shared toolbar, tooltip, sidebar, document, graph, theme, and mobile owners.",
  });
}

function validateDropdownPrimitiveContract(repoRoot, failures) {
  const selectPath = join(repoRoot, "apps/web/src/components/ui/select.tsx");
  const menuPath = join(repoRoot, "apps/web/src/components/ui/menu-select.tsx");
  if (!existsSync(selectPath) || !existsSync(menuPath)) return;

  const selectSource = readFileSync(selectPath, "utf8");
  const menuSource = readFileSync(menuPath, "utf8");
  const selectContract = ["MenuSelect", "data-starlane-select-control", "aria-label", "emitChange"];
  const menuContract = [
    "PopoverContent",
    'role="listbox"',
    "aria-selected",
    "ArrowDown",
    "ArrowUp",
    "Home",
    "End",
    "setMenuOpen(false)",
  ];

  if (
    !/<select\b/.test(selectSource) &&
    selectContract.every((token) => selectSource.includes(token)) &&
    menuContract.every((token) => menuSource.includes(token))
  ) {
    return;
  }

  failures.push({
    file: "apps/web/src/components/ui/select.tsx",
    line: 1,
    message:
      "The canonical Select primitive must be an anchored Starlane menu, not a native browser select or a focus-incomplete wrapper.",
    text: "Route Select through MenuSelect and preserve anchored portal, keyboard, selection, and focus-return behavior.",
  });
}

function validateEditorToolbarContract(repoRoot, failures) {
  const editorPath = join(repoRoot, "apps/web/src/components/editor/Editor.tsx");
  if (!existsSync(editorPath)) return;

  const source = readFileSync(editorPath, "utf8");
  const requiredTokens = [
    "TABLE_EDITOR_TOOLBAR_SURFACE_CLASS = `${MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS}",
    "MOBILE_EDITOR_TOOLBAR_ACTION_CLASS",
    'label: "Font family"',
    '{ value: "Figtree", label: "Figtree" }',
    '{ value: "Source Serif 4", label: "Source Serif 4" }',
    '{ value: "Fira Code", label: "Fira Code" }',
    "getEditorFontFamilyCount()",
    "filterEditorFontGroups(fontFamilyQuery)",
    'aria-label="Search font families"',
    "bundled families · available offline",
    'data-starlane-selection-toolbar="true"',
    'aria-label="Text formatting"',
    "shouldShowSelectionToolbar({",
    "CONTROL_CHROME_ACTIVE_ACTION_CLASS",
    "onPointerDown={claimEditorControlPointer}",
  ];
  const hasSharedToolbar = requiredTokens.every((token) => source.includes(token));

  if (hasSharedToolbar) return;

  failures.push({
    file: "apps/web/src/components/editor/Editor.tsx",
    line: 1,
    message:
      "Editor toolbars must share the persistent main/mobile/contextual surface and action contract.",
    text: "Compose the selection and object-aware toolbars from shared editor primitives, keep Font family in the persistent catalog, and return pointer focus to the editor.",
  });
}

function validateGovernanceContract(repoRoot, failures, publicContributor = false) {
  if (publicContributor) {
    for (const path of [
      "AGENTS.md",
      "CONTRIBUTING.md",
      "contributor-kit/reference/interface-contract.md",
      ".agents/skills/starlane-interface-design/SKILL.md",
      ".agents/skills/starlane-interface-review/SKILL.md",
    ]) {
      if (!existsSync(join(repoRoot, path))) failures.push({
        file: path, line: 1, message: "Public contributor contract is missing.",
        text: "Restore the reviewed contributor kit before approving UI changes.",
      });
    }
    return;
  }
  const agentsPath = join(repoRoot, "AGENTS.md");
  if (!existsSync(agentsPath)) return;

  for (const relativePath of governanceArtifacts) {
    if (existsSync(join(repoRoot, relativePath))) continue;
    failures.push({
      file: relativePath,
      line: 1,
      message: "The canonical Starlane design-governance artifact is missing.",
      text: "Restore or migrate the versioned suite contract before approving UI changes.",
    });
  }

  const requiredReferences = new Map([
    [
      "AGENTS.md",
      [
        "repo-docs/guidelines/starlane-interface-craft.md",
        "documented,",
        "verified exception",
      ],
    ],
    [
      "CODEX.md",
      ["starlane-interface-design", "starlane-interface-review"],
    ],
    [
      ".codex/skills/starlane-interface-design/SKILL.md",
      ["name: starlane-interface-design", "references/contract-map.md"],
    ],
    [
      ".codex/skills/starlane-interface-review/SKILL.md",
      ["name: starlane-interface-review", "references/review-matrix.md"],
    ],
  ]);

  for (const [relativePath, tokens] of requiredReferences) {
    const path = join(repoRoot, relativePath);
    if (!existsSync(path)) continue;
    const source = readFileSync(path, "utf8");
    const missing = tokens.filter((token) => !source.includes(token));
    if (missing.length === 0) continue;
    failures.push({
      file: relativePath,
      line: 1,
      message: "Starlane design-governance guidance is stale or incomplete.",
      text: `Restore references to: ${missing.join(", ")}`,
    });
  }
}

export function validateStarlaneDesign({
  repoRoot = defaultRepoRoot,
  scanRoots = defaultScanRoots,
  publicContributor = false,
} = {}) {
  const failures = [];

  for (const root of scanRoots) {
    for (const file of walk(join(repoRoot, root))) {
      const rel = relative(repoRoot, file);
      if (!fileExtensions.has(extensionOf(file))) continue;

      const contents = readFileSync(file, "utf8");
      if (extensionOf(file) === ".tsx" && !rel.includes(".test.")) {
        validateMenuContract(rel, contents, failures);
        validateManualMenuGeometry(rel, contents, failures);
      }
      if (ignoredPathFragments.some((fragment) => rel.includes(fragment))) continue;

      const lines = contents.split(/\r?\n/);
      validateRawFormControls(rel, lines, failures);
      validateDialogOwnership(rel, contents, failures);
      lines.forEach((line, index) => {
        for (const rule of bannedPatterns) {
          if (!rule.pattern.test(line)) continue;
          if (rule.allow?.test(line)) continue;
          failures.push({
            file: rel,
            line: index + 1,
            message: rule.message,
            text: line.trim(),
          });
        }
      });
    }
  }

  validateFocusRingContract(repoRoot, failures);
  validateTypographyContract(repoRoot, failures);
  validateCodeBlockFieldContract(repoRoot, failures);
  validateControlChromeContract(repoRoot, failures);
  validateDropdownPrimitiveContract(repoRoot, failures);
  validateEditorToolbarContract(repoRoot, failures);
  validateGovernanceContract(repoRoot, failures, publicContributor);

  return failures;
}

function reportFailures(failures) {
  console.error("Starlane design validation failed:");
  for (const failure of failures) {
    console.error(`- ${failure.file}:${failure.line}: ${failure.message}`);
    console.error(`  ${failure.text}`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const failures = validateStarlaneDesign({ publicContributor: process.argv.includes("--public-contributor") });
  if (failures.length > 0) {
    reportFailures(failures);
    process.exit(1);
  }

  console.log("Starlane design validation passed.");
}
