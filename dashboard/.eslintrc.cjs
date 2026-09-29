// Lint rules for the dashboard.
//
// - i18next/no-literal-string: every user-facing string in JSX must come from
//   the translation catalogue (src/locales/*.json). See web-dashboard docs.
// - jsx-a11y: catches missing labels, invalid ARIA and non-keyboard handlers.
module.exports = {
  root: true,
  parser: "@typescript-eslint/parser",
  parserOptions: {
    ecmaVersion: "latest",
    sourceType: "module",
    ecmaFeatures: { jsx: true },
  },
  plugins: ["i18next", "jsx-a11y", "react-hooks"],
  extends: ["plugin:jsx-a11y/recommended"],
  rules: {
    "i18next/no-literal-string": [
      "error",
      {
        mode: "jsx-only",
        "jsx-attributes": {
          include: ["aria-label", "aria-description", "title", "alt", "placeholder", "label"],
        },
      },
    ],
    // Scrollable regions and code blocks must be focusable so keyboard users
    // can scroll them (axe: scrollable-region-focusable).
    "jsx-a11y/no-noninteractive-tabindex": ["error", { tags: ["pre"], roles: ["region"] }],
    "react-hooks/rules-of-hooks": "error",
    "react-hooks/exhaustive-deps": "warn",
  },
  ignorePatterns: ["dist", "node_modules", "tests", "*.cjs"],
};
