#!/usr/bin/env python3
import asyncio
import json
import sys


async def main():
    try:
        import yfinance as yf
    except Exception as exc:
        print(json.dumps({
            "error": f"import yfinance failed: {exc}",
            "hint": "run: pip3 install -r /Users/wangkai/.codex/worktrees/eb3f/toush-fish/py/requirements.txt"
        }))
        return 2

    symbols = [s.strip().upper() for s in sys.argv[1:] if s.strip()]
    if not symbols:
        print(json.dumps({"error": "no symbols"}))
        return 2

    ws = yf.AsyncWebSocket()
    seen = {}
    try:
        await ws.subscribe(symbols)
        while len(seen) < len(symbols):
            message = await ws.listen()
            if not isinstance(message, dict):
                continue
            symbol = str(message.get("id") or message.get("symbol") or "").upper()
            if not symbol or symbol not in symbols:
                continue
            seen[symbol] = {
                "symbol": symbol,
                "price": message.get("price"),
                "changePercent": message.get("changePercent") or message.get("change_percent"),
                "change": message.get("change"),
                "previousClose": message.get("previousClose") or message.get("previous_close"),
                "bid": message.get("bid"),
                "ask": message.get("ask"),
                "marketHours": message.get("marketHours") or message.get("market_hours"),
                "shortName": message.get("shortName") or message.get("short_name"),
                "time": message.get("time"),
            }
        print(json.dumps({"quotes": list(seen.values())}, ensure_ascii=False))
        return 0
    finally:
        try:
            await ws.close()
        except Exception:
            pass


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
