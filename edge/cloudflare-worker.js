/**
 * Cloudflare Worker for SorobanPulse Edge Computing
 *
 * ES module worker that proxies requests to the SorobanPulse API origin
 * (configured via the ORIGIN_URL var in wrangler.toml) and applies a small,
 * conservative edge cache for public, unauthenticated GETs.
 *
 * Security notes:
 *  - Requests carrying Authorization or X-API-Key are NEVER cached and are
 *    never served from the shared edge cache, so responses cannot leak
 *    between tenants.
 *  - Streaming requests (SSE via Accept: text/event-stream, and WebSocket
 *    upgrades) bypass the cache entirely and are passed through untouched.
 */

const PUBLIC_CACHEABLE_PATHS = new Set(['/v1/meta']);

const PUBLIC_CACHE_TTL_SECONDS = 60;

const AUTH_HEADERS = ['authorization', 'x-api-key'];

const CORS_HEADERS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type, Authorization, X-API-Key',
};

export default {
  async fetch(request, env, ctx) {
    return handleRequest(request, env, ctx);
  },
};

async function handleRequest(request, env, ctx) {
  const url = new URL(request.url);

  // Handle CORS preflight without touching the origin.
  if (request.method === 'OPTIONS') {
    return new Response(null, { status: 204, headers: CORS_HEADERS });
  }

  const originUrl = resolveOriginUrl(env);
  if (!originUrl) {
    return addCORSHeaders(
      new Response(
        JSON.stringify({ error: 'Edge origin is not configured (ORIGIN_URL)' }),
        { status: 500, headers: { 'Content-Type': 'application/json' } },
      ),
    );
  }

  const isStreaming = isStreamingRequest(request);
  const isAuthenticated = hasAuthHeaders(request);
  const canUseCache =
    !isStreaming &&
    !isAuthenticated &&
    request.method === 'GET' &&
    isPublicCacheablePath(url.pathname);

  const cache = caches.default;
  const cacheKey = canUseCache ? buildCacheKey(url, originUrl) : null;

  if (canUseCache) {
    const cached = await cache.match(cacheKey);
    if (cached) {
      return addCORSHeaders(cached);
    }
  }

  const originRequest = buildOriginRequest(request, url, originUrl, isStreaming);

  let response;
  try {
    response = await fetch(originRequest);
  } catch (err) {
    return addCORSHeaders(
      new Response(
        JSON.stringify({ error: 'Upstream request failed', detail: String(err) }),
        { status: 502, headers: { 'Content-Type': 'application/json' } },
      ),
    );
  }

  if (canUseCache && response.ok) {
    const cacheResponse = response.clone();
    const headers = new Headers(cacheResponse.headers);
    headers.set('Cache-Control', `public, max-age=${PUBLIC_CACHE_TTL_SECONDS}`);
    const storable = new Response(cacheResponse.body, {
      status: cacheResponse.status,
      statusText: cacheResponse.statusText,
      headers,
    });
    const putPromise = cache.put(cacheKey, storable);
    if (ctx && typeof ctx.waitUntil === 'function') {
      ctx.waitUntil(putPromise);
    } else {
      await putPromise;
    }
  }

  return addCORSHeaders(response);
}

function resolveOriginUrl(env) {
  const raw = env && (env.ORIGIN_URL || env.originUrl);
  if (!raw || typeof raw !== 'string') {
    return null;
  }
  const trimmed = raw.trim().replace(/\/+$/, '');
  if (!trimmed) {
    return null;
  }
  try {
    const parsed = new URL(trimmed);
    if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:') {
      return null;
    }
    return trimmed;
  } catch {
    return null;
  }
}

function hasAuthHeaders(request) {
  for (const name of AUTH_HEADERS) {
    const value = request.headers.get(name);
    if (value && value.trim() !== '') {
      return true;
    }
  }
  return false;
}

function isStreamingRequest(request) {
  const upgrade = request.headers.get('Upgrade');
  if (upgrade && upgrade.toLowerCase() === 'websocket') {
    return true;
  }
  const accept = request.headers.get('Accept') || '';
  if (accept.toLowerCase().includes('text/event-stream')) {
    return true;
  }
  return false;
}

function isPublicCacheablePath(pathname) {
  if (PUBLIC_CACHEABLE_PATHS.has(pathname)) {
    return true;
  }
  // Allow trailing-slash variants of the public meta endpoint.
  const normalized = pathname.replace(/\/+$/, '');
  return PUBLIC_CACHEABLE_PATHS.has(normalized);
}

function buildCacheKey(url, originUrl) {
  // Cache key is scoped to the origin and the request path/query only.
  // Authenticated requests never reach this function.
  const key = new URL(url.pathname + url.search, originUrl);
  return new Request(key.toString(), { method: 'GET' });
}

function buildOriginRequest(request, url, originUrl, isStreaming) {
  const target = new URL(url.pathname + url.search, originUrl);

  const headers = new Headers(request.headers);
  // Preserve the original Host for the origin.
  headers.delete('host');

  const init = {
    method: request.method,
    headers,
    redirect: 'manual',
  };

  if (request.method !== 'GET' && request.method !== 'HEAD') {
    init.body = request.body;
  }

  if (isStreaming) {
    // Ensure streaming responses are not buffered or transformed.
    headers.set('Accept-Encoding', 'identity');
  }

  return new Request(target.toString(), init);
}

function addCORSHeaders(response) {
  const headers = new Headers(response.headers);
  for (const [key, value] of Object.entries(CORS_HEADERS)) {
    headers.set(key, value);
  }
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}