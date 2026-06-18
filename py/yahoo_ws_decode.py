#!/usr/bin/env python3
import base64
import json
import sys


def main() -> int:
    try:
        from google.protobuf.json_format import MessageToDict
        from yfinance.pricing_pb2 import PricingData
    except Exception as exc:
        print(json.dumps({"error": f"import failed: {exc}"}), file=sys.stderr)
        return 2

    if len(sys.argv) > 1:
        raw = sys.argv[1]
    else:
        raw = sys.stdin.read().strip()

    if not raw:
        print(json.dumps({"error": "empty input"}), file=sys.stderr)
        return 2

    try:
        payload = json.loads(raw)
    except Exception as exc:
        print(json.dumps({"error": f"invalid json: {exc}"}), file=sys.stderr)
        return 2

    if "message" not in payload:
        print(json.dumps({"error": "missing message field"}), file=sys.stderr)
        return 2

    try:
        decoded = base64.b64decode(payload["message"])
        pricing = PricingData()
        pricing.ParseFromString(decoded)
        obj = MessageToDict(pricing, preserving_proto_field_name=True)
        print(json.dumps(obj, ensure_ascii=False, indent=2))
        return 0
    except Exception as exc:
        print(json.dumps({"error": f"decode failed: {exc}"}), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
