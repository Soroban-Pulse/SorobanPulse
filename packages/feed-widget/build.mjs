// Builds dist/feed-widget.js (ESM) and dist/feed-widget.iife.js (classic <script>),
// emits type declarations, and enforces the 15 KB gzipped budget (issue #1111).
import { build } from 'esbuild';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { gzipSync } from 'node:zlib';

const BUDGET = 15 * 1024;
const common = { entryPoints: ['src/feed-widget.ts'], bundle: true, minify: true, target: 'es2022', legalComments: 'none' };

await build({ ...common, format: 'esm', outfile: 'dist/feed-widget.js' });
await build({ ...common, format: 'iife', globalName: 'SorobanPulseFeed', outfile: 'dist/feed-widget.iife.js' });
execFileSync('npx', ['tsc', '-p', '.'], { stdio: 'inherit' });

for (const f of ['dist/feed-widget.js', 'dist/feed-widget.iife.js']) {
  const size = gzipSync(readFileSync(f), { level: 9 }).length;
  console.log(`${f}: ${(size / 1024).toFixed(2)} KB gzipped`);
  if (size > BUDGET) {
    console.error(`${f} exceeds the ${BUDGET / 1024} KB gzipped budget`);
    process.exit(1);
  }
}
