#!/usr/bin/env node
/**
 * Icon generator script for Ondo
 * Generates PNG icons from a base image or creates placeholder icons
 *
 * Run: node scripts/generate-icons.js
 */

const fs = require('fs');
const path = require('path');

const iconsDir = path.join(__dirname, '..', 'src-tauri', 'icons');

// Simple 1x1 cyan PNG (placeholder)
const createPlaceholderPng = (size) => {
  // PNG header + IHDR + IDAT + IEND (minimal valid PNG)
  // This creates a small cyan colored PNG

  const width = size;
  const height = size;

  // PNG signature
  const signature = Buffer.from([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

  // IHDR chunk
  const ihdrData = Buffer.alloc(13);
  ihdrData.writeUInt32BE(width, 0);
  ihdrData.writeUInt32BE(height, 4);
  ihdrData[8] = 8; // bit depth
  ihdrData[9] = 6; // color type (RGBA)
  ihdrData[10] = 0; // compression
  ihdrData[11] = 0; // filter
  ihdrData[12] = 0; // interlace

  const ihdr = createChunk('IHDR', ihdrData);

  // Create raw image data (cyan color with alpha)
  const rawData = [];
  for (let y = 0; y < height; y++) {
    rawData.push(0); // filter byte
    for (let x = 0; x < width; x++) {
      // Create a gradient effect
      const centerX = width / 2;
      const centerY = height / 2;
      const dist = Math.sqrt((x - centerX) ** 2 + (y - centerY) ** 2);
      const maxDist = Math.sqrt(centerX ** 2 + centerY ** 2);
      const factor = 1 - (dist / maxDist) * 0.5;

      rawData.push(Math.floor(10 * factor)); // R
      rawData.push(Math.floor(212 * factor)); // G
      rawData.push(Math.floor(255 * factor)); // B
      rawData.push(255); // A (fully opaque)
    }
  }

  // Compress with zlib (deflate)
  const zlib = require('zlib');
  const compressed = zlib.deflateSync(Buffer.from(rawData));
  const idat = createChunk('IDAT', compressed);

  // IEND chunk
  const iend = createChunk('IEND', Buffer.alloc(0));

  return Buffer.concat([signature, ihdr, idat, iend]);
};

const createChunk = (type, data) => {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);

  const typeBuffer = Buffer.from(type, 'ascii');
  const crcData = Buffer.concat([typeBuffer, data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(crcData), 0);

  return Buffer.concat([length, typeBuffer, data, crc]);
};

// CRC32 implementation
const crc32 = (data) => {
  let crc = 0xFFFFFFFF;
  const table = [];

  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let j = 0; j < 8; j++) {
      c = (c & 1) ? (0xEDB88320 ^ (c >>> 1)) : (c >>> 1);
    }
    table[i] = c;
  }

  for (let i = 0; i < data.length; i++) {
    crc = table[(crc ^ data[i]) & 0xFF] ^ (crc >>> 8);
  }

  return (crc ^ 0xFFFFFFFF) >>> 0;
};

// Ensure icons directory exists
if (!fs.existsSync(iconsDir)) {
  fs.mkdirSync(iconsDir, { recursive: true });
}

// Generate icons
const sizes = [
  { name: '32x32.png', size: 32 },
  { name: '128x128.png', size: 128 },
  { name: '128x128@2x.png', size: 256 },
  { name: 'icon.png', size: 256 },
];

console.log('Generating placeholder icons...');

sizes.forEach(({ name, size }) => {
  const iconPath = path.join(iconsDir, name);
  const png = createPlaceholderPng(size);
  fs.writeFileSync(iconPath, png);
  console.log(`  Created ${name} (${size}x${size})`);
});

// Create a simple .ico file (Windows icon)
const createIco = () => {
  const size = 32;
  const png = createPlaceholderPng(size);

  // ICO header
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // Reserved
  header.writeUInt16LE(1, 2); // Type (1 = ICO)
  header.writeUInt16LE(1, 4); // Number of images

  // ICO directory entry
  const entry = Buffer.alloc(16);
  entry[0] = size; // Width
  entry[1] = size; // Height
  entry[2] = 0; // Color palette
  entry[3] = 0; // Reserved
  entry.writeUInt16LE(1, 4); // Color planes
  entry.writeUInt16LE(32, 6); // Bits per pixel
  entry.writeUInt32LE(png.length, 8); // Size of image data
  entry.writeUInt32LE(22, 12); // Offset to image data (6 + 16)

  return Buffer.concat([header, entry, png]);
};

const icoPath = path.join(iconsDir, 'icon.ico');
fs.writeFileSync(icoPath, createIco());
console.log('  Created icon.ico');

// Create placeholder .icns (macOS - just copy PNG for now)
const icnsPath = path.join(iconsDir, 'icon.icns');
fs.copyFileSync(path.join(iconsDir, '128x128@2x.png'), icnsPath);
console.log('  Created icon.icns (placeholder)');

console.log('\nIcon generation complete!');
console.log('Note: For production, replace with proper icons using a design tool.');
