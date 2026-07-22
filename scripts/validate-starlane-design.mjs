#!/usr/bin/env node
import { pathToFileURL } from "node:url";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const defaultRepoRoot = new URL("..", import.meta.url).pathname;
const defaultScanRoots = ["apps/web/src"];
const fileExtensions = new Set([".css", ".ts", ".tsx"]);

const bannedPatterns = [
  {
    pattern: /\bbackdrop-blur(?:-[\w[\]/.]+)?\b/,
    message: "Glassmorphism/backdrop blur is banned by the Starlane brand baseline.",
  },
  {
    pattern: /\bbg-gradient-|linear-gradient\(|radial-gradient\(|conic-gradient\(/,
    message: "Decorative gradients are banned in app UI.",
  },
  {
    pattern: /\bshadow-(?:lg|xl|2xl|inner)\b|drop-shadow/,
    message: "Use Starlane shadow tokens only; large/drop shadows are banned.",
  },
  {
    pattern: /\bbg-background\b/,
    message: "`bg-background` is not part of the Starlane token set; use `bg-bg`, `bg-surface`, or `bg-surface-raised`.",
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

const allowedRawInputTypes = new Set(["checkbox", "color", "file", "hidden", "radio", "range"]);

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

export function validateStarlaneDesign({
  repoRoot = defaultRepoRoot,
  scanRoots = defaultScanRoots,
} = {}) {
  const failures = [];

  for (const root of scanRoots) {
    for (const file of walk(join(repoRoot, root))) {
      const rel = relative(repoRoot, file);
      if (ignoredPathFragments.some((fragment) => rel.includes(fragment))) continue;
      if (!fileExtensions.has(extensionOf(file))) continue;

      const lines = readFileSync(file, "utf8").split(/\r?\n/);
      validateRawFormControls(rel, lines, failures);
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
  const failures = validateStarlaneDesign();
  if (failures.length > 0) {
    reportFailures(failures);
    process.exit(1);
  }

  console.log("Starlane design validation passed.");
}
