#!/usr/bin/env node
"use strict";
// Thin launcher: exec the prebuilt native binary downloaded by install.js.
// stdio is inherited so the terminal UI (raw mode / arrow keys) works.
const { spawnSync } = require("child_process");
const path = require("path");
const fs = require("fs");

// `npm link` development mode: run the current repository rather than the
// downloaded release binary, so `moyu` and `moyu-test` behave consistently.
const packageRoot = path.resolve(__dirname, "..");
const projectRoot = path.resolve(packageRoot, "..");
if (fs.existsSync(path.join(projectRoot, "Cargo.toml"))) {
  const result = spawnSync("cargo", ["run", "--bin", "moyu", "--", ...process.argv.slice(2)], {
    cwd: projectRoot,
    stdio: "inherit",
  });
  if (result.error) {
    console.error("moyu: 启动 cargo 失败 -", result.error.message);
    process.exit(1);
  }
  process.exit(result.status === null ? 1 : result.status);
}

const binName = process.platform === "win32" ? "moyu.exe" : "moyu";
const binPath = path.join(__dirname, binName);

if (!fs.existsSync(binPath)) {
  console.error(
    "moyu: 找不到可执行文件。请重新安装:npm i -g moyu-fish\n" +
      "(若安装时跳过了脚本,运行:node " +
      path.join(__dirname, "..", "install.js") +
      " )"
  );
  process.exit(1);
}

const res = spawnSync(binPath, process.argv.slice(2), { stdio: "inherit" });
if (res.error) {
  console.error("moyu: 启动失败 -", res.error.message);
  process.exit(1);
}
process.exit(res.status === null ? 1 : res.status);
