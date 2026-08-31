"""Exception types raised by the SorobanPulse Python SDK."""

from __future__ import annotations

from typing import Any, Optional


class SorobanPulseError(Exception):
    """Base class for all SDK errors."""


class ApiError(SorobanPulseError):
    """Raised when the API returns a non-2xx response."""

    def __init__(self, status_code: int, message: str, payload: Optional[Any] = None) -> None:
        super().__init__(f"API error {status_code}: {message}")
        self.status_code = status_code
        self.message = message
        self.payload = payload


class AuthenticationError(SorobanPulseError):
    """Raised when the API key is missing or rejected by the server."""
