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

const NUM_RE = /^-?[\d,]+\.\d+$/;
const PCT_RE = /^\(([+-][\d.]+)%\)$/;
const SESS_RE = /(At close|Overnight|Pre-Market|Pre-market|After hours|After-hours|As of|Market open|Live|Closed)/i;
const REGULAR_RE = /At close|As of|Market open|Live|Closed/i;
const OVERNIGHT_RE = /Overnight/i;
const PRE_RE = /Pre-Market|Pre-market/i;
const POST_RE = /After hours|After-hours/i;
const num = (s) => parseFloat(s.replace(/,/g, ""));

function parseYahooHeader(ticker, text) {
  const lines = text
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);

  const blocks = [];
  let price = null;
  let pct = null;
  for (const l of lines) {
    if (NUM_RE.test(l)) {
      if (price === null) price = num(l);
      continue;
    }
    const mp = l.match(PCT_RE);
    if (mp && price !== null) {
      pct = num(mp[1]);
      continue;
    }
    if (SESS_RE.test(l) && price !== null) {
      blocks.push({ price, pct: pct ?? 0, label: l });
      price = null;
      pct = null;
    }
  }
  return buildQuoteFromBlocks(ticker, blocks);
}

function buildQuoteFromBlocks(ticker, blocks) {
  if (!blocks.length) return null;

  const regular = blocks.find((b) => REGULAR_RE.test(b.label)) ?? blocks[0];
  const overnight = blocks.find((b) => OVERNIGHT_RE.test(b.label));
  const pre = blocks.find((b) => PRE_RE.test(b.label));
  const post = blocks.find((b) => POST_RE.test(b.label));
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

function parseYahooPieces(ticker, pieces) {
  const blocks = [];
  const sessionIndexes = pieces
    .map((value, index) => (SESS_RE.test(value) ? index : -1))
    .filter((index) => index >= 0);

  for (const index of sessionIndexes) {
    let price = null;
    let pct = null;
    for (let i = index - 1; i >= 0 && i >= index - 10; i -= 1) {
      const value = pieces[i];
      if (pct === null) {
        const pctMatch = value.match(PCT_RE);
        if (pctMatch) {
          pct = num(pctMatch[1]);
          continue;
        }
      }
      if (price === null && NUM_RE.test(value)) {
        price = num(value);
      }
      if (price !== null && pct !== null) {
        break;
      }
    }
    if (price !== null) {
      blocks.push({
        price,
        pct: pct ?? 0,
        label: pieces[index],
      });
    }
  }

  return buildQuoteFromBlocks(ticker, blocks);
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
  let snapshot = await page.evaluate(() => {
    const region =
      document.querySelector('[data-testid="quote-hdr"], [data-testid="quote-price"]') ||
      document.body;
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        const value = (node.textContent || "").replace(/\s+/g, " ").trim();
        return value ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
      },
    });
    const pieces = [];
    while (walker.nextNode()) {
      const value = (walker.currentNode.textContent || "").replace(/\s+/g, " ").trim();
      if (value && value.length <= 120) {
        pieces.push(value);
      }
      if (pieces.length >= 400) {
        break;
      }
    }
    return {
      headerText: region.innerText.slice(0, 1200),
      pieces,
    };
  });
  let parsed = parseYahooPieces(symbol, snapshot.pieces) || parseYahooHeader(symbol, snapshot.headerText);
  if (!parsed || parsed.extendedPrice === null) {
    await page.waitForTimeout(2500);
    snapshot = await page.evaluate(() => {
      const region =
        document.querySelector('[data-testid="quote-hdr"], [data-testid="quote-price"]') ||
        document.body;
      const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
        acceptNode(node) {
          const value = (node.textContent || "").replace(/\s+/g, " ").trim();
          return value ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
        },
      });
      const pieces = [];
      while (walker.nextNode()) {
        const value = (walker.currentNode.textContent || "").replace(/\s+/g, " ").trim();
        if (value && value.length <= 120) {
          pieces.push(value);
        }
        if (pieces.length >= 500) {
          break;
        }
      }
      return {
        headerText: region.innerText.slice(0, 1500),
        pieces,
      };
    });
    parsed = parseYahooPieces(symbol, snapshot.pieces) || parseYahooHeader(symbol, snapshot.headerText);
  }
  if (!parsed) process.exit(2);
  console.log(JSON.stringify(parsed));
  await page.close();
  await ctx.close();
} finally {
  await browser.close();
}
