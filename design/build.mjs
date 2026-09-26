// Issue #1113: build SorobanPulse design tokens with Style Dictionary.
//
// Sources:  tokens/base/*.json    — palette, typography, spacing, radius, motion
//           tokens/themes/*.json  — semantic colours + elevation, one file per theme
// Outputs:  build/tokens.css      — CSS custom properties (light on :root, dark overrides)
//           build/tokens.json     — flat light-theme values (for tooling / Figma import)
//           build/tokens.dark.json
//           build/tokens.js       — ES module export of the light values
//
// Generated files are committed so web/, the feed widget and the VS Code webviews
// can consume them without running this build. Re-run `npm run build` after
// editing anything under tokens/.
import StyleDictionary from 'style-dictionary';
import { fileHeader, formattedVariables } from 'style-dictionary/utils';

const PREFIX = 'sp';
const isThemed = (token) => token.filePath.includes('/themes/');

// Dark overrides apply when the user explicitly picks dark (data-theme="dark"),
// or when the OS prefers dark and no explicit light choice has been made.
StyleDictionary.registerFormat({
  name: 'css/theme-dark',
  format: async ({ dictionary, file, options }) => {
    const vars = formattedVariables({ format: 'css', dictionary, outputReferences: false });
    const indented = vars.replace(/^/gm, '  ');
    return (
      (await fileHeader({ file })) +
      `:root[data-theme="dark"] {\n${vars}\n}\n\n` +
      `@media (prefers-color-scheme: dark) {\n  :root:not([data-theme="light"]) {\n${indented}\n  }\n}\n`
    );
  },
});

const build = async (theme) => {
  const light = theme === 'light';
  const sd = new StyleDictionary({
    source: ['tokens/base/**/*.json', `tokens/themes/${theme}.json`],
    log: { verbosity: 'default' },
    platforms: {
      css: {
        transformGroup: 'css',
        prefix: PREFIX,
        buildPath: 'build/',
        files: light
          ? [{ destination: 'tokens.light.css', format: 'css/variables', options: { selector: ':root' } }]
          : [{ destination: 'tokens.dark.css', format: 'css/theme-dark', filter: isThemed }],
      },
      json: {
        transformGroup: 'js',
        buildPath: 'build/',
        files: [
          light
            ? { destination: 'tokens.json', format: 'json/nested' }
            : { destination: 'tokens.dark.json', format: 'json/nested', filter: isThemed },
        ],
      },
      ...(light && {
        js: {
          transformGroup: 'js',
          buildPath: 'build/',
          files: [{ destination: 'tokens.js', format: 'javascript/es6' }],
        },
      }),
    },
  });
  await sd.buildAllPlatforms();
};

await build('light');
await build('dark');

// Concatenate into the single stylesheet consumers import.
const { readFile, writeFile, rm } = await import('node:fs/promises');
const light = await readFile('build/tokens.light.css', 'utf8');
const dark = await readFile('build/tokens.dark.css', 'utf8');
await writeFile('build/tokens.css', `${light}\n${dark.replace(/^\/\*\*[\s\S]*?\*\/\n/, '')}`);
await rm('build/tokens.light.css');
await rm('build/tokens.dark.css');
