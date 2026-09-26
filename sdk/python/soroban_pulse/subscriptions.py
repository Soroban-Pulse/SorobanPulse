"""Event subscription helpers (SSE streaming)."""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Callable, Iterator, Optional
from urllib import request as urllib_request

from .client import SorobanPulseClient


@dataclass
class SorobanEvent:
    id: str
    contract_id: str
    event_type: str
    ledger: int
    data: dict


class EventSubscription:
    """Consumes SorobanPulse's Server-Sent Events (SSE) stream and dispatches
    parsed events to a callback, with automatic reconnection.

    Example:
        sub = EventSubscription(client, contract_id="C...")
        sub.on_event(lambda evt: print(evt.event_type, evt.data))
        sub.run()
    """

    def __init__(self, client: SorobanPulseClient, contract_id: Optional[str] = None,
                 event_types: Optional[list] = None) -> None:
        self.client = client
        self.contract_id = contract_id
        self.event_types = event_types or []
        self._handler: Optional[Callable[[SorobanEvent], None]] = None
        self._stopped = False

    def on_event(self, handler: Callable[[SorobanEvent], None]) -> None:
        self._handler = handler

    def stop(self) -> None:
        self._stopped = True

    def _stream_url(self) -> str:
        params = []
        if self.contract_id:
            params.append(f"contract_id={self.contract_id}")
        for et in self.event_types:
            params.append(f"event_type={et}")
        query = "&".join(params)
        suffix = f"?{query}" if query else ""
        return f"{self.client.base_url}/events/stream{suffix}"

    def _iter_sse_lines(self) -> Iterator[str]:
        req = urllib_request.Request(self._stream_url(), headers=self.client._headers())
        with urllib_request.urlopen(req) as resp:
            for raw_line in resp:
                yield raw_line.decode("utf-8").rstrip("\n")

    def run(self, max_reconnects: int = 10) -> None:
        """Blocking loop that reconnects with exponential backoff on
        transient network failures, up to `max_reconnects` attempts."""
        attempts = 0
        while not self._stopped and attempts < max_reconnects:
            try:
                buffer = ""
                for line in self._iter_sse_lines():
                    if self._stopped:
                        return
                    if line.startswith("data:"):
                        buffer = line[len("data:"):].strip()
                    elif line == "" and buffer:
                        self._dispatch(buffer)
                        buffer = ""
                attempts = 0
            except Exception:
                attempts += 1
                backoff = min(2 ** attempts, 30)
                import time

                time.sleep(backoff)

    def _dispatch(self, raw_json: str) -> None:
        if not self._handler:
            return
        payload = json.loads(raw_json)
        event = SorobanEvent(
            id=payload.get("id", ""),
            contract_id=payload.get("contract_id", ""),
            event_type=payload.get("event_type", ""),
            ledger=payload.get("ledger", 0),
            data=payload.get("data", {}),
        )
        self._handler(event)
