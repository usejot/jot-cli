#!/usr/bin/env node
/**
 * One version for the `jot` CLI everywhere it is written: the crate
 * (Cargo.toml), @usejot/cli and its five platform packages,
 * and @usejot/cli's optionalDependencies on them.
 *
 *   node npm/version.mjs 0.2.0   # set it everywhere
 *   node npm/version.mjs --check # fail unless all agree (CI); prints it
 *   node npm/version.mjs --check 0.2.0  # …and equal this (a cli-v0.2.0 tag)
 *
 * Cargo.lock follows on the next `cargo build`.
 */
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// the crate is this file's parent folder, here and in the exported usejot/jot-cli repo.
// fileURLToPath, not .pathname: on Windows .pathname is "/D:/…", which join turns into D:\D:\…
const CRATE = fileURLToPath(new URL("..", import.meta.url));
const ROOT = CRATE;
const CARGO = join(CRATE, "Cargo.toml");
const NPM = join(CRATE, "npm");

/** Every npm package of the CLI: @usejot/cli first, then the platform ones. */
const packages = ["cli", ...readdirSync(NPM).filter((dir) => dir.startsWith("cli-"))].map((dir) =>
  join(NPM, dir, "package.json"),
);

const CARGO_VERSION = /^version = "([^"]+)"$/m;

function versions() {
  const found = new Map();
  found.set(CARGO, CARGO_VERSION.exec(readFileSync(CARGO, "utf8"))?.[1]);
  for (const path of packages) {
    const pkg = JSON.parse(readFileSync(path, "utf8"));
    found.set(path, pkg.version);
    for (const [dep, version] of Object.entries(pkg.optionalDependencies ?? {})) {
      found.set(`${path} → ${dep}`, version);
    }
  }
  return found;
}

const args = process.argv.slice(2);
const wanted = args.find((arg) => !arg.startsWith("--"));

// 1. check: every place says the same version (and the one asked for, if any)
if (args.includes("--check")) {
  const found = versions();
  const expected = wanted ?? found.get(CARGO);
  const wrong = [...found].filter(([, version]) => version !== expected);
  for (const [where, version] of wrong) {
    console.error(`  ✗ ${where.replace(ROOT, "")}: ${version}, expected ${expected}`);
  }
  if (wrong.length > 0) process.exit(1);
  console.log(expected);
  process.exit(0);
}

// 2. set: the crate, each package, and the main package's pins on the platform ones
if (!wanted || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(wanted)) {
  console.error("Usage: node npm/version.mjs <x.y.z> | --check [x.y.z]");
  process.exit(1);
}
writeFileSync(CARGO, readFileSync(CARGO, "utf8").replace(CARGO_VERSION, `version = "${wanted}"`));
for (const path of packages) {
  const pkg = JSON.parse(readFileSync(path, "utf8"));
  pkg.version = wanted;
  for (const dep of Object.keys(pkg.optionalDependencies ?? {}))
    pkg.optionalDependencies[dep] = wanted;
  writeFileSync(path, `${JSON.stringify(pkg, null, 2)}\n`);
}
console.log(`  ✓ jot CLI ${wanted}: Cargo.toml and ${packages.length} npm packages`);
