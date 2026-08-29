/**
 * Cloudflare Worker for SorobanPulse Edge Computing
 * Provides caching, request routing, and edge processing
 */

const CONFIG = {
  originUrl: 'https://api.soroban-pulse.example.com',
  cacheControl: {
    ledgers: 300,
    transactions: 600,
    events: 60,
  },
};

addEventListener('fetch', event => {
  event.respondWith(handleRequest(event.request));
});

async function handleRequest(request) {
  const url = new URL(request.url);
  const cache = caches.default;
  
  // Check cache first
  let response = await cache.match(request);
  if (response) {
    return addCORSHeaders(response);
  }

  // Forward to origin
  const originRequest = new Request(`${CONFIG.originUrl}${url.pathname}${url.search}`, request);
  response = await fetch(originRequest);
  
  // Cache GET requests
  if (request.method === 'GET' && response.ok) {
    const cacheResponse = response.clone();
    const ttl = getCacheTTL(url.pathname);
    if (ttl > 0) {
      const headers = new Headers(cacheResponse.headers);
      headers.set('Cache-Control', `public, max-age=${ttl}`);
      const cachedResponse = new Response(cacheResponse.body, {
        status: cacheResponse.status,
        statusText: cacheResponse.statusText,
        headers: headers
      });
      event.waitUntil(cache.put(request, cachedResponse));
    }
  }

  return addCORSHeaders(response);
}

function getCacheTTL(pathname) {
  if (pathname.includes('/ledgers')) return CONFIG.cacheControl.ledgers;
  if (pathname.includes('/transactions')) return CONFIG.cacheControl.transactions;
  if (pathname.includes('/events')) return CONFIG.cacheControl.events;
  return 0;
}

function addCORSHeaders(response) {
  const headers = new Headers(response.headers);
  headers.set('Access-Control-Allow-Origin', '*');
  headers.set('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS');
  headers.set('Access-Control-Allow-Headers', 'Content-Type, Authorization');
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: headers
  });
}
