# coding: utf-8

import asyncio
import unittest
from unittest.mock import AsyncMock, MagicMock, patch

from openapi_client.configuration import Configuration
from openapi_client.rest import RESTClientObject


def _make_response(status_code: int, headers: dict | None = None):
    resp = MagicMock()
    resp.status_code = status_code
    resp.reason_phrase = "Test"
    resp.headers = headers or {}
    return resp


class TestRetryLogic(unittest.IsolatedAsyncioTestCase):

    def _make_client(self, max_retries=3, retry_on_status=None):
        cfg = Configuration()
        cfg.max_retries = max_retries
        if retry_on_status is not None:
            cfg.retry_on_status = retry_on_status
        return RESTClientObject(cfg)

    async def test_no_retry_on_success(self):
        client = self._make_client()
        ok_response = _make_response(200)
        client.pool_manager = MagicMock()
        client.pool_manager.request = AsyncMock(return_value=ok_response)

        result = await client.request("GET", "http://example.com/")

        self.assertEqual(result.status, 200)
        self.assertEqual(client.pool_manager.request.call_count, 1)

    async def test_retries_on_503_then_succeeds(self):
        client = self._make_client(max_retries=3)
        responses = [
            _make_response(503),
            _make_response(503),
            _make_response(200),
        ]
        client.pool_manager = MagicMock()
        client.pool_manager.request = AsyncMock(side_effect=responses)

        with patch("asyncio.sleep", new_callable=AsyncMock) as mock_sleep:
            result = await client.request("GET", "http://example.com/")

        self.assertEqual(result.status, 200)
        self.assertEqual(client.pool_manager.request.call_count, 3)
        self.assertEqual(mock_sleep.call_count, 2)

    async def test_exhausts_retries_and_returns_last_response(self):
        client = self._make_client(max_retries=2)
        client.pool_manager = MagicMock()
        client.pool_manager.request = AsyncMock(return_value=_make_response(503))

        with patch("asyncio.sleep", new_callable=AsyncMock):
            result = await client.request("GET", "http://example.com/")

        self.assertEqual(result.status, 503)
        self.assertEqual(client.pool_manager.request.call_count, 3)  # initial + 2 retries

    async def test_retry_after_header_respected(self):
        client = self._make_client(max_retries=1)
        responses = [
            _make_response(503, headers={"retry-after": "5"}),
            _make_response(200),
        ]
        client.pool_manager = MagicMock()
        client.pool_manager.request = AsyncMock(side_effect=responses)

        with patch("asyncio.sleep", new_callable=AsyncMock) as mock_sleep:
            await client.request("GET", "http://example.com/")

        mock_sleep.assert_awaited_once_with(5.0)

    async def test_retry_after_invalid_value_falls_back_to_backoff(self):
        client = self._make_client(max_retries=1)
        responses = [
            _make_response(429, headers={"retry-after": "not-a-number"}),
            _make_response(200),
        ]
        client.pool_manager = MagicMock()
        client.pool_manager.request = AsyncMock(side_effect=responses)

        with patch("asyncio.sleep", new_callable=AsyncMock) as mock_sleep:
            await client.request("GET", "http://example.com/")

        delay = mock_sleep.call_args[0][0]
        self.assertGreaterEqual(delay, 1.0)  # 2^0 + jitter >= 1

    async def test_no_retry_on_non_retryable_status(self):
        client = self._make_client(max_retries=3)
        client.pool_manager = MagicMock()
        client.pool_manager.request = AsyncMock(return_value=_make_response(404))

        with patch("asyncio.sleep", new_callable=AsyncMock) as mock_sleep:
            result = await client.request("GET", "http://example.com/")

        self.assertEqual(result.status, 404)
        self.assertEqual(client.pool_manager.request.call_count, 1)
        mock_sleep.assert_not_called()

    async def test_custom_retry_on_status(self):
        client = self._make_client(max_retries=2, retry_on_status=[503])
        responses = [_make_response(502), _make_response(200)]
        client.pool_manager = MagicMock()
        client.pool_manager.request = AsyncMock(side_effect=responses)

        with patch("asyncio.sleep", new_callable=AsyncMock) as mock_sleep:
            result = await client.request("GET", "http://example.com/")

        # 502 is not in retry_on_status=[503], so no retry
        self.assertEqual(result.status, 502)
        mock_sleep.assert_not_called()

    async def test_zero_retries(self):
        client = self._make_client(max_retries=0)
        client.pool_manager = MagicMock()
        client.pool_manager.request = AsyncMock(return_value=_make_response(503))

        with patch("asyncio.sleep", new_callable=AsyncMock) as mock_sleep:
            result = await client.request("GET", "http://example.com/")

        self.assertEqual(result.status, 503)
        self.assertEqual(client.pool_manager.request.call_count, 1)
        mock_sleep.assert_not_called()


if __name__ == "__main__":
    unittest.main()
