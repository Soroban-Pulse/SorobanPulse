#!/usr/bin/env node
// Issue #1114: regenerate SorobanPulse brand assets from the master SVG mark.
//
// Usage (from the repo root):
//   npm i --no-save @resvg/resvg-js opentype.js @fontsource/inter
//   node scripts/build-brand-assets.mjs
//
// Outputs into docs/assets/brand/ (wordmark + lockups, PNG sizes, favicon set)
// and vscode-extension/media/icon.png (128×128 Marketplace icon).

import { readFileSync, writeFileSync, mkdirSync, copyFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Resvg } from '@resvg/resvg-js';
import opentype from 'opentype.js';

const require = createRequire(import.meta.url);
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const brand = join(root, 'docs/assets/brand');
const png = join(brand, 'png');
const favicon = join(brand, 'favicon');
mkdirSync(png, { recursive: true });
mkdirSync(favicon, { recursive: true });

const INK = '#0F1222';
const PAPER = '#FFFFFF';
const ACCENT = '#5B3DF5';

const fontDir = dirname(require.resolve('@fontsource/inter/package.json')) + '/files';
const loadFont = (w) => {
  const buf = readFileSync(`${fontDir}/inter-latin-${w}-normal.woff`);
  return opentype.parse(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength));
};
const medium = loadFont(500);
const bold = loadFont(700);

// Outline text so the wordmark renders identically without the font installed.
// Glyphs are laid out by hand (advance + kerning) because opentype.js does not
// support every GSUB lookup in Inter and the wordmark needs no shaping.
function outline(font, text, x, baseline, size, letterSpacing = 0) {
  const scale = size / font.unitsPerEm;
  const glyphs = [...text].map((c) => font.charToGlyph(c));
  let cursor = x;
  const parts = glyphs.map((g, i) => {
    const d = g.getPath(cursor, baseline, size).toPathData(2);
    cursor += g.advanceWidth * scale + letterSpacing;
    if (glyphs[i + 1]) cursor += font.getKerningValue(g, glyphs[i + 1]) * scale;
    return d;
  });
  return { d: parts.join(''), width: cursor - letterSpacing - x };
}

function wordmark(x, baseline, size, colors) {
  const a = outline(medium, 'Soroban', x, baseline, size, -0.02 * size);
  const b = outline(bold, 'Pulse', x + a.width + 0.04 * size, baseline, size, -0.02 * size);
  return {
    svg: `<path d="${a.d}" fill="${colors.soroban}"/><path d="${b.d}" fill="${colors.pulse}"/>`,
    width: a.width + 0.04 * size + b.width,
  };
}

const markSvg = readFileSync(join(brand, 'logo-mark.svg'), 'utf8');
const markInner = markSvg.replace(/^[\s\S]*?<svg[^>]*>/, '').replace(/<\/svg>\s*$/, '');

// Wordmark alone.
for (const [name, colors] of [
  ['wordmark.svg', { soroban: INK, pulse: ACCENT }],
  ['wordmark-dark.svg', { soroban: PAPER, pulse: '#A594FF' }],
]) {
  const w = wordmark(0, 38, 48, colors);
  const width = Math.ceil(w.width);
  writeFileSync(
    join(brand, name),
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -2 ${width} 52" width="${width}" height="52" role="img" aria-label="SorobanPulse">${w.svg}</svg>\n`,
  );
}

// Horizontal lockup: mark + wordmark. Gap = 0.25 × mark height (see usage rules).
for (const [name, colors] of [
  ['logo-horizontal.svg', { soroban: INK, pulse: ACCENT }],
  ['logo-horizontal-dark.svg', { soroban: PAPER, pulse: '#A594FF' }],
]) {
  const w = wordmark(80, 44, 40, colors);
  const width = Math.ceil(80 + w.width + 2);
  writeFileSync(
    join(brand, name),
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} 64" width="${width}" height="64" role="img" aria-label="SorobanPulse">` +
      `<g>${markInner}</g>${w.svg}</svg>\n`,
  );
}

// Stacked lockup for square placements.
{
  const w = wordmark(0, 0, 30, { soroban: INK, pulse: ACCENT });
  const width = Math.ceil(Math.max(w.width, 96)) + 8;
  const markX = (width - 96) / 2;
  const again = wordmark((width - w.width) / 2, 136, 30, { soroban: INK, pulse: ACCENT });
  writeFileSync(
    join(brand, 'logo-stacked.svg'),
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} 146" width="${width}" height="146" role="img" aria-label="SorobanPulse">` +
      `<g transform="translate(${markX} 0) scale(1.5)">${markInner}</g>${again.svg}</svg>\n`,
  );
}

const render = (svg, size) =>
  new Resvg(svg, { fitTo: { mode: 'width', value: size }, background: 'rgba(0,0,0,0)' }).render().asPng();

const sizes = [16, 24, 32, 48, 64, 128, 180, 192, 256, 512];
const pngs = {};
for (const s of sizes) {
  pngs[s] = render(markSvg, s);
  writeFileSync(join(png, `logo-mark-${s}.png`), pngs[s]);
}
writeFileSync(join(png, 'logo-horizontal-1024.png'), render(readFileSync(join(brand, 'logo-horizontal.svg'), 'utf8'), 1024));
writeFileSync(join(png, 'logo-horizontal-dark-1024.png'), render(readFileSync(join(brand, 'logo-horizontal-dark.svg'), 'utf8'), 1024));

// ICO containing PNG-encoded 16/32/48 images (supported by every modern browser).
function ico(images) {
  const header = Buffer.alloc(6 + 16 * images.length);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);
  let offset = header.length;
  images.forEach(({ size, data }, i) => {
    const e = 6 + 16 * i;
    header.writeUInt8(size >= 256 ? 0 : size, e);
    header.writeUInt8(size >= 256 ? 0 : size, e + 1);
    header.writeUInt16LE(1, e + 4);
    header.writeUInt16LE(32, e + 6);
    header.writeUInt32LE(data.length, e + 8);
    header.writeUInt32LE(offset, e + 12);
    offset += data.length;
  });
  return Buffer.concat([header, ...images.map((i) => i.data)]);
}

writeFileSync(join(favicon, 'favicon.ico'), ico([16, 32, 48].map((size) => ({ size, data: pngs[size] }))));
copyFileSync(join(brand, 'logo-mark.svg'), join(favicon, 'favicon.svg'));
writeFileSync(join(favicon, 'apple-touch-icon.png'), pngs[180]);
writeFileSync(join(favicon, 'icon-192.png'), pngs[192]);
writeFileSync(join(favicon, 'icon-512.png'), pngs[512]);
writeFileSync(
  join(favicon, 'site.webmanifest'),
  JSON.stringify(
    {
      name: 'SorobanPulse',
      short_name: 'Pulse',
      icons: [
        { src: 'icon-192.png', sizes: '192x192', type: 'image/png' },
        { src: 'icon-512.png', sizes: '512x512', type: 'image/png' },
      ],
      theme_color: ACCENT,
      background_color: '#0F1222',
      display: 'standalone',
    },
    null,
    2,
  ) + '\n',
);

// VS Code Marketplace icon (128×128) — referenced by vscode-extension/package.json.
writeFileSync(join(root, 'vscode-extension/media/icon.png'), pngs[128]);

console.log('Brand assets written to docs/assets/brand/ and vscode-extension/media/icon.png');
