#!/usr/bin/env node
/**
 * `jot` on npm: finds the native binary npm installed for this machine (one
 * of the optionalDependencies, chosen by their os/cpu fields) and runs it,
 * passing arguments, stdio, the exit code and signals straight through.
 * The CLI itself is Rust (packages/jot-cli); this file only locates it.
 */
"use strict";
const { spawnSync } = require("node:child_process");
const { optionalDependencies } = require("../package.json");

// 1. the binary for this platform, from its own package
const platform = `${process.platform}-${process.arch}`;
const name = `@usejot/cli-${platform}`;
const exe = process.platform === "win32" ? "jot.exe" : "jot";
let binary;
try {
  binary = require.resolve(`${name}/bin/${exe}`);
} catch {
  const supported = Object.keys(optionalDependencies).map((dep) => dep.replace("@usejot/cli-", ""));
  console.error(
    supported.includes(platform)
      ? `✗ ${name} is missing: npm skipped it. Reinstall without --no-optional (or --omit=optional): npm i -g @usejot/cli`
      : `✗ jot has no build for ${platform} yet (it ships for ${supported.join(", ")}). The MCP server alone still works: npx -y @usejot/mcp`,
  );
  process.exit(1);
}

// 2. run it, as if it were this process
const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`✗ ${result.error.message}`);
  process.exit(1);
}
if (result.signal) process.kill(process.pid, result.signal);
process.exit(result.status ?? 1);
