const sharp = require('sharp');
const path = require('path');
const fs = require('fs');

const ICON_DIR = path.join(__dirname, '..', 'src-tauri', 'icons');
fs.mkdirSync(ICON_DIR, { recursive: true });

// Accent color: amber/orange - matches the app's theme accent
const ACCENT = [240, 165, 0];       // #f0a500
const BG_DARK = [13, 17, 23];       // #0d1117 (app background)
const BG_LIGHT = [255, 255, 255];

async function genIcon(size, outputPath) {
  // Create SVG: rounded rect background + code document icon
  const svg = `<svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:#1a1f2e;stop-opacity:1" />
      <stop offset="100%" style="stop-color:#0d1117;stop-opacity:1" />
    </linearGradient>
  </defs>
  <!-- Background rounded rect -->
  <rect width="${size}" height="${size}" rx="${Math.round(size * 0.22)}" ry="${Math.round(size * 0.22)}" fill="url(#bg)"/>
  <!-- Code document shape -->
  <g transform="translate(${size * 0.18}, ${size * 0.16})">
    <!-- Document body -->
    <rect width="${size * 0.64}" height="${size * 0.68}" rx="${Math.round(size * 0.07)}" fill="#f0a500" opacity="0.15"/>
    <rect width="${size * 0.64}" height="${size * 0.68}" rx="${Math.round(size * 0.07)}" fill="none" stroke="#f0a500" stroke-width="${Math.max(1, size * 0.025)}"/>
    <!-- Corner fold -->
    <path d="M${Math.round(size * 0.64 - size * 0.16)} ${size * 0.68}
             L${Math.round(size * 0.64)} ${Math.round(size * 0.68 - size * 0.16)}
             L${Math.round(size * 0.64)} ${size * 0.68}
             Z"
          fill="#f0a500" opacity="0.3"/>
    <!-- Fold line -->
    <line
      x1="${Math.round(size * 0.64 - size * 0.16)}" y1="${size * 0.68}"
      x2="${size * 0.64}" y2="${Math.round(size * 0.68 - size * 0.16)}"
      stroke="#f0a500" stroke-width="${Math.max(1, size * 0.025)}" opacity="0.6"/>

    <!-- Code brackets < /> -->
    <text x="${size * 0.32}" y="${size * 0.44}"
      font-family="monospace, Courier New, Consolas"
      font-size="${Math.round(size * 0.20)}"
      font-weight="bold"
      fill="#f0a500"
      text-anchor="middle"
      dominant-baseline="middle">&lt;/&gt;</text>
  </g>
  <!-- Accent dot / gist star indicator -->
  <circle cx="${Math.round(size * 0.82)}" cy="${Math.round(size * 0.82)}" r="${Math.round(size * 0.07)}" fill="#f0a500"/>
</svg>`;

  await sharp(Buffer.from(svg))
    .resize(size, size)
    .png()
    .toFile(outputPath);

  console.log(`Generated: ${outputPath}`);
}

async function genIco(inputPng) {
  // Generate ICO from the 256x256 PNG
  // For Windows ICO, we need multiple sizes
  const sizes = [16, 32, 48, 64, 128, 256];
  const images = [];

  for (const size of sizes) {
    const buf = await sharp(inputPNG256)
      .resize(size, size)
      .png()
      .toBuffer();
    images.push({ size, data: buf });
  }

  // Simple ICO format: header + directory entries + image data
  // ICO header: 6 bytes
  // Directory: 16 bytes per image
  // Then PNG data

  const headerSize = 6;
  const dirEntrySize = 16;
  const numImages = images.length;

  let dataOffset = headerSize + numImages * dirEntrySize;
  const dirEntries = [];
  const imageBufs = [];

  for (const img of images) {
    const w = img.size >= 256 ? 0 : img.size;
    const h = img.size >= 256 ? 0 : img.size;
    const entry = Buffer.alloc(16);
    entry.writeUInt8(w, 0);         // width
    entry.writeUInt8(h, 1);         // height
    entry.writeUInt8(0, 2);          // color palette
    entry.writeUInt8(0, 3);          // reserved
    entry.writeUInt16LE(1, 4);       // color planes
    entry.writeUInt16LE(32, 6);     // bits per pixel
    entry.writeUInt32LE(img.data.length, 8);  // size of image data
    entry.writeUInt32LE(dataOffset, 12);       // offset
    dataOffset += img.data.length;
    dirEntries.push(entry);
    imageBufs.push(img.data);
  }

  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);           // reserved
  header.writeUInt16LE(1, 2);           // type: ICO
  header.writeUInt16LE(numImages, 4);  // number of images

  const ico = Buffer.concat([header, ...dirEntries, ...imageBufs]);
  return ico;
}

async function main() {
  // Generate PNGs
  const sizes = [
    { size: 32,  file: '32x32.png' },
    { size: 128, file: '128x128.png' },
    { size: 256, file: '128x128@2x.png' },
    { size: 256, file: '256x256@2x.png' },
  ];

  let png256Path;
  for (const { size, file } of sizes) {
    const outPath = path.join(ICON_DIR, file);
    await genIcon(size, outPath);
    if (size === 256 && file === '128x128@2x.png') {
      png256Path = outPath;
    }
  }

  // Generate ICO
  const png256 = await sharp(path.join(ICON_DIR, '128x128@2x.png'))
    .resize(256, 256)
    .png()
    .toBuffer();

  const icoBuf = await genIcoFromPng(png256);
  fs.writeFileSync(path.join(ICON_DIR, 'icon.ico'), icoBuf);
  console.log('Generated: icon.ico');

  // Generate ICNS placeholder (macOS) - just copy the 256 PNG as a fallback
  // Real ICNS would need a dedicated tool, use PNG fallback
  const icnsPng = await sharp(path.join(ICON_DIR, '128x128@2x.png'))
    .resize(512, 512)
    .png()
    .toBuffer();
  fs.writeFileSync(path.join(ICON_DIR, 'icon.icns'), icnsPng);
  console.log('Generated: icon.icns (PNG fallback)');
}

async function genIcoFromPng(pngData) {
  const sizes = [16, 32, 48, 256];
  const images = [];

  const inputSharp = sharp(pngData);

  for (const size of sizes) {
    const buf = await sharp(pngData).resize(size, size).png().toBuffer();
    images.push({ size, data: buf });
  }

  const headerSize = 6;
  const dirEntrySize = 16;
  const numImages = images.length;
  let dataOffset = headerSize + numImages * dirEntrySize;

  const buffers = [];
  const dirEntries = [];

  for (const img of images) {
    const w = img.size >= 256 ? 0 : img.size;
    const h = img.size >= 256 ? 0 : img.size;
    const entry = Buffer.alloc(16);
    entry.writeUInt8(w, 0);
    entry.writeUInt8(h, 1);
    entry.writeUInt8(0, 2);
    entry.writeUInt8(0, 3);
    entry.writeUInt16LE(1, 4);
    entry.writeUInt16LE(32, 6);
    entry.writeUInt32LE(img.data.length, 8);
    entry.writeUInt32LE(dataOffset, 12);
    dataOffset += img.data.length;
    dirEntries.push(entry);
    buffers.push(img.data);
  }

  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(numImages, 4);

  return Buffer.concat([header, ...dirEntries, ...buffers]);
}

main().catch(console.error);
