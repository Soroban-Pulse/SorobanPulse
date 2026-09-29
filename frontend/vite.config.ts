import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// In development, API calls are proxied to the Rust server so the UI can use
// same-origin relative URLs. Override the target with VITE_API_PROXY.
const target = process.env.VITE_API_PROXY ?? "http://localhost:3000";
const proxied = ["/v1", "/health", "/healthz", "/status"];
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// The Soroban Pulse API listens on :3000 by default. Proxy API paths in dev so
// the UI can run on its own port without CORS configuration.
const apiTarget = process.env.SOROBAN_PULSE_API ?? 'http://localhost:3000';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: Object.fromEntries(proxied.map((p) => [p, { target, changeOrigin: true }])),
    proxy: {
      '/v1': apiTarget,
      '/health': apiTarget,
    },
  },
});
