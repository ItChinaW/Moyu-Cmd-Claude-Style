#!/usr/bin/env node
import process from "node:process";
import path from "node:path";
import fs from "node:fs";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const here = path.dirname(new URL(import.meta.url).pathname);

function ensurePlaywrightReady() {
  const cliPath = path.join(here, "node_modules", "playwright", "cli.js");
  if (!fs.existsSync(cliPath)) {
    const installPkg = spawnSync("npm", ["install"], { cwd: here, stdio: "inherit" });
    if (installPkg.status !== 0) {
      throw new Error("failed to install playwright package");
    }
  }
  const browserPath = path.join(
    process.env.HOME || "",
    "Library",
    "Caches",
    "ms-playwright"
  );
  if (!fs.existsSync(browserPath) || fs.readdirSync(browserPath).length === 0) {
    const installBrowser = spawnSync(process.execPath, [cliPath, "install", "chromium"], {
      cwd: here,
      stdio: "inherit",
    });
    if (installBrowser.status !== 0) {
      throw new Error("failed to install chromium");
    }
  }
}

ensurePlaywrightReady();
const { chromium } = require("playwright");

const symbol = (process.argv[2] || "").trim().toUpperCase();
if (!symbol) {
  console.error("symbol required");
  process.exit(1);
}

const UA =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

function parseYahooHeader(ticker, text) {
  const lines = text
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);

  const numRe = /^-?[\d,]+\.\d+$/;
  const pctRe = /^\(([+-][\d.]+)%\)$/;
  const sessRe = /(At close|Overnight|Pre-Market|Pre-market|After hours|After-hours|As of|Market open|Live|Closed)/i;
  const num = (s) => parseFloat(s.replace(/,/g, ""));

  const blocks = [];
  let price = null;
  let pct = null;
  for (const l of lines) {
    if (numRe.test(l)) {
      if (price === null) price = num(l);
      continue;
    }
    const mp = l.match(pctRe);
    if (mp && price !== null) {
      pct = num(mp[1]);
      continue;
    }
    if (sessRe.test(l) && price !== null) {
      blocks.push({ price, pct: pct ?? 0, label: l });
      price = null;
      pct = null;
    }
  }
  if (!blocks.length) return null;

  const regular =
    blocks.find((b) => /At close|As of|Market open|Live|Closed/i.test(b.label)) ?? blocks[0];
  const overnight = blocks.find((b) => /Overnight/i.test(b.label));
  const pre = blocks.find((b) => /Pre-Market|Pre-market/i.test(b.label));
  const post = blocks.find((b) => /After hours|After-hours/i.test(b.label));
  const ext = overnight ?? post ?? pre;
  const previousClose =
    regular.pct !== 0 ? regular.price / (1 + regular.pct / 100) : regular.price;

  return {
    symbol: ticker,
    price: regular.price,
    change: regular.price - previousClose,
    changePercent: regular.pct,
    previousClose,
    extendedPrice: ext && ext.price !== regular.price ? ext.price : null,
    extendedChangePercent: ext && ext.price !== regular.price ? ext.pct : null,
  };
}

const browser = await chromium.launch({ headless: true });
try {
  const ctx = await browser.newContext({
    userAgent: UA,
    locale: "en-US",
    timezoneId: "America/New_York",
  });
  const page = await ctx.newPage();
  await page.goto(`https://finance.yahoo.com/quote/${encodeURIComponent(symbol)}/`, {
    waitUntil: "domcontentloaded",
    timeout: 45_000,
  });
  await page.waitForTimeout(3500);
  const text = await page.evaluate(() => {
    const region =
      document.querySelector('[data-testid="quote-hdr"], [data-testid="quote-price"]') ||
      document.body;
    return region.innerText.slice(0, 800);
  });
  const parsed = parseYahooHeader(symbol, text);
  if (!parsed) process.exit(2);
  console.log(JSON.stringify(parsed));
  await page.close();
  await ctx.close();
} finally {
  await browser.close();
}
