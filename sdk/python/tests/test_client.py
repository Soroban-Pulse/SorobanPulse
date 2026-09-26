import pytest

from soroban_pulse.client import SorobanPulseClient
from soroban_pulse.exceptions import AuthenticationError


def test_requires_api_key():
    with pytest.raises(AuthenticationError):
        SorobanPulseClient(api_key="")


def test_default_base_url_and_headers():
    client = SorobanPulseClient(api_key="sp_test_123")
    headers = client._headers()
    assert headers["Authorization"] == "Bearer sp_test_123"
    assert client.base_url.startswith("https://")


def test_base_url_trailing_slash_stripped():
    client = SorobanPulseClient(api_key="sp_test_123", base_url="https://example.com/v1/")
    assert client.base_url == "https://example.com/v1"
