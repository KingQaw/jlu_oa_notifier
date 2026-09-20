// 用 cheerio 复现 Rust 后端相同的选择器，验证解析逻辑。
import * as cheerio from "cheerio";
import { readFileSync } from "node:fs";

const listHtml = readFileSync(new URL("../../oa_list.html", import.meta.url), "utf8");
const detailHtml = readFileSync(new URL("../../oa_detail.html", import.meta.url), "utf8");

// ---- 列表 ----
const $ = cheerio.load(listHtml);
const items = [];
$("#itemContainer .li").each((_, li) => {
  const $li = $(li);
  const $title = $li.find("a.font14").first();
  if ($title.length === 0) return;
  const href = $title.attr("href") || "";
  const m = href.match(/[?&]id=(\d+)/);
  const id = m ? m[1] : null;
  if (!id) return;
  const rawTitle = $title.text().trim();
  const title = rawTitle.replace(/^\[置顶\]/, "").trim();
  const pinned = $title.find("font").toArray().some((f) => $(f).text().includes("置顶"));
  const org = $li.find("a.column").first().text().trim();
  const time = $li.find("span.time").first().text().trim();
  const isNew = $li.find("img[src*='new.gif']").length > 0;
  items.push({ id, title, org, time, pinned, isNew });
});

const totalM = listHtml.match(/共&nbsp;(\d+)&nbsp;条记录/);
const pagesM = listHtml.match(/\/(\d+)&nbsp;页/);
console.log("=== LIST ===");
console.log("total =", totalM?.[1], " totalPages =", pagesM?.[1], " items =", items.length);
for (const it of items.slice(0, 6)) {
  console.log(`- id=${it.id} pinned=${it.pinned} new=${it.isNew} | ${it.org} | ${it.time} | ${it.title}`);
}

// ---- 组织去重 ----
const orgs = [...new Set(items.map((i) => i.org))];
console.log("=== ORGS ===");
console.log(orgs.join("、"));

// ---- 详情 ----
const $d = cheerio.load(detailHtml);
const title = $d(".content_t").first().text().trim();
const org = $d(".content_time span").first().text().trim();
const timeRaw = $d(".content_time").first().text();
const time = timeRaw.replace(org, "").replace(/\u00a0/g, " ").trim();
const contentHtml = $d(".content_font").first().html() || "";
const atts = [];
$d(".news_aboutFile span[title]").each((_, s) => {
  atts.push({ filename: $(s).attr("id"), name: $(s).attr("title") });
});
console.log("=== DETAIL ===");
console.log("title =", title);
console.log("org =", org, " time =", time);
console.log("contentHtml len =", contentHtml.length);
console.log("contentHtml head =", JSON.stringify(contentHtml.slice(0, 80)));
console.log("contentHtml tail =", JSON.stringify(contentHtml.slice(-40)));
console.log("attachments =", atts.length);
for (const a of atts) console.log(`  - ${a.name} (${a.filename})`);
