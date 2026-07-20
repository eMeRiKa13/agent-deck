import { deflateSync } from 'node:zlib'
import { writeFileSync } from 'node:fs'

const size = 512
const pixels = Buffer.alloc(size * size * 4)

function setPixel(x, y, [red, green, blue, alpha]) {
  if (x < 0 || y < 0 || x >= size || y >= size) return
  const offset = (y * size + x) * 4
  pixels[offset] = red
  pixels[offset + 1] = green
  pixels[offset + 2] = blue
  pixels[offset + 3] = alpha
}

function fillRoundedRect(x, y, width, height, radius, color) {
  for (let py = y; py < y + height; py += 1) {
    for (let px = x; px < x + width; px += 1) {
      const nearestX = Math.max(x + radius, Math.min(px, x + width - radius - 1))
      const nearestY = Math.max(y + radius, Math.min(py, y + height - radius - 1))
      const dx = px - nearestX
      const dy = py - nearestY
      if (dx * dx + dy * dy <= radius * radius) setPixel(px, py, color)
    }
  }
}

function fillRect(x, y, width, height, color) {
  for (let py = y; py < y + height; py += 1) {
    for (let px = x; px < x + width; px += 1) setPixel(px, py, color)
  }
}

function fillCircle(centerX, centerY, radius, color) {
  for (let y = centerY - radius; y <= centerY + radius; y += 1) {
    for (let x = centerX - radius; x <= centerX + radius; x += 1) {
      const dx = x - centerX
      const dy = y - centerY
      if (dx * dx + dy * dy <= radius * radius) setPixel(x, y, color)
    }
  }
}

fillRoundedRect(0, 0, size, size, 120, [23, 23, 23, 255])
fillRect(157, 162, 198, 46, [245, 245, 244, 255])
fillRect(157, 233, 128, 46, [245, 245, 244, 255])
fillRect(157, 304, 198, 46, [245, 245, 244, 255])
fillCircle(325, 256, 30, [34, 197, 94, 255])

const scanlines = Buffer.alloc((size * 4 + 1) * size)
for (let row = 0; row < size; row += 1) {
  const rowOffset = row * (size * 4 + 1)
  scanlines[rowOffset] = 0
  pixels.copy(scanlines, rowOffset + 1, row * size * 4, (row + 1) * size * 4)
}

const crcTable = Array.from({ length: 256 }, (_, index) => {
  let value = index
  for (let bit = 0; bit < 8; bit += 1) value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1
  return value >>> 0
})

function crc32(buffer) {
  let crc = 0xffffffff
  for (const byte of buffer) crc = crcTable[(crc ^ byte) & 0xff] ^ (crc >>> 8)
  return (crc ^ 0xffffffff) >>> 0
}

function chunk(type, data) {
  const name = Buffer.from(type)
  const length = Buffer.alloc(4)
  const checksum = Buffer.alloc(4)
  length.writeUInt32BE(data.length)
  checksum.writeUInt32BE(crc32(Buffer.concat([name, data])))
  return Buffer.concat([length, name, data, checksum])
}

const header = Buffer.alloc(13)
header.writeUInt32BE(size, 0)
header.writeUInt32BE(size, 4)
header[8] = 8
header[9] = 6

const png = Buffer.concat([
  Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
  chunk('IHDR', header),
  chunk('IDAT', deflateSync(scanlines)),
  chunk('IEND', Buffer.alloc(0)),
])

writeFileSync(new URL('../src-tauri/icons/icon.png', import.meta.url), png)
