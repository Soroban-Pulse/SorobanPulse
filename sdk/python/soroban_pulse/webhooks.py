"""Webhook signature verification utilities.

SorobanPulse signs outgoing webhook payloads with HMAC-SHA256 over
`{timestamp}.{body}`, sent as the `X-SorobanPulse-Signature` header in the
form `t=<unix_ts>,v1=<hex_hmac>`.
"""

from __future__ import annotations

import hashlib
import hmac
import time
from typing import Mapping, Optional, Union

SIGNATURE_HEADER = "X-SorobanPulse-Signature"
DEFAULT_TOLERANCE_SECONDS = 300


class WebhookVerificationError(Exception):
    """Raised when a webhook signature fails verification."""


def _parse_signature_header(header: str) -> tuple:
    parts = dict(item.split("=", 1) for item in header.split(",") if "=" in item)
    timestamp = parts.get("t")
    signature = parts.get("v1")
    if not timestamp or not signature:
        raise WebhookVerificationError("malformed signature header")
    return timestamp, signature


def verify_webhook_signature(
    payload: bytes,
    signature_header: str,
    secret: str,
    tolerance_seconds: int = 300,
) -> bool:
    """Verify a webhook request body against its signature header.

    Raises `WebhookVerificationError` if the signature is invalid or the
    timestamp is outside `tolerance_seconds` of the current time (replay
    protection). Returns True on success.
    """
    timestamp, signature = _parse_signature_header(signature_header)

    try:
        ts_int = int(timestamp)
    except ValueError as exc:
        raise WebhookVerificationError("invalid timestamp") from exc

    if abs(time.time() - ts_int) > tolerance_seconds:
        raise WebhookVerificationError("timestamp outside tolerance window")

    signed_payload = f"{timestamp}.".encode("utf-8") + payload
    expected = hmac.new(secret.encode("utf-8"), signed_payload, hashlib.sha256).hexdigest()

    if not hmac.compare_digest(expected, signature):
        raise WebhookVerificationError("signature mismatch")

    return True


def _get_header(headers: Mapping[str, str], name: str) -> Optional[str]:
    """Case-insensitive header lookup."""
    if name in headers:
        return headers[name]
    lowered = name.lower()
    for key, value in headers.items():
        if key.lower() == lowered:
            return value
    return None


def verify_webhook(
    raw_body: Union[bytes, str],
    headers: Mapping[str, str],
    secret: str,
    tolerance_sec: int = DEFAULT_TOLERANCE_SECONDS,
) -> bool:
    """Verify a webhook request using its raw body and headers.

    Mirrors the Go SDK helper: looks up the signature header
    (case-insensitively), enforces the timestamp tolerance window, computes
    HMAC-SHA256 over `{timestamp}.{raw_body}` and compares in constant time.

    `raw_body` must be the exact bytes received (do not re-serialize parsed
    JSON). Raises `WebhookVerificationError` on any failure; returns True on
    success.
    """
    header = _get_header(headers, SIGNATURE_HEADER)
    if not header:
        raise WebhookVerificationError("missing signature header")

    if isinstance(raw_body, str):
        raw_body = raw_body.encode("utf-8")

    return verify_webhook_signature(
        raw_body,
        header,
        secret,
        tolerance_seconds=tolerance_sec,
    )
