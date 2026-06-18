#!/usr/bin/env node

const WebSocket = require("ws");

const symbols = process.argv.slice(2).map((s) => s.trim()).filter(Boolean);
if (!symbols.length) {
  console.error("usage: node py/yahoo_ws_raw.js SPCX NVDA QQQ");
  process.exit(1);
}

const ws = new WebSocket("wss://streamer.finance.yahoo.com/?version=2");

ws.on("open", () => {
  const payload = JSON.stringify({ subscribe: symbols });
  console.log(`[open] subscribe -> ${payload}`);
  ws.send(payload);
});

ws.on("message", (data) => {
  const text = data.toString("utf8");
  console.log(text);
});

ws.on("error", (err) => {
  console.error("[error]", err.message);
});

ws.on("close", (code, reason) => {
  console.error(`[close] code=${code} reason=${reason.toString("utf8")}`);
});
