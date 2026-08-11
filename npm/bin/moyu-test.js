#!/usr/bin/env node
"use strict";

// Development launcher. When npm link points at this repository, run the
// current Cargo source so local changes are immediately debuggable. A normal
// globally installed package has no Cargo.toml, so it falls back to the
// prebuilt binary downloaded by install.js.
const { spawnSync } = require("child_process");
const path = require("path");
const fs = require("fs");

const packageRoot = path.resolve(__dirname, "..");
const projectRoot = path.resolve(packageRoot, "..");
const args = process.argv.slice(2);

if (fs.existsSync(path.join(projectRoot, "Cargo.toml"))) {
  const result = spawnSync("cargo", ["run", "--bin", "moyu", "--", ...args], {
    cwd: projectRoot,
    stdio: "inherit",
  });
  if (result.error) {
    console.error("moyu-test: 启动 cargo 失败 -", result.error.message);
    process.exit(1);
  }
  process.exit(result.status === null ? 1 : result.status);
}

const binName = process.platform === "win32" ? "moyu.exe" : "moyu";
const binPath = path.join(__dirname, binName);
if (!fs.existsSync(binPath)) {
  console.error(
    "moyu-test: 找不到调试入口。请在项目 npm 目录执行 npm link，或重新安装 moyu-fish。"
  );
  process.exit(1);
}

const result = spawnSync(binPath, args, { stdio: "inherit" });
if (result.error) {
  console.error("moyu-test: 启动失败 -", result.error.message);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
