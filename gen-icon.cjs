const sharp = require('sharp');

const SKY = '#38bdf8';
const DARK = '#0c4a6e';
const WHITE = '#ffffff';

async function genIcon(size, filename) {
  const cx = size / 2, cy = size / 2;
  const r = size * 0.5;

  // Build SVG with code + cloud theme
  const svg = `
<svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:#0ea5e9"/>
      <stop offset="100%" style="stop-color:#0369a1"/>
    </linearGradient>
    <linearGradient id="cloud" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:rgba(255,255,255,0.9)"/>
      <stop offset="100%" style="stop-color:rgba(255,255,255,0.7)"/>
    </linearGradient>
  </defs>

  <!-- Background circle -->
  <circle cx="${cx}" cy="${cy}" r="${r}" fill="url(#bg)"/>

  <!-- Code bracket <  (left) -->
  <polyline points="${size*0.26},${size*0.60} ${size*0.34},${size*0.40} ${size*0.26},${size*0.40}"
    fill="none" stroke="white" stroke-width="${size*0.045}" stroke-linecap="round" stroke-linejoin="round"/>

  <!-- Code bracket >  (right) -->
  <polyline points="${size*0.74},${size*0.60} ${size*0.66},${size*0.40} ${size*0.74},${size*0.40}"
    fill="none" stroke="white" stroke-width="${size*0.045}" stroke-linecap="round" stroke-linejoin="round"/>

  <!-- Slash / in center (the / in </>) -->
  <line x1="${size*0.44}" y1="${size*0.36}" x2="${size*0.56}" y2="${size*0.64}"
    stroke="white" stroke-width="${size*0.045}" stroke-linecap="round"/>

  <!-- Small cloud accent at bottom -->
  <ellipse cx="${cx}" cy="${size*0.78}" rx="${size*0.22}" ry="${size*0.10}" fill="rgba(255,255,255,0.25)"/>
  <ellipse cx="${cx-size*0.10}" cy="${size*0.76}" rx="${size*0.12}" ry="${size*0.08}" fill="rgba(255,255,255,0.25)"/>
  <ellipse cx="${cx+size*0.10}" cy="${size*0.76}" rx="${size*0.12}" ry="${size*0.08}" fill="rgba(255,255,255,0.25)"/>
</svg>`;

  const buf = Buffer.from(svg);
  await sharp(buf, { density: 300 })
    .resize(size, size)
    .png()
    .toFile(filename);

  // Re-read and ensure RGBA
  const buf2 = await sharp(filename)
    .raw()
    .toBuffer({ resolveWithObject: true })
    .then(({ data, info }) =>
      sharp(data, {
        raw: { width: info.width, height: info.height, channels: info.channels }
      }).png().toBuffer()
    );

  await sharp(buf2).toFile(filename);
  console.log(`Generated: ${filename}`);
}

async function main() {
  await genIcon(32,  'src-tauri/icons/32x32.png');
  await genIcon(128, 'src-tauri/icons/128x128.png');
  await genIcon(256, 'src-tauri/icons/128x128@2x.png');
  await genIcon(512, 'src-tauri/icons/256x256@2x.png');
  console.log('All icons generated.');
}

main().catch(console.error);