"""Synchronous SorobanPulse API client."""

from __future__ import annotations

import hashlib
import hmac
import json
import time
from typing import Any, Dict, Iterable, List, Optional, Union
from urllib import request as urllib_request
from urllib.error import HTTPError, URLError

from .exceptions import ApiError, AuthenticationError
from .models import MetaResponse

DEFAULT_BASE_URL = "https://api.sorobanpulse.io/v1"
DEFAULT_TIMEOUT = 30

#: Default tolerance (in seconds) for webhook signature timestamps.
DEFAULT_SIGNATURE_TOLERANCE = 300

SIGNATURE_HEADER = "X-SorobanPulse-Signature"
DELIVERY_ID_HEADER = "X-SorobanPulse-Delivery-Id"
LEGACY_SIGNATURE_HEADER = "X-Signature-256"


class WebhookSignatureError(Exception):
    """Raised when a webhook signature is missing, malformed, or invalid."""


def _header_value(headers: Dict[str, str], name: str) -> Optional[str]:
    """Case-insensitive header lookup."""
    for key, value in headers.items():
        if key.lower() == name.lower():
            return value
    return None


def _parse_signature_header(header: str) -> Dict[str, str]:
    """Parse ``t=<unix>,v1=<hex>`` into a dict of key/value pairs."""
    parts: Dict[str, str] = {}
    for item in header.split(","):
        item = item.strip()
        if not item or "=" not in item:
            continue
        key, _, value = item.partition("=")
        parts[key.strip()] = value.strip()
    return parts


def _compute_signature(secret: str, timestamp: str, body: Union[bytes, str]) -> str:
    if isinstance(body, str):
        body = body.encode("utf-8")
    signed_payload = timestamp.encode("utf-8") + b"." + body
    return hmac.new(secret.encode("utf-8"), signed_payload, hashlib.sha256).hexdigest()


def verify_webhook_signature(
    secret: str,
    body: Union[bytes, str],
    headers: Dict[str, str],
    tolerance: int = DEFAULT_SIGNATURE_TOLERANCE,
    now: Optional[int] = None,
) -> bool:
    """Verify an incoming SorobanPulse webhook signature.

    Supports signature v2 (``X-SorobanPulse-Signature: t=<unix>,v1=<hex>``)
    and falls back to the legacy ``X-Signature-256`` header during the
    deprecation window.

    Args:
        secret: The webhook signing secret.
        body: The raw request body (bytes or str).
        headers: The request headers (case-insensitive lookup).
        tolerance: Maximum allowed age of the timestamp in seconds.
        now: Override for the current unix time (useful in tests).

    Returns:
        ``True`` if the signature is valid and fresh.

    Raises:
        WebhookSignatureError: If the signature is missing, malformed,
            stale, or does not match.
    """
    if isinstance(body, str):
        body = body.encode("utf-8")
    current_time = int(time.time()) if now is None else int(now)

    signature_header = _header_value(headers, SIGNATURE_HEADER)
    if signature_header:
        parts = _parse_signature_header(signature_header)
        timestamp = parts.get("t")
        provided = parts.get("v1")
        if not timestamp or not provided:
            raise WebhookSignatureError("malformed signature header")
        try:
            ts = int(timestamp)
        except ValueError as exc:
            raise WebhookSignatureError("invalid signature timestamp") from exc
        if tolerance >= 0 and abs(current_time - ts) > tolerance:
            raise WebhookSignatureError("signature timestamp outside tolerance window")
        expected = _compute_signature(secret, timestamp, body)
        if not hmac.compare_digest(expected, provided):
            raise WebhookSignatureError("signature mismatch")
        return True

    legacy = _header_value(headers, LEGACY_SIGNATURE_HEADER)
    if legacy:
        provided = legacy.strip()
        if provided.startswith("sha256="):
            provided = provided[len("sha256="):]
        expected = hmac.new(secret.encode("utf-8"), body, hashlib.sha256).hexdigest()
        if not hmac.compare_digest(expected, provided):
            raise WebhookSignatureError("legacy signature mismatch")
        return True

    raise WebhookSignatureError("missing signature header")


class SorobanPulseClient:
    """Blocking client for the SorobanPulse REST API.

    Example:
        client = SorobanPulseClient(api_key="sp_live_...")
        events = client.list_events(contract_id="C...", limit=50)
    """

    def __init__(
        self,
        api_key: str,
        base_url: str = DEFAULT_BASE_URL,
        timeout: int = DEFAULT_TIMEOUT,
    ) -> None:
        if not api_key:
            raise AuthenticationError("api_key is required")
        self.api_key = api_key
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout

    def _headers(self) -> Dict[str, str]:
        return {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json",
            "User-Agent": "soroban-pulse-python-sdk/0.1.0",
        }

    def _request(self, method: str, path: str, params: Optional[Dict[str, Any]] = None,
                 body: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        url = f"{self.base_url}{path}"
        if params:
            query = "&".join(f"{k}={v}" for k, v in params.items() if v is not None)
            if query:
                url = f"{url}?{query}"

        data = json.dumps(body).encode("utf-8") if body is not None else None
        req = urllib_request.Request(url, data=data, headers=self._headers(), method=method)

        try:
            with urllib_request.urlopen(req, timeout=self.timeout) as resp:
                raw = resp.read()
                return json.loads(raw) if raw else {}
        except HTTPError as exc:
            payload = exc.read()
            try:
                parsed = json.loads(payload) if payload else {}
            except json.JSONDecodeError:
                parsed = {"message": payload.decode("utf-8", "replace")}
            if exc.code in (401, 403):
                raise AuthenticationError(parsed.get("message", "authentication failed")) from exc
            raise ApiError(exc.code, parsed.get("message", str(exc)), payload=parsed) from exc
        except URLError as exc:
            raise ApiError(0, f"network error: {exc.reason}") from exc

    # -- System -----------------------------------------------------------

    def get_meta(self) -> MetaResponse:
        """Build metadata and enabled features (``GET /v1/meta``, no auth needed)."""
        return MetaResponse.from_dict(self._request("GET", "/meta"))

    # -- Events -----------------------------------------------------------

    def list_events(
        self,
        contract_id: Optional[str] = None,
        event_type: Optional[str] = None,
        limit: int = 100,
        cursor: Optional[str] = None,
    ) -> Dict[str, Any]:
        params = {
            "contract_id": contract_id,
            "event_type": event_type,
            "limit": limit,
            "cursor": cursor,
        }
        return self._request("GET", "/events", params=params)

    def iter_events(self, **kwargs: Any) -> Iterable[Dict[str, Any]]:
        """Yield events across all pages, following `next_cursor`."""
        cursor = kwargs.pop("cursor", None)
        while True:
            page = self.list_events(cursor=cursor, **kwargs)
            for event in page.get("data", []):
                yield event
            cursor = page.get("next_cursor")
            if not cursor:
                break

    def get_event(self, event_id: str) -> Dict[str, Any]:
        return self._request("GET", f"/events/{event_id}")

    # -- Subscriptions ------------------------------------------------------

    def list_subscriptions(self) -> List[Dict[str, Any]]:
        return self._request("GET", "/subscriptions").get("data", [])

    def create_subscription(self, contract_id: str, webhook_url: str,
                             event_types: Optional[List[str]] = None) -> Dict[str, Any]:
        body = {
            "contract_id": contract_id,
            "webhook_url": webhook_url,
            "event_types": event_types or [],
        }
        return self._request("POST", "/subscriptions", body=body)

    def delete_subscription(self, subscription_id: str) -> None:
        self._request("DELETE", f"/subscriptions/{subscription_id}")
