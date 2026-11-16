// Packs square PNGs into a Windows .ico and into a compiled resource (.res) holding the same
// icon. The linker takes a .res as an input, so the daemon's build needs no resource compiler.
// Windows reads PNG-compressed icon images at every size.
//
// Usage: node windows.mjs <out.ico> <out.res> <png>...

import { readFileSync, writeFileSync } from "node:fs";

const RT_ICON = 3;
const RT_GROUP_ICON = 14;
const ICON_GROUP_ID = 1;
// English (United States), what resource compilers default to.
const LANGUAGE = 0x0409;
// Moveable and discardable, as resource compilers mark icons; their group is pure as well.
const ICON_FLAGS = 0x1010;
const GROUP_FLAGS = 0x1030;

const [icoPath, resPath, ...pngPaths] = process.argv.slice(2);
if (!resPath || pngPaths.length === 0) {
  console.error("usage: node windows.mjs <out.ico> <out.res> <png>...");
  process.exit(2);
}

const images = pngPaths.map((path) => {
  const data = readFileSync(path);
  const size = data.readUInt32BE(16);
  if (data.readUInt32BE(20) !== size || size > 256) {
    throw new Error(`${path} isn't a square PNG of at most 256 pixels`);
  }
  return { size, data };
});

// The fields an .ico's directory entry and a group resource's entry share: size (0 means
// 256), no palette, one plane, 32 bits per pixel, and the image's length.
function entryHead(image) {
  const head = Buffer.alloc(12);
  head.writeUInt8(image.size % 256, 0);
  head.writeUInt8(image.size % 256, 1);
  head.writeUInt16LE(1, 4);
  head.writeUInt16LE(32, 6);
  head.writeUInt32LE(image.data.length, 8);
  return head;
}

function directoryHead(count) {
  const head = Buffer.alloc(6);
  head.writeUInt16LE(1, 2);
  head.writeUInt16LE(count, 4);
  return head;
}

function ico() {
  let offset = 6 + 16 * images.length;
  const entries = images.map((image) => {
    const where = Buffer.alloc(4);
    where.writeUInt32LE(offset, 0);
    offset += image.data.length;
    return Buffer.concat([entryHead(image), where]);
  });
  return Buffer.concat([directoryHead(images.length), ...entries, ...images.map((image) => image.data)]);
}

// One resource with a numeric type and name, its data padded to four bytes.
function resource(type, name, flags, data, language = LANGUAGE) {
  const header = Buffer.alloc(32);
  header.writeUInt32LE(data.length, 0);
  header.writeUInt32LE(header.length, 4);
  header.writeUInt16LE(0xffff, 8);
  header.writeUInt16LE(type, 10);
  header.writeUInt16LE(0xffff, 12);
  header.writeUInt16LE(name, 14);
  header.writeUInt16LE(flags, 20);
  header.writeUInt16LE(language, 22);
  const padding = Buffer.alloc((4 - (data.length % 4)) % 4);
  return Buffer.concat([header, data, padding]);
}

function res() {
  // A resource file starts with an empty entry that marks it as 32-bit.
  const marker = resource(0, 0, 0, Buffer.alloc(0), 0);
  const icons = images.map((image, index) => resource(RT_ICON, index + 1, ICON_FLAGS, image.data));
  const groupEntries = images.map((image, index) => {
    const id = Buffer.alloc(2);
    id.writeUInt16LE(index + 1, 0);
    return Buffer.concat([entryHead(image), id]);
  });
  const group = Buffer.concat([directoryHead(images.length), ...groupEntries]);
  return Buffer.concat([marker, ...icons, resource(RT_GROUP_ICON, ICON_GROUP_ID, GROUP_FLAGS, group)]);
}

writeFileSync(icoPath, ico());
writeFileSync(resPath, res());
