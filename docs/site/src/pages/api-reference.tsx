// docs/site/src/pages/api-reference.tsx
//
// Interactive API Reference page powered by Scalar — renders the
// SorobanPulse OpenAPI spec in a polished, searchable UI.
//
// The spec is served from /openapi.json (copied from the repo root into
// docs/site/static/ by the docs.yml workflow and the local dev server).
//
// To view locally:
//   cd docs/site && npm run start
//   Then navigate to http://localhost:3000/api-reference

import React, { useEffect } from 'react';
import Layout from '@theme/Layout';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------
declare global {
  namespace JSX {
    interface IntrinsicElements {
      'rapi-doc': React.DetailedHTMLProps<
        React.HTMLAttributes<HTMLElement> & {
          'spec-url'?: string;
          theme?: string;
          'primary-color'?: string;
          'bg-color'?: string;
          'text-color'?: string;
          'nav-bg-color'?: string;
          'nav-text-color'?: string;
          'nav-accent-color'?: string;
          'font-size'?: string;
          'show-header'?: string;
          'show-info'?: string;
          'allow-authentication'?: string;
          'allow-try'?: string;
          'render-style'?: string;
          'schema-style'?: string;
          'nav-item-spacing'?: string;
        },
        HTMLElement
      >;
    }
  }
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------
export default function ApiReference(): JSX.Element {
  useEffect(() => {
    // Dynamically load RapiDoc — a lightweight, zero-dependency OpenAPI viewer
    // that works as a native web component and requires no bundler integration.
    if (!customElements.get('rapi-doc')) {
      const script = document.createElement('script');
      script.type = 'module';
      script.src =
        'https://unpkg.com/rapidoc@9.3.4/dist/rapidoc-min.js';
      script.crossOrigin = 'anonymous';
      document.head.appendChild(script);
    }
  }, []);

  return (
    <Layout
      title="API Reference"
      description="Interactive SorobanPulse REST API reference — explore all endpoints, request/response schemas, and try requests directly in your browser."
    >
      <main style={{ display: 'flex', flexDirection: 'column', flex: 1 }}>
        {/* RapiDoc web component renders the full OpenAPI spec */}
        {/* @ts-ignore — custom element not in standard HTML types */}
        <rapi-doc
          spec-url="/openapi.json"
          theme="dark"
          primary-color="#a78bfa"
          bg-color="#13141f"
          text-color="#e2e8f0"
          nav-bg-color="#1a1d2e"
          nav-text-color="#cbd5e1"
          nav-accent-color="#a78bfa"
          font-size="default"
          show-header="false"
          show-info="true"
          allow-authentication="true"
          allow-try="true"
          render-style="read"
          schema-style="table"
          nav-item-spacing="relaxed"
          style={{ flex: 1, width: '100%' }}
        />
      </main>
    </Layout>
  );
}
