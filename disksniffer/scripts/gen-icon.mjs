import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SIZE = 1024;
const SAMPLES = 3;
const here = dirname(fileURLToPath(import.meta.url));

let crcTable = null;
function crc32(buffer) {
  if (!crcTable) {
    crcTable = new Int32Array(256);
    for (let n = 0; n < 256; n += 1) {
      let c = n;
      for (let k = 0; k < 8; k += 1) {
        c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      }
      crcTable[n] = c;
    }
  }
  let crc = -1;
  for (let i = 0; i < buffer.length; i += 1) {
    crc = crcTable[(crc ^ buffer[i]) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ -1) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const typeBuffer = Buffer.from(type, "ascii");
  const body = Buffer.concat([typeBuffer, data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([length, body, crc]);
}

function encodePng(width, height, rgba) {
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y += 1) {
    raw[y * (stride + 1)] = 0;
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  return Buffer.concat([
    signature,
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function hex(value) {
  const clean = value.replace("#", "");
  return [parseInt(clean.slice(0, 2), 16), parseInt(clean.slice(2, 4), 16), parseInt(clean.slice(4, 6), 16)];
}

function mix(a, b, t) {
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
}

function insideRoundRect(x, y, rx, ry, rw, rh, radius) {
  if (x < rx || y < ry || x > rx + rw || y > ry + rh) return false;
  const cx = Math.min(Math.max(x, rx + radius), rx + rw - radius);
  const cy = Math.min(Math.max(y, ry + radius), ry + rh - radius);
  const dx = x - cx;
  const dy = y - cy;
  return dx * dx + dy * dy <= radius * radius;
}

const MARGIN = 118;
const GAP = 18;
const WIDTH = SIZE - MARGIN * 2;
const LEFT_W = Math.round(WIDTH * 0.52);
const RIGHT_X = MARGIN + LEFT_W + GAP;
const RIGHT_W = WIDTH - LEFT_W - GAP;
const TOP_H = Math.round(WIDTH * 0.42);
const MID_Y = MARGIN + TOP_H + GAP;
const MID_H = Math.round(WIDTH * 0.23);
const BOT_Y = MID_Y + MID_H + GAP;

const blocks = [
  { x: MARGIN, y: MARGIN, w: LEFT_W, h: WIDTH, top: hex("#6cb4ff"), bottom: hex("#2f74d6") },
  { x: RIGHT_X, y: MARGIN, w: RIGHT_W, h: TOP_H, top: hex("#5ed3c8"), bottom: hex("#2f9a92") },
  { x: RIGHT_X, y: MID_Y, w: RIGHT_W, h: MID_H, top: hex("#f0b45f"), bottom: hex("#c9832f") },
  { x: RIGHT_X, y: BOT_Y, w: RIGHT_W, h: MARGIN + WIDTH - BOT_Y, top: hex("#f0776b"), bottom: hex("#c8483d") },
];

const bgTop = hex("#1d2734");
const bgBottom = hex("#0a0d13");

function samplePoint(x, y) {
  if (!insideRoundRect(x, y, 0, 0, SIZE, SIZE, 216)) return null;
  let color = mix(bgTop, bgBottom, y / SIZE);
  for (const block of blocks) {
    if (insideRoundRect(x, y, block.x, block.y, block.w, block.h, 30)) {
      const t = (y - block.y) / Math.max(block.h, 1);
      color = mix(block.top, block.bottom, Math.min(Math.max(t, 0), 1));
      break;
    }
  }
  // subtle top light
  const glow = Math.max(0, 1 - Math.hypot(x - SIZE * 0.3, y - SIZE * 0.18) / (SIZE * 0.9));
  color = mix(color, [255, 255, 255], glow * 0.05);
  return color;
}

const buffer = Buffer.alloc(SIZE * SIZE * 4);
for (let y = 0; y < SIZE; y += 1) {
  for (let x = 0; x < SIZE; x += 1) {
    let r = 0;
    let g = 0;
    let b = 0;
    let a = 0;
    for (let sy = 0; sy < SAMPLES; sy += 1) {
      for (let sx = 0; sx < SAMPLES; sx += 1) {
        const px = x + (sx + 0.5) / SAMPLES;
        const py = y + (sy + 0.5) / SAMPLES;
        const color = samplePoint(px, py);
        if (color) {
          r += color[0];
          g += color[1];
          b += color[2];
          a += 255;
        }
      }
    }
    const total = SAMPLES * SAMPLES;
    const offset = (y * SIZE + x) * 4;
    if (a > 0) {
      const covered = a / total;
      buffer[offset] = Math.round(r / (a / 255));
      buffer[offset + 1] = Math.round(g / (a / 255));
      buffer[offset + 2] = Math.round(b / (a / 255));
      buffer[offset + 3] = Math.round(covered);
    }
  }
}

const output = join(here, "..", "app-icon.png");
writeFileSync(output, encodePng(SIZE, SIZE, buffer));
console.log("icon written to " + output);
