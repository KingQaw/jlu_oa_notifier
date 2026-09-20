// 验证：按 app 的实际规则（基准 15px + 继承），各通知正文里有多少文本会被抬到可读下限。
import * as cheerio from "cheerio";
import { readFileSync, readdirSync } from "node:fs";

const MIN_PX = 15;
const BASE_PX = 15; // .detail-content 基准

function toPx(v) {
  const m = /^\s*([\d.]+)\s*(pt|px|em|rem)?\s*$/i.exec(v || "");
  if (!m) return null;
  const n = parseFloat(m[1]);
  const unit = (m[2] || "px").toLowerCase();
  if (unit === "pt") return (n * 96) / 72;
  if (unit === "em" || unit === "rem") return n * BASE_PX;
  return n;
}

const files = readdirSync(".").filter((f) => /^d_\d+\.html$/.test(f)).sort();
console.log(`发现 ${files.length} 条通知\n`);

for (const f of files) {
  const html = readFileSync(f, "utf8");
  const $ = cheerio.load(html);
  const root = $(".content_font").first();
  if (root.length === 0) continue;

  let total = 0;
  let clamped = 0;
  const sizes = new Set();

  const walk = (el, inherited) => {
    for (const node of el.children().toArray()) {
      if (node.type !== "tag") continue;
      const $n = $(node);
      const style = $n.attr("style") || "";
      const m = /font-size\s*:\s*([^;"']+)/i.exec(style);
      const own = m ? toPx(m[1]) : null;
      const size = own ?? inherited;
      if (node.tagName !== "sup" && node.tagName !== "sub") {
        total++;
        if (size < MIN_PX) clamped++;
        sizes.add(Math.round(size));
      }
      walk($n, size);
    }
  };
  walk(root, BASE_PX);

  const sorted = [...sizes].sort((a, b) => a - b);
  console.log(
    `${f}: 元素 ${String(total).padStart(4)} | 会抬升 ${String(clamped).padStart(4)}` +
      ` | 字号(px) ${sorted[0]}~${sorted[sorted.length - 1]} ${JSON.stringify(sorted)}`,
  );
}
