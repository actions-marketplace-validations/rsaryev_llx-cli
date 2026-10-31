#!/usr/bin/env node
// Starts the llx binary of this platform, which npm installed as an optional dependency
// (@localixai/llx-<os>-<cpu>, one per release target). The wrapper only finds it and hands over:
// arguments, stdio and the exit code pass through, and so do the signals that stop a run.
"use strict";

const { spawn } = require("node:child_process");
const { chmodSync } = require("node:fs");

const key = `${process.platform}-${process.arch}`;
const SUPPORTED = ["darwin-arm64", "darwin-x64", "linux-x64"];
function fail(message) {
  process.stderr.write(`llx: ${message}\n`);
  process.exit(3);
}

function binary() {
  if (!SUPPORTED.includes(key)) {
    fail(`no build for ${key}. Supported: macOS (arm64, x64) and Linux x64.`);
  }
  try {
    return require.resolve(`@localixai/llx-${key}/bin/llx`);
  } catch {
    fail(
      `the package @localixai/llx-${key} is missing. Reinstall without --omit=optional ` +
        "or --no-optional, or install the binary: https://github.com/rsaryev/llx#install",
    );
  }
}

function start(path, retry) {
  const child = spawn(path, process.argv.slice(2), { stdio: "inherit" });
  const relay = (signal) => () => child.kill(signal);
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.on(signal, relay(signal));
  }
  child.on("error", (error) => {
    // npm packs files without the executable bit unless they are listed as a bin.
    if (error.code === "EACCES" && retry) {
      chmodSync(path, 0o755);
      start(path, false);
      return;
    }
    fail(`cannot start ${path}: ${error.message}`);
  });
  child.on("exit", (code, signal) => {
    if (signal) {
      process.removeAllListeners(signal);
      process.kill(process.pid, signal);
    } else {
      process.exit(code ?? 1);
    }
  });
}

start(binary(), true);
