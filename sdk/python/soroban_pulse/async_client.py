"""Async (asyncio) SorobanPulse API client, mirroring `SorobanPulseClient`."""

from __future__ import annotations

import asyncio
import json
from typing import Any, AsyncIterator, Dict, List, Optional

from .exceptions import ApiError, AuthenticationError
from .client import DEFAULT_BASE_URL, DEFAULT_TIMEOUT


class AsyncSorobanPulseClient:
    """Async client built on top of `aiohttp` (imported lazily so the sync
    client has no hard dependency on it).

    Example:
        async with AsyncSorobanPulseClient(api_key="sp_live_...") as client:
            events = await client.list_events(contract_id="C...")
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
        self._session = None

    async def __aenter__(self) -> "AsyncSorobanPulseClient":
        import aiohttp  # noqa: WPS433 (lazy import to keep this optional)

        self._session = aiohttp.ClientSession(
            headers=self._headers(),
            timeout=aiohttp.ClientTimeout(total=self.timeout),
        )
        return self

    async def __aexit__(self, *exc_info: Any) -> None:
        if self._session is not None:
            await self._session.close()
            self._session = None

    def _headers(self) -> Dict[str, str]:
        return {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json",
            "User-Agent": "soroban-pulse-python-sdk/0.1.0",
        }

    async def _request(self, method: str, path: str, params: Optional[Dict[str, Any]] = None,
                        body: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        if self._session is None:
            raise RuntimeError("AsyncSorobanPulseClient must be used as an async context manager")

        url = f"{self.base_url}{path}"
        clean_params = {k: v for k, v in (params or {}).items() if v is not None}

        async with self._session.request(method, url, params=clean_params, json=body) as resp:
            text = await resp.text()
            payload = json.loads(text) if text else {}
            if resp.status >= 400:
                if resp.status in (401, 403):
                    raise AuthenticationError(payload.get("message", "authentication failed"))
                raise ApiError(resp.status, payload.get("message", "request failed"), payload=payload)
            return payload

    async def list_events(
        self,
        contract_id: Optional[str] = None,
        event_type: Optional[str] = None,
        limit: int = 100,
        cursor: Optional[str] = None,
    ) -> Dict[str, Any]:
        return await self._request(
            "GET",
            "/events",
            params={
                "contract_id": contract_id,
                "event_type": event_type,
                "limit": limit,
                "cursor": cursor,
            },
        )

    async def iter_events(self, **kwargs: Any) -> AsyncIterator[Dict[str, Any]]:
        cursor = kwargs.pop("cursor", None)
        while True:
            page = await self.list_events(cursor=cursor, **kwargs)
            for event in page.get("data", []):
                yield event
            cursor = page.get("next_cursor")
            if not cursor:
                break

    async def create_subscription(self, contract_id: str, webhook_url: str,
                                   event_types: Optional[List[str]] = None) -> Dict[str, Any]:
        body = {
            "contract_id": contract_id,
            "webhook_url": webhook_url,
            "event_types": event_types or [],
        }
        return await self._request("POST", "/subscriptions", body=body)

    async def wait_until_ready(self, poll_interval: float = 1.0, attempts: int = 5) -> bool:
        """Poll the health endpoint until the API is reachable."""
        for _ in range(attempts):
            try:
                await self._request("GET", "/health")
                return True
            except ApiError:
                await asyncio.sleep(poll_interval)
        return False
