#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

const repoRoot = resolve(new URL("../..", import.meta.url).pathname);
const docsRoot = join(repoRoot, "docs/docs");

const requiredDocs = [
  "index.md",
  "features/documents.md",
  "features/spaces.md",
  "features/properties.md",
  "features/local-first-mode.md",
  "features/themes.md",
  "features/users-groups-permissions.md",
  "features/ai-integration.md",
  "api/overview.md",
  "api/authentication.md",
  "api/webhooks.md",
  "api/notifications.md",
  "api/mcp.md",
  "authentication/local-auth.md",
  "authentication/oauth-google.md",
  "authentication/oauth-github.md",
  "authentication/oidc.md",
  "authentication/2fa.md",
  "authentication/scim.md",
  "authentication/auth-hardening.md",
  "user-provisioning/csv-import.md",
  "user-provisioning/sso-provisioning.md",
  "plugins/overview.md",
  "screenshots.md",
  "self-hosting/docker.md",
  "self-hosting/environment-variables.md",
  "self-hosting/upgrading.md",
  "self-hosting/app-readiness-validation.md",
  "self-hosting/app-store-review-readiness.md",
  "self-hosting/app-store-metadata.md",
  "self-hosting/reviewer-access.md",
  "self-hosting/gitea-container-registry.md",
  "self-hosting/gitea-actions-runner.md",
  "self-hosting/tauri-mobile-prerequisites.md",
  "self-hosting/tauri-updater-signing.md",
  "development.md",
  "development/phase-1-fast-track.md",
  "development/phased-prompt-gap-audit.md",
  "contributing.md",
  "privacy.md",
  "support.md",
];

const requiredFeatureMentions = [
  ["child pages", "features/documents.md"],
  ["attachments", "features/documents.md"],
  ["Excalidraw", "features/documents.md"],
  ["spaces", "features/spaces.md"],
  ["folders", "features/spaces.md"],
  ["permissions", "features/users-groups-permissions.md"],
  ["properties", "features/properties.md"],
  ["local-first", "features/local-first-mode.md"],
  ["AI", "features/ai-integration.md"],
  ["Scalar", "api/overview.md"],
  ["OpenAPI", "api/overview.md"],
  ["MCP", "api/mcp.md"],
  ["webhooks", "api/webhooks.md"],
  ["notifications", "api/notifications.md"],
  ["Docker", "self-hosting/docker.md"],
  ["environment variables", "self-hosting/environment-variables.md"],
  ["OIDC", "authentication/oidc.md"],
  ["SCIM", "authentication/scim.md"],
  ["2FA", "authentication/2fa.md"],
  ["plugins", "plugins/overview.md"],
];

const sourceEnvScanRoots = [
  "server/src",
  "apps/web/src",
  "apps/web/vite.config.ts",
  "scripts",
  "docker",
];

const legacyDocsToolingScanRoots = [
  "README.md",
  "AGENTS.md",
  "CODEX.md",
  "CLAUDE.md",
  ".claude/commands",
  ".codex/skills",
  "docs/docs",
  "repo-docs/guidelines",
];

const sourceExtensions = new Set([".rs", ".ts", ".tsx", ".js", ".mjs", ".sh", ".yml", ".yaml", ".example"]);
const documentationExtensions = new Set([".md", ".mdx"]);
const ignoredSourcePathParts = new Set(["node_modules", "target", "dist", "artifacts"]);
const ignoredEnvNames = new Set([
  "HOME",
  "PATH",
  "PWD",
  "RUST_LOG",
  "SHELL",
  "TMPDIR",
  "USER",
]);

function readText(path) {
  return readFileSync(path, "utf8");
}

function walkFiles(path) {
  if (!existsSync(path)) return [];
  const stat = statSync(path);
  if (stat.isFile()) return [path];

  const files = [];
  for (const entry of readdirSync(path)) {
    const child = join(path, entry);
    const relativeChild = relative(repoRoot, child);
    if (relativeChild.split(/[\\/]/).some((part) => ignoredSourcePathParts.has(part))) continue;
    files.push(...walkFiles(child));
  }
  return files;
}

function extensionOf(path) {
  const match = path.match(/(\.[^.]+)$/);
  return match?.[1] ?? "";
}

function collectDocumentedEnvVars() {
  const envDocPath = join(docsRoot, "self-hosting/environment-variables.md");
  if (!existsSync(envDocPath)) return new Set();
  const text = readText(envDocPath);
  return new Set([...text.matchAll(/`([A-Z][A-Z0-9_]{2,})`/g)].map((match) => match[1]));
}

function collectUsedEnvVars() {
  const names = new Set();
  const codeRegexes = [
    /(?:std::)?env::var\(\s*"([A-Z][A-Z0-9_]{2,})"\s*\)/g,
    /std::env::var\(\s*"([A-Z][A-Z0-9_]{2,})"\s*\)/g,
    /process\.env\.([A-Z][A-Z0-9_]{2,})/g,
    /import\.meta\.env\.([A-Z][A-Z0-9_]{2,})/g,
  ];
  const envFileRegex = /^\s*([A-Z][A-Z0-9_]{2,})=/gm;

  for (const root of sourceEnvScanRoots) {
    for (const file of walkFiles(join(repoRoot, root))) {
      if (!sourceExtensions.has(extensionOf(file)) && !file.endsWith(".env.example")) continue;
      const text = readText(file);
      for (const regex of codeRegexes) {
        regex.lastIndex = 0;
        for (const match of text.matchAll(regex)) {
          const name = match[1];
          if (!ignoredEnvNames.has(name)) names.add(name);
        }
      }
      if (file.endsWith(".env.example")) {
        for (const match of text.matchAll(envFileRegex)) {
          const name = match[1];
          if (!ignoredEnvNames.has(name)) names.add(name);
        }
      }
    }
  }

  return names;
}

function checkRequiredDocs(errors) {
  for (const doc of requiredDocs) {
    if (!existsSync(join(docsRoot, doc))) {
      errors.push(`Missing required docs page: docs/docs/${doc}`);
    }
  }
}

function checkFeatureMentions(errors) {
  for (const [term, doc] of requiredFeatureMentions) {
    const path = join(docsRoot, doc);
    if (!existsSync(path)) continue;
    const text = readText(path).toLowerCase();
    if (!text.includes(term.toLowerCase())) {
      errors.push(`Expected docs/docs/${doc} to mention "${term}".`);
    }
  }
}

function checkEnvVars(errors) {
  const used = collectUsedEnvVars();
  const documented = collectDocumentedEnvVars();
  const missing = [...used].filter((name) => !documented.has(name)).sort();
  if (missing.length > 0) {
    errors.push(`Undocumented environment variables: ${missing.join(", ")}`);
  }
}

function checkDockerDocs(errors) {
  const dockerFiles = [
    "docker/Dockerfile",
    "docker/docker-compose.yml",
    "docker/docker-compose.volume.yml",
    "docker/docker-compose.dev.yml",
    "docker/.env.example",
  ];
  const dockerDoc = join(docsRoot, "self-hosting/docker.md");
  const envDoc = join(docsRoot, "self-hosting/environment-variables.md");

  for (const file of dockerFiles) {
    if (!existsSync(join(repoRoot, file))) errors.push(`Missing Docker asset: ${file}`);
  }
  if (!existsSync(dockerDoc)) errors.push("Missing Docker docs: docs/docs/self-hosting/docker.md");
  if (!existsSync(envDoc)) {
    errors.push("Missing environment docs: docs/docs/self-hosting/environment-variables.md");
    return;
  }

  const dockerText = existsSync(dockerDoc) ? readText(dockerDoc) : "";
  const envText = readText(envDoc);
  for (const expected of ["docker compose", ".env", "BASE_URL", "JWT_SECRET", "volume"]) {
    const combined = `${dockerText}\n${envText}`.toLowerCase();
    if (!combined.includes(expected.toLowerCase())) {
      errors.push(`Docker/environment docs should mention "${expected}".`);
    }
  }
}

function checkWikiSections(errors) {
  const indexPath = join(docsRoot, "index.md");
  if (!existsSync(indexPath)) return;
  const text = readText(indexPath);
  for (const section of ["Features", "Self-hosting", "API", "Authentication", "Development"]) {
    if (!text.includes(section)) {
      errors.push(`docs/docs/index.md should include a "${section}" wiki/docs section.`);
    }
  }

  for (const doc of requiredDocs) {
    if (doc === "index.md") continue;
    if (!text.includes(doc)) {
      errors.push(`docs/docs/index.md should link required docs page: ${doc}`);
    }
  }
}

function checkScreenshotDocs(errors) {
  const screenshotPath = join(docsRoot, "screenshots.md");
  if (!existsSync(screenshotPath)) return;

  const text = readText(screenshotPath);
  const stalePhrases = ["Screenshots have not been added yet", "Screenshots will live"];
  for (const phrase of stalePhrases) {
    if (text.includes(phrase)) {
      errors.push(`docs/docs/screenshots.md should replace stale placeholder copy: "${phrase}".`);
    }
  }

  for (const expected of ["Documentation capture set", "App Store capture set", "actual iOS or iPadOS app", "iPhone", "iPad"]) {
    if (!text.includes(expected)) {
      errors.push(`docs/docs/screenshots.md should include "${expected}" screenshot guidance.`);
    }
  }
}

function checkPrivacyDocs(errors) {
  const privacyPath = join(docsRoot, "privacy.md");
  if (!existsSync(privacyPath)) return;

  const text = readText(privacyPath);
  for (const expected of ["self-hosted", "local", "AI", "OAuth", "administrator", "App Store"]) {
    if (!text.includes(expected)) {
      errors.push(`docs/docs/privacy.md should include "${expected}" privacy guidance.`);
    }
  }
}

function checkReviewerAccessDocs(errors) {
  const reviewerPath = join(docsRoot, "self-hosting/reviewer-access.md");
  if (!existsSync(reviewerPath)) return;

  const text = readText(reviewerPath);
  for (const expected of ["demo server", "reviewer credentials", "local mode", "review notes", "AI-disabled"]) {
    if (!text.includes(expected)) {
      errors.push(`docs/docs/self-hosting/reviewer-access.md should include "${expected}" reviewer-access guidance.`);
    }
  }
}

function checkAppStoreMetadataDocs(errors) {
  const metadataPath = join(docsRoot, "self-hosting/app-store-metadata.md");
  if (!existsSync(metadataPath)) return;

  const text = readText(metadataPath);
  for (const expected of ["description", "keywords", "support URL", "privacy policy URL", "screenshots", "unsupported"]) {
    if (!text.includes(expected)) {
      errors.push(`docs/docs/self-hosting/app-store-metadata.md should include "${expected}" metadata guidance.`);
    }
  }
}

function checkAuthHardeningDocs(errors) {
  const localAuthPath = join(docsRoot, "authentication/local-auth.md");
  if (existsSync(localAuthPath)) {
    const text = readText(localAuthPath);
    for (const expected of ["server deletes the current session record", "access token's `sid`", "client also discards"]) {
      if (!text.includes(expected)) {
        errors.push(`docs/docs/authentication/local-auth.md should include "${expected}" logout/session guidance.`);
      }
    }
    if (text.includes("logout is handled client-side by discarding tokens")) {
      errors.push("docs/docs/authentication/local-auth.md should not describe logout as client-only token discard.");
    }
  }

  const hardeningPath = join(docsRoot, "authentication/auth-hardening.md");
  if (existsSync(hardeningPath)) {
    const text = readText(hardeningPath);
    for (const expected of ["localStorage", "Secure", "HttpOnly", "SameSite", "CSRF", "compatibility mode"]) {
      if (!text.includes(expected)) {
        errors.push(`docs/docs/authentication/auth-hardening.md should include "${expected}" hardening guidance.`);
      }
    }
  }
}

function checkWorkflowEnforcement(errors) {
  const ciWorkflow = join(repoRoot, ".gitea/workflows/ci.yml");
  const wikiWorkflow = join(repoRoot, ".gitea/workflows/sync-wiki.yml");
  const documented = collectDocumentedEnvVars();

  if (!existsSync(ciWorkflow)) {
    errors.push("Missing docs validation workflow: .gitea/workflows/ci.yml");
  } else if (!readText(ciWorkflow).includes("npm run docs:validate")) {
    errors.push(".gitea/workflows/ci.yml should run `npm run docs:validate`.");
  }

  if (!existsSync(wikiWorkflow)) {
    errors.push("Missing wiki sync workflow: .gitea/workflows/sync-wiki.yml");
    return;
  }

  const wikiText = readText(wikiWorkflow);
  for (const expected of ["npm run docs:validate", "scripts/docs/sync-gitea-wiki.mjs", "--push"]) {
    if (!wikiText.includes(expected)) {
      errors.push(`.gitea/workflows/sync-wiki.yml should include "${expected}".`);
    }
  }

  for (const secret of ["WIKI_USERNAME", "WIKI_TOKEN"]) {
    if (!wikiText.includes(secret)) {
      errors.push(`.gitea/workflows/sync-wiki.yml should configure the ${secret} secret.`);
    }
    if (!documented.has(secret)) {
      errors.push(`docs/docs/self-hosting/environment-variables.md should document ${secret}.`);
    }
  }
}

function checkAgentWorkflowInventories(errors) {
  const inventoryFiles = ["AGENTS.md", "CODEX.md"];

  for (const file of inventoryFiles) {
    const path = join(repoRoot, file);
    if (!existsSync(path)) {
      errors.push(`Missing agent workflow inventory: ${file}`);
      continue;
    }

    const text = readText(path);
    const claudeCommandRefs = [...text.matchAll(/\.claude\/commands\/[A-Za-z0-9_.-]+\.md/g)].map(
      (match) => match[0],
    );
    for (const ref of claudeCommandRefs) {
      if (!existsSync(join(repoRoot, ref))) {
        errors.push(`${file} references missing Claude command: ${ref}`);
      }
    }

    if (!text.includes("app-readiness-validation")) {
      errors.push(`${file} should list the app-readiness validation Codex skill.`);
    }
  }

  const claudePath = join(repoRoot, "CLAUDE.md");
  if (existsSync(claudePath)) {
    const text = readText(claudePath);
    const slashCommandRefs = [...text.matchAll(/`\/([A-Za-z0-9_.-]+)`/g)].map(
      (match) => match[1],
    );
    for (const command of slashCommandRefs) {
      if (!existsSync(join(repoRoot, ".claude/commands", `${command}.md`))) {
        errors.push(`CLAUDE.md references missing Claude slash command: /${command}`);
      }
    }
  }
}

function checkLegacyDocsTooling(errors) {
  const legacyPattern = /\b(mkdocs|docusaurus|vitepress)\b/i;

  for (const root of legacyDocsToolingScanRoots) {
    for (const file of walkFiles(join(repoRoot, root))) {
      if (!documentationExtensions.has(extensionOf(file))) continue;
      const text = readText(file);
      const match = text.match(legacyPattern);
      if (match) {
        errors.push(
          `${relative(repoRoot, file)} references legacy docs tooling "${match[0]}"; use docs/docs, Gitea Wiki, or future Starlight portal guidance instead.`,
        );
      }
    }
  }
}

const errors = [];
checkRequiredDocs(errors);
checkFeatureMentions(errors);
checkEnvVars(errors);
checkDockerDocs(errors);
checkWikiSections(errors);
checkScreenshotDocs(errors);
checkPrivacyDocs(errors);
checkReviewerAccessDocs(errors);
checkAppStoreMetadataDocs(errors);
checkAuthHardeningDocs(errors);
checkWorkflowEnforcement(errors);
checkAgentWorkflowInventories(errors);
checkLegacyDocsTooling(errors);

if (errors.length > 0) {
  console.error("Documentation completeness validation failed:\n");
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}

console.log("Documentation completeness validation passed.");
