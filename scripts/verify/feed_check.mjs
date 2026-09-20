// 验证「今天/近三天/近一周」的抓取与过滤逻辑（与 src/main.ts 中的实现保持一致）。
// 运行：node scripts/verify/feed_check.mjs
import * as cheerio from "cheerio";

const UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
const CHANNEL = "179577";

const TIME_OPTIONS = [
  { value: "today", label: "今天", daysBack: 0 },
  { value: "3d", label: "近三天", daysBack: 2 },
  { value: "7d", label: "近一周", daysBack: 6 },
  { value: "all", label: "全部", daysBack: null },
];
const FEED_PAGE_CAP = { today: 5, "3d": 12, "7d": 25 };

// ---- 与前端一致的日期工具 ----
function shanghaiToday() {
  const s = new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date());
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
}
function cutoffFor(range) {
  const o = TIME_OPTIONS.find((x) => x.value === range);
  if (!o || o.daysBack === null) return null;
  const t = shanghaiToday();
  t.setDate(t.getDate() - o.daysBack);
  return t;
}
function parseNoticeDate(text, today) {
  const s = String(text).replace(/\u00a0/g, " ").trim();
  const shift = (n) => {
    const d = new Date(today);
    d.setDate(d.getDate() - n);
    return d;
  };
  if (s.startsWith("今天")) return shift(0);
  if (s.startsWith("昨天")) return shift(1);
  if (s.startsWith("前天")) return shift(2);
  let m = /^(\d{4})-(\d{1,2})-(\d{1,2})/.exec(s);
  if (m) return new Date(+m[1], +m[2] - 1, +m[3]);
  m = /^(\d{1,2})-(\d{1,2})$/.exec(s);
  if (m) return new Date(today.getFullYear(), +m[1] - 1, +m[2]);
  m = /^(\d{1,2}):(\d{2})/.exec(s);
  if (m) return shift(0);
  return null;
}
function inTimeRange(item, cutoff, today) {
  if (!cutoff || item.pinned) return true;
  const d = parseNoticeDate(item.time, today);
  return d === null ? true : d.getTime() >= cutoff.getTime();
}
function pageReachedCutoff(items, cutoff, today) {
  let oldest = null;
  for (const it of items) {
    if (it.pinned) continue;
    const d = parseNoticeDate(it.time, today);
    if (!d) continue;
    const t = d.getTime();
    if (oldest === null || t < oldest) oldest = t;
  }
  return oldest !== null && oldest < cutoff.getTime();
}

// ---- 抓取与解析 ----
async function fetchPage(page) {
  const url = `https://oa.jlu.edu.cn/defaultroot/PortalInformation!jldxList.action?channelId=${CHANNEL}&startPage=${page}`;
  const res = await fetch(url, { headers: { "User-Agent": UA } });
  const html = await res.text();
  const $ = cheerio.load(html);
  const items = [];
  $("#itemContainer .li").each((_, li) => {
    const $li = $(li);
    const $t = $li.find("a.font14").first();
    if ($t.length === 0) return;
    const m = ($t.attr("href") || "").match(/[?&]id=(\d+)/);
    if (!m) return;
    const raw = $t.text().trim();
    items.push({
      id: m[1],
      title: raw.replace(/^\[置顶\]/, "").trim(),
      org: $li.find("a.column").first().text().trim(),
      time: $li.find("span.time").first().text().replace(/\s+/g, " ").trim(),
      pinned: $t.find("font").toArray().some((f) => $(f).text().includes("置顶")),
    });
  });
  const totalPages = Number((html.match(/\/(\d+)&nbsp;页/) || [])[1] || 1);
  return { items, totalPages };
}

const fmt = (d) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
const today = shanghaiToday();
console.log("Asia/Shanghai 今天 =", fmt(today));
console.log();

for (const opt of TIME_OPTIONS) {
  if (opt.value === "all") continue;
  const cap = FEED_PAGE_CAP[opt.value];
  const cutoff = cutoffFor(opt.value);
  const collected = [];
  let fetched = 0;
  let reachedEnd = false;

  for (let page = 1; page <= cap; page++) {
    const { items, totalPages } = await fetchPage(page);
    fetched = page;
    collected.push(...items);
    if (page >= totalPages) {
      reachedEnd = true;
      break;
    }
    if (pageReachedCutoff(items, cutoff, today)) break;
  }

  const kept = collected.filter((i) => inTimeRange(i, cutoff, today));
  const pinnedKept = kept.filter((i) => i.pinned).length;
  const orgs = new Set(kept.map((i) => i.org));
  console.log(
    `${opt.label.padEnd(4)} 截止>=${fmt(cutoff)} | ` +
      `抓取 ${fetched} 页${reachedEnd ? "(到末尾)" : ""} | 命中 ${kept.length} 条 ` +
      `(其中置顶 ${pinnedKept}) | 涉及 ${orgs.size} 个组织`,
  );
  const sample = kept.filter((i) => !i.pinned).slice(0, 3);
  for (const s of sample) console.log(`      · ${s.time}  ${s.org}  ${s.title.slice(0, 24)}`);
}
