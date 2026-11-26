#!/usr/bin/env python3
"""
sdk/conformance/webhook_sink/server.py

Lightweight webhook sink used by the SDK conformance test suite.

Listens on PORT (default 9002) for POST /hook requests and records every
delivery.  The conformance runner POSTs subscription webhook_url to this
address.

Endpoints:
  POST /hook         – Accept a webhook delivery (always returns 200)
  GET  /received     – Return list of received deliveries as JSON, including the
                       raw body and the X-Signature-256 header so runners can
                       recompute the HMAC
  POST /reset        – Clear all recorded deliveries
  GET  /health       – Liveness probe (returns {"status":"ok"})

Environment:
  PORT            – Listen port (default 9002)
  WEBHOOK_SECRET  – Shared secret used by the signature scenario; exposed via
                    GET /health so runners can verify HMACs
"""

from __future__ import annotations

import hashlib
import hmac
import json
import os
from http.server import BaseHTTPRequestHandler, HTTPServer
from typing import Any

received: list[dict[str, Any]] = []

WEBHOOK_SECRET = os.environ.get("WEBHOOK_SECRET", "")

# Path to the shared cross-language webhook signature fixture.
_FIXTURE_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "..",
    "..",
    "..",
    "tests",
    "fixtures",
    "webhook_signature_vectors.json",
)


def compute_signature(secret: str, timestamp: str, payload: str) -> str:
    """Compute the webhook signature: HMAC-SHA256(secret, timestamp + "." + payload)."""
    message = f"{timestamp}.{payload}".encode("utf-8")
    return hmac.new(secret.encode("utf-8"), message, hashlib.sha256).hexdigest()


def load_signature_vectors(path: str = _FIXTURE_PATH) -> list[dict[str, Any]]:
    """Load the shared webhook signature test vectors fixture."""
    with open(path, "r", encoding="utf-8") as fh:
        data = json.load(fh)
    if isinstance(data, dict):
        return data.get("vectors", [])
    return data


class SinkHandler(BaseHTTPRequestHandler):
    def log_message(self, fmt: str, *args: object) -> None:
        # Suppress default access log to keep CI output clean
        pass

    def _send(self, status: int, body: Any) -> None:
        payload = json.dumps(body).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_POST(self) -> None:
        if self.path == "/hook":
            length = int(self.headers.get("Content-Length", "0"))
            raw = self.rfile.read(length).decode("utf-8") if length else ""
            try:
                payload = json.loads(raw) if raw else {}
            except json.JSONDecodeError:
                payload = {"raw": raw}
            received.append({
                "headers": dict(self.headers),
                "body": payload,
                "raw_body": raw,
                "signature": self.headers.get("X-Signature-256", ""),
            })
            print(f"[webhook-sink] received delivery #{len(received)}", flush=True)
            self._send(200, {"status": "ok", "count": len(received)})
        elif self.path == "/reset":
            received.clear()
            self._send(200, {"status": "ok"})
        else:
            self._send(404, {"error": "not found"})

    def do_GET(self) -> None:
        if self.path == "/health":
            self._send(200, {"status": "ok", "secret_configured": bool(WEBHOOK_SECRET)})
        elif self.path == "/received":
            self._send(200, {"deliveries": received, "count": len(received)})
        else:
            self._send(404, {"error": "not found"})


def test_webhook_signature_vectors() -> None:
    """Verify every fixture vector against the server's signing scheme."""
    vectors = load_signature_vectors()
    assert vectors, "webhook_signature_vectors.json must contain at least one vector"
    for vector in vectors:
        expected = compute_signature(
            vector["secret"], vector["timestamp"], vector["payload"]
        )
        assert expected == vector["signature"], (
            f"signature mismatch for vector {vector.get('name', '<unnamed>')}: "
            f"expected {vector['signature']}, computed {expected}"
        )


def main() -> None:
    port = int(os.environ.get("PORT", "9002"))
    server = HTTPServer(("0.0.0.0", port), SinkHandler)
    print(
        f"[webhook-sink] listening on :{port} "
        f"(secret {'configured' if WEBHOOK_SECRET else 'not configured'})",
        flush=True,
    )
    server.serve_forever()


if __name__ == "__main__":
    main()
