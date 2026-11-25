import { Resvg } from "@resvg/resvg-js";
import { readFileSync, writeFileSync } from "node:fs";
const [src, out, png] = process.argv.slice(2);
const svg = readFileSync(src, "utf8");
const sizes = [16, 20, 24, 32, 40, 48, 64, 256];
const imgs = sizes.map(s => new Resvg(svg, { fitTo: { mode: "width", value: s } }).render().asPng());
writeFileSync(png, new Resvg(svg, { fitTo: { mode: "width", value: 256 } }).render().asPng());
const head = Buffer.alloc(6 + 16 * sizes.length);
head.writeUInt16LE(0, 0); head.writeUInt16LE(1, 2); head.writeUInt16LE(sizes.length, 4);
let off = head.length;
sizes.forEach((s, i) => {
  const e = 6 + 16 * i;
  head[e] = s === 256 ? 0 : s; head[e + 1] = s === 256 ? 0 : s;
  head.writeUInt16LE(1, e + 4); head.writeUInt16LE(32, e + 6);
  head.writeUInt32LE(imgs[i].length, e + 8); head.writeUInt32LE(off, e + 12);
  off += imgs[i].length;
});
writeFileSync(out, Buffer.concat([head, ...imgs]));
