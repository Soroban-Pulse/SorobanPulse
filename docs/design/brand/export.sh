#!/usr/bin/env bash
# Rasterise the brand SVGs. Requires rsvg-convert (librsvg) and ImageMagick.
set -euo pipefail
cd "$(dirname "$0")"
for size in 16 32 64 128 256 512; do
  rsvg-convert -w "$size" -h "$size" icon.svg -o "icon-${size}.png"
done
rsvg-convert -w 180 -h 180 icon.svg -o ../../../dashboard/public/apple-touch-icon.png
magick icon-16.png icon-32.png icon-64.png ../../../dashboard/public/favicon.ico
echo "Exported PNGs, apple-touch-icon.png and favicon.ico"
