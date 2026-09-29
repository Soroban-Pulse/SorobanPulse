// @ts-check
// docs/site/docusaurus.config.js
//
// SorobanPulse documentation site — Docusaurus 3 configuration.
// Design tokens are in docs/design-tokens.md; CSS overrides in src/css/custom.css.

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'SorobanPulse',
  tagline: 'Real-time Soroban smart contract event indexing for the Stellar network',
  favicon: 'img/favicon.svg',

  // Production URL — update when the site is deployed
  url: 'https://docs.sorobanpulse.dev',
  baseUrl: '/',

  organizationName: 'Soroban-Pulse',
  projectName: 'SorobanPulse',

  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  // ---------------------------------------------------------------------------
  // Plugins
  // ---------------------------------------------------------------------------
  plugins: [
    // Docs-only plugin instances for each IA section so they can live under
    // their own URL prefix and have independent sidebars.
    [
      '@docusaurus/plugin-content-docs',
      {
        id: 'guides',
        path: '../../docs',
        routeBasePath: 'guides',
        sidebarPath: require.resolve('./sidebars/guides.js'),
        // Automatically generate sidebar from folder structure as fallback
        sidebarCollapsed: false,
        editUrl:
          'https://github.com/Soroban-Pulse/SorobanPulse/edit/main/docs/',
      },
    ],
  ],

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          // The primary "Getting Started" section uses the default docs plugin
          path: '../../docs',
          routeBasePath: 'docs',
          sidebarPath: require.resolve('./sidebars/getting-started.js'),
          editUrl:
            'https://github.com/Soroban-Pulse/SorobanPulse/edit/main/docs/',
          showLastUpdateAuthor: true,
          showLastUpdateTime: true,
          // Enable Mermaid diagrams in MDX
          remarkPlugins: [],
          rehypePlugins: [],
        },
        blog: false, // No blog section
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
        sitemap: {
          changefreq: 'weekly',
          priority: 0.5,
          ignorePatterns: ['/tags/**'],
        },
      }),
    ],
  ],

  // ---------------------------------------------------------------------------
  // Mermaid diagrams
  // ---------------------------------------------------------------------------
  markdown: {
    mermaid: true,
  },
  themes: ['@docusaurus/theme-mermaid'],

  // ---------------------------------------------------------------------------
  // Theme configuration
  // ---------------------------------------------------------------------------
  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      // Social card used for og:image
      image: 'img/sorobanpulse-social-card.png',

      // Announcement bar (remove or update as needed)
      announcementBar: {
        id: 'api_v1',
        content:
          '🚀 SorobanPulse v1 API is live — unversioned routes are deprecated. <a href="/docs/api-deprecation">Migrate now →</a>',
        backgroundColor: '#1a1d2e',
        textColor: '#a78bfa',
        isCloseable: true,
      },

      // -----------------------------------------------------------------
      // Colour mode (dark default, user-switchable)
      // -----------------------------------------------------------------
      colorMode: {
        defaultMode: 'dark',
        disableSwitch: false,
        respectPrefersColorScheme: true,
      },

      // -----------------------------------------------------------------
      // Navigation bar
      // -----------------------------------------------------------------
      navbar: {
        title: 'SorobanPulse',
        logo: {
          alt: 'SorobanPulse logo',
          src: 'img/logo.svg',
          srcDark: 'img/logo-dark.svg',
          href: '/',
          target: '_self',
        },
        style: 'dark',
        hideOnScroll: false,
        items: [
          // ── Section 1: Getting Started ──────────────────────────────
          {
            type: 'doc',
            docId: 'onboarding',
            position: 'left',
            label: 'Getting Started',
          },
          // ── Section 2: Guides ───────────────────────────────────────
          {
            type: 'dropdown',
            label: 'Guides',
            position: 'left',
            items: [
              {
                label: 'API Usage',
                to: '/guides/api-guide',
              },
              {
                label: 'SDK Integration',
                to: '/guides/sdk-integration-guide',
              },
              {
                label: 'Webhooks',
                to: '/guides/webhook-verification',
              },
              {
                label: 'Subscriptions',
                to: '/guides/subscription-best-practices',
              },
              {
                label: 'Contract Events',
                to: '/guides/contract-event-schemas',
              },
              {
                label: 'Streaming (SSE)',
                to: '/guides/sse-reconnection',
              },
            ],
          },
          // ── Section 3: API Reference ─────────────────────────────────
          {
            type: 'dropdown',
            label: 'API Reference',
            position: 'left',
            items: [
              {
                label: 'Interactive API Reference',
                to: '/api-reference',
              },
              {
                label: 'Interactive Docs (Swagger)',
                href: '/docs',
              },
              {
                label: 'OpenAPI JSON',
                href: '/openapi.json',
              },
              {
                label: 'API Versioning',
                to: '/guides/api-versioning',
              },
              {
                label: 'API Changelog',
                to: '/guides/api-changelog',
              },
              {
                label: 'Rate Limiting',
                to: '/guides/rate_limiting',
              },
            ],
          },
          // ── Section 4: Operations ────────────────────────────────────
          {
            type: 'dropdown',
            label: 'Operations',
            position: 'left',
            items: [
              {
                label: 'Deployment',
                to: '/guides/deployment',
              },
              {
                label: 'Architecture',
                to: '/guides/architecture',
              },
              {
                label: 'Metrics Reference',
                to: '/guides/metrics-reference',
              },
              {
                label: 'Alerting',
                to: '/guides/alerting',
              },
              {
                label: 'Troubleshooting',
                to: '/guides/troubleshooting-guide',
              },
              {
                label: 'Runbooks',
                to: '/guides/runbooks/operator-runbook',
              },
            ],
          },
          // ── Section 5: Contributing ──────────────────────────────────
          {
            type: 'dropdown',
            label: 'Contributing',
            position: 'left',
            items: [
              {
                label: 'CONTRIBUTING.md',
                href: 'https://github.com/Soroban-Pulse/SorobanPulse/blob/main/CONTRIBUTING.md',
              },
              {
                label: 'Development Setup',
                to: '/guides/development-setup',
              },
              {
                label: 'Architecture Decisions (ADRs)',
                to: '/guides/adr/README',
              },
              {
                label: 'Design Tokens',
                to: '/guides/design-tokens',
              },
              {
                label: 'Diagram Style Guide',
                to: '/guides/diagram-style-guide',
              },
            ],
          },
          // ── Right-side items ─────────────────────────────────────────
          {
            href: 'https://github.com/Soroban-Pulse/SorobanPulse',
            position: 'right',
            className: 'navbar-github-link',
            'aria-label': 'GitHub repository',
          },
          {
            type: 'search',
            position: 'right',
          },
        ],
      },

      // -----------------------------------------------------------------
      // Footer
      // -----------------------------------------------------------------
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Docs',
            items: [
              { label: 'Getting Started', to: '/docs/onboarding' },
              { label: 'API Guide', to: '/guides/api-guide' },
              { label: 'SDK Integration', to: '/guides/sdk-integration-guide' },
              { label: 'FAQ', to: '/guides/FAQ' },
            ],
          },
          {
            title: 'API',
            items: [
              { label: 'API Reference', to: '/api-reference' },
              { label: 'Swagger UI', href: '/docs' },
              { label: 'OpenAPI JSON', href: '/openapi.json' },
              { label: 'API Changelog', to: '/guides/api-changelog' },
              { label: 'API Versioning', to: '/guides/api-versioning' },
            ],
          },
          {
            title: 'Operations',
            items: [
              { label: 'Deployment', to: '/guides/deployment' },
              { label: 'Metrics', to: '/guides/metrics-reference' },
              { label: 'Alerts', to: '/guides/alerting' },
              { label: 'Runbooks', to: '/guides/runbooks/operator-runbook' },
            ],
          },
          {
            title: 'Community',
            items: [
              {
                label: 'GitHub',
                href: 'https://github.com/Soroban-Pulse/SorobanPulse',
              },
              {
                label: 'Issues',
                href: 'https://github.com/Soroban-Pulse/SorobanPulse/issues',
              },
              {
                label: 'CONTRIBUTING.md',
                href: 'https://github.com/Soroban-Pulse/SorobanPulse/blob/main/CONTRIBUTING.md',
              },
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} SorobanPulse contributors. Built with Docusaurus.`,
      },

      // -----------------------------------------------------------------
      // Algolia DocSearch (fill in your own keys when ready)
      // -----------------------------------------------------------------
      // algolia: {
      //   appId: 'YOUR_APP_ID',
      //   apiKey: 'YOUR_SEARCH_API_KEY',
      //   indexName: 'sorobanpulse',
      //   contextualSearch: true,
      // },

      // -----------------------------------------------------------------
      // Prism syntax highlighting
      // -----------------------------------------------------------------
      prism: {
        theme: require('prism-react-renderer').themes.nightOwlLight,
        darkTheme: require('prism-react-renderer').themes.nightOwl,
        additionalLanguages: [
          'bash',
          'json',
          'toml',
          'yaml',
          'sql',
          'rust',
          'typescript',
          'python',
          'go',
          'lua',
          'docker',
        ],
      },

      // -----------------------------------------------------------------
      // Table of contents
      // -----------------------------------------------------------------
      tableOfContents: {
        minHeadingLevel: 2,
        maxHeadingLevel: 4,
      },

      // -----------------------------------------------------------------
      // Mermaid theme
      // -----------------------------------------------------------------
      mermaid: {
        theme: { light: 'default', dark: 'dark' },
        options: {
          // Global Mermaid init — see diagram-style-guide.md for per-diagram
          // %%{init} blocks that apply the SorobanPulse palette.
          fontFamily: 'Inter, system-ui, sans-serif',
          fontSize: 14,
        },
      },
    }),
};

module.exports = config;
