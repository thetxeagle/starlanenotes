#!/usr/bin/env node
import { randomBytes } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const args = new Set(process.argv.slice(2));
const secret = randomBytes(32).toString("hex");
const line = `JWT_SECRET=${secret}`;

if (!args.has("--write")) {
  console.log(line);
  process.exit(0);
}

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const envPath = args.has("--root") ? resolve(repoRoot, ".env") : resolve(repoRoot, "docker", ".env");
const current = existsSync(envPath) ? readFileSync(envPath, "utf8") : "";
const next = current.match(/^JWT_SECRET=.*$/m)
  ? current.replace(/^JWT_SECRET=.*$/m, line)
  : `${current}${current && !current.endsWith("\n") ? "\n" : ""}${line}\n`;

writeFileSync(envPath, next, { mode: 0o600 });
console.log(`Updated ${envPath} with a new JWT_SECRET.`);
