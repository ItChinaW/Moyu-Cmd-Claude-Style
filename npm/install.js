#!/usr/bin/env node
"use strict";
// postinstall: download the prebuilt binary matching this platform/arch from the
// GitHub Release that matches the package version, into bin/.
const fs = require("fs");
const path = require("path");
const https = require("https");

const pkg = require("./package.json");
const REPO = "ItChinaW/Moyu-Cmd-Claude-Style";

// Pick the release asset for this platform. macOS = one universal binary (Intel + M),
// Windows = x64 exe. (Linux 用户请用 cargo install 自行编译。)
const isWin = process.platform === "win32";
let asset;
if (process.platform === "darwin") {
  asset = "moyu-darwin-universal";
} else if (isWin && process.arch === "x64") {
  asset = "moyu-win32-x64.exe";
} else {
  console.error(
    `moyu: 暂未提供 ${process.platform}/${process.arch} 的预编译包。` +
      `\n可从 https://github.com/${REPO}/releases 手动下载,或用 cargo install --git https://github.com/${REPO} 自行编译。`
  );
  process.exit(0); // don't hard-fail the whole npm install
}

const url = `https://github.com/${REPO}/releases/download/v${pkg.version}/${asset}`;

const outDir = path.join(__dirname, "bin");
const outPath = path.join(outDir, isWin ? "moyu.exe" : "moyu");
fs.mkdirSync(outDir, { recursive: true });

function download(u, dest, redirects = 0) {
  return new Promise((resolve, reject) => {
    if (redirects > 10) return reject(new Error("too many redirects"));
    https
      .get(u, { headers: { "User-Agent": "moyu-installer" } }, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          res.resume();
          return resolve(download(res.headers.location, dest, redirects + 1));
        }
        if (res.statusCode !== 200) {
          res.resume();
          return reject(new Error(`HTTP ${res.statusCode} for ${u}`));
        }
        const tmp = dest + ".download";
        const file = fs.createWriteStream(tmp);
        res.pipe(file);
        file.on("finish", () => file.close(() => {
          fs.renameSync(tmp, dest);
          resolve();
        }));
        file.on("error", reject);
      })
      .on("error", reject);
  });
}

(async () => {
  try {
    console.log(`moyu: 正在下载 ${asset} …`);
    await download(url, outPath);
    if (!isWin) fs.chmodSync(outPath, 0o755);
    console.log("moyu: 安装完成,直接运行 `moyu` 即可。");
  } catch (e) {
    console.error(
      `moyu: 下载预编译二进制失败(${e.message})。\n` +
        `请确认该版本的 Release 已发布,或从 https://github.com/${REPO}/releases 手动下载 ${asset} 放到:\n  ${outPath}`
    );
    process.exit(0); // soft-fail: let the user fix it manually without breaking install
  }
})();
