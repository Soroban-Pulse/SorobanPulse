import hashlib
import hmac
import time

import pytest

from soroban_pulse.webhooks import verify_webhook_signature, WebhookVerificationError

SECRET = "whsec_test_secret"


def sign(payload: bytes, ts: int, secret: str = SECRET) -> str:
    signed = f"{ts}.".encode("utf-8") + payload
    sig = hmac.new(secret.encode("utf-8"), signed, hashlib.sha256).hexdigest()
    return f"t={ts},v1={sig}"


def test_valid_signature_passes():
    payload = b'{"event_type": "transfer"}'
    ts = int(time.time())
    header = sign(payload, ts)
    assert verify_webhook_signature(payload, header, SECRET) is True


def test_invalid_signature_raises():
    payload = b'{"event_type": "transfer"}'
    ts = int(time.time())
    header = sign(payload, ts, secret="wrong_secret")
    with pytest.raises(WebhookVerificationError):
        verify_webhook_signature(payload, header, SECRET)


def test_expired_timestamp_raises():
    payload = b'{"event_type": "transfer"}'
    ts = int(time.time()) - 10_000
    header = sign(payload, ts)
    with pytest.raises(WebhookVerificationError):
        verify_webhook_signature(payload, header, SECRET, tolerance_seconds=300)


def test_malformed_header_raises():
    with pytest.raises(WebhookVerificationError):
        verify_webhook_signature(b"{}", "not-a-valid-header", SECRET)


def test_tampered_payload_fails():
    payload = b'{"event_type": "transfer"}'
    ts = int(time.time())
    header = sign(payload, ts)
    tampered = b'{"event_type": "mint"}'
    with pytest.raises(WebhookVerificationError):
        verify_webhook_signature(tampered, header, SECRET)
