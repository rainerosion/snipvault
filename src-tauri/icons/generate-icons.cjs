const sharp = require('sharp');
const path = require('path');
const fs = require('fs');

const srcPath = path.join(__dirname, 'icon.png');
const outDir = __dirname;

async function generateIcons() {
  const srcBuffer = fs.readFileSync(srcPath);

  const meta = await sharp(srcBuffer).metadata();
  console.log(`Source: ${meta.width}x${meta.height}`);

  // Generate PNGs at various sizes
  const sizes = [
    { size: 32, file: '32x32.png' },
    { size: 128, file: '128x128.png' },
    { size: 256, file: '128x128@2x.png' },
    { size: 256, file: '256x256@2x.png' },
  ];

  for (const { size, file } of sizes) {
    await sharp(srcBuffer)
      .resize(size, size, { fit: 'contain' })
      .png()
      .toFile(path.join(outDir, file));
    console.log(`Generated ${file} (${size}x${size})`);
  }

  // Generate ICO
  const icoSizes = [16, 32, 48, 256];
  const icoBuffers = await Promise.all(
    icoSizes.map(s =>
      sharp(srcBuffer)
        .resize(s, s, { fit: 'contain' })
        .png()
        .toBuffer()
    )
  );

  const ico = buildIco(icoBuffers, icoSizes);
  fs.writeFileSync(path.join(outDir, 'icon.ico'), ico);
  console.log('Generated icon.ico');

  console.log('All icons generated!');
}

function buildIco(pngBuffers, sizes) {
  const headerSize = 6;
  const entrySize = 16;
  const numImages = pngBuffers.length;
  let dataOffset = headerSize + entrySize * numImages;

  const header = Buffer.alloc(headerSize);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(numImages, 4);

  const entries = [];
  for (let i = 0; i < numImages; i++) {
    const size = sizes[i];
    const buf = pngBuffers[i];
    const entry = Buffer.alloc(entrySize);
    entry.writeUInt8(size === 256 ? 0 : size, 0);
    entry.writeUInt8(size === 256 ? 0 : size, 1);
    entry.writeUInt8(0, 2);
    entry.writeUInt8(0, 3);
    entry.writeUInt16LE(1, 4);
    entry.writeUInt16LE(32, 6);
    entry.writeUInt32LE(buf.length, 8);
    entry.writeUInt32LE(dataOffset, 12);
    entries.push(entry);
    dataOffset += buf.length;
  }

  return Buffer.concat([header, ...entries, ...pngBuffers]);
}

generateIcons().catch(console.error);
