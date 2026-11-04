#!/usr/bin/env node
"use strict";

const { spawn } = require("child_process");
const path = require("path");

const names = {
  "linux-x64": "@iron_adamant/trammel-linux-x64",
  "linux-arm64": "@iron_adamant/trammel-linux-arm64",
  "darwin-x64": "@iron_adamant/trammel-darwin-x64",
  "darwin-arm64": "@iron_adamant/trammel-darwin-arm64",
  "win32-x64": "@iron_adamant/trammel-windows-x64",
  "win32-arm64": "@iron_adamant/trammel-windows-arm64",
};

function binary() {
  const arch = process.arch === "arm64" ? "arm64" : "x64";
  const pkg = names[`${process.platform}-${arch}`];
  const file = process.platform === "win32" ? "trammel-mcp.exe" : "trammel-mcp";
  if (!pkg) {
    console.error("No Trammel MCP binary is published for " + process.platform + " " + process.arch);
    process.exit(1);
  }
  try {
    const root = path.dirname(require.resolve(pkg + "/package.json"));
    return path.join(root, file);
  } catch (err) {
    console.error("The platform package " + pkg + " is not installed.");
    process.exit(1);
  }
}

const child = spawn(binary(), process.argv.slice(2), { stdio: "inherit" });
child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code == null ? 1 : code);
});
