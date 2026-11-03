#!/usr/bin/env node
"use strict";

const { spawn } = require("child_process");
const path = require("path");

const names = {
  "linux-x64": "trammel-linux-x64",
  "linux-arm64": "trammel-linux-arm64",
  "darwin-x64": "trammel-darwin-x64",
  "darwin-arm64": "trammel-darwin-arm64",
  "win32-x64": "trammel-windows-x64",
  "win32-arm64": "trammel-windows-arm64",
};

function target() {
  const arch = process.arch === "arm64" ? "arm64" : "x64";
  return names[`${process.platform}-${arch}`];
}

function binary() {
  const pkg = target();
  const file = process.platform === "win32" ? "trammel.exe" : "trammel";
  if (!pkg) {
    console.error("No Trammel binary is published for " + process.platform + " " + process.arch);
    process.exit(1);
  }
  try {
    const root = path.dirname(require.resolve(pkg + "/package.json"));
    return path.join(root, file);
  } catch (err) {
    console.error(
      "The platform package " + pkg + " is not installed. " +
      "Install Trammel from the GitHub Release, or reinstall so the optional dependency for this OS is selected."
    );
    process.exit(1);
  }
}

const child = spawn(binary(), process.argv.slice(2), { stdio: "inherit" });
child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code == null ? 1 : code);
});
