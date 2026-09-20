import "./style.css";
import { openUrl } from "@tauri-apps/plugin-opener";
import { onBackButtonPress } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import {
  fetchList,
  fetchDetail,
  searchOrgs,
  buildAttachmentUrl,
  fetchImage,
  startVpnLogin,
  closeVpnLogin,
} from "./api";
import { ORG_PRESETS } from "./orgs";
import {
  NEED_VPN_LOGIN,
  NETWORK_UNREACHABLE,
  VPN_LOGIN_EVENT,
  type Access,
  type ListOptions,
  type NoticeItem,
  type NoticeDetail,
  type VpnLoginResult,
} from "./types";

const BASE = "https://oa.jlu.edu.cn/defaultroot/";
const CHANNEL_ID = "179577";

/** 时间范围：今天 / 近三天 / 近一周 / 全部 */
type TimeRange = "today" | "3d" | "7d" | "all";

const TIME_OPTIONS: {
  value: TimeRange;
  label: string;
  /** 往前追溯的自然日天数（0=今天，2=今天+昨天+前天）；null 表示不限制 */
  daysBack: number | null;
}[] = [
  { value: "today", label: "今天", daysBack: 0 },
  { value: "3d", label: "近三天", daysBack: 2 },
  { value: "7d", label: "近一周", daysBack: 6 },
  { value: "all", label: "全部", daysBack: null },
];

/** 关注流模式（时间≠全部）下，每种范围最多自动抓取的页数，防止极端情况请求过多。 */
const FEED_PAGE_CAP: Record<Exclude<TimeRange, "all">, number> = {
  today: 5,
  "3d": 12,
  "7d": 25,
};

/** 「全部」模式下，若某页按关注组织过滤后为空，最多自动往后跳几页。 */
const MAX_SKIP_EMPTY_PAGES = 5;

const STORAGE_ORGS = "jlu-oa:followed-orgs";
const STORAGE_TIME = "jlu-oa:time-range";
const STORAGE_ACCESS_MODE = "jlu-oa:access-mode";
const STORAGE_VPN_URL = "jlu-oa:vpn-url";
const STORAGE_VPN_TICKET = "jlu-oa:vpn-cookies";

/** 访问模式：直连校内 OA，或经网页版 VPN 转发。 */
type AccessMode = "direct" | "vpn";

interface State {
  /** 关注的组织（白名单）；空数组表示不过滤 */
  followedOrgs: string[];
  timeRange: TimeRange;
  page: number;
  keyword: string;
  searchType: number;
  total: number;
  totalPages: number;
  items: NoticeItem[];
  currentId: string;
  loadingList: boolean;
  loadingDetail: boolean;
  /** 上一次成功渲染的「查询条件 + 页码」快照，用于判断是否需要把列表滚回顶部 */
  listRenderKey: string;
  /** 当前访问模式 */
  accessMode: AccessMode;
  /** 网页 VPN 站点地址（正常情况下由登录窗口自动获取） */
  vpnUrl: string;
  /** 网页 VPN 会话 Cookie 串（正常情况下由登录窗口自动抓取） */
  vpnTicket: string;
}

const state: State = {
  followedOrgs: [],
  timeRange: "3d",
  page: 1,
  keyword: "",
  searchType: 0,
  total: 0,
  totalPages: 1,
  items: [],
  currentId: "",
  loadingList: false,
  loadingDetail: false,
  listRenderKey: "",
  accessMode: "direct",
  vpnUrl: "",
  vpnTicket: "",
};

// ---------- DOM 骨架 ----------
const app = document.getElementById("app")!;
app.innerHTML = `
  <header class="topbar">
    <div class="brand">
      <span class="brand-title">吉林大学校内通知</span>
      <span class="brand-sub" id="totalInfo"></span>
    </div>
    <div class="topbar-actions">
      <button id="vpnBtn" class="btn vpn-btn" title="配置网页版 VPN 访问">直连</button>
      <button id="refreshBtn" class="btn" title="刷新">刷新</button>
    </div>
  </header>

  <div id="vpnPanel" class="vpn-panel hidden">
    <div class="vpn-row vpn-lead">
      <button id="vpnLoginBtn" class="btn btn-primary" type="button">
        一键登录 VPN
      </button>
      <span class="vpn-status" id="vpnAutoHint">
        点击后在应用内登录（支持扫码 / 账号密码），登录成功会自动配置并启用，无需手动复制任何内容。
      </span>
    </div>
    <div id="vpnProgress" class="vpn-progress hidden">
      <span class="spinner spinner-sm"></span>
      <span id="vpnProgressText">等待登录…</span>
    </div>

    <details id="vpnManual">
      <summary>手动配置（自动登录不可用时使用）</summary>
      <div class="vpn-row">
        <label class="vpn-label" for="vpnUrlInput">VPN 地址</label>
        <input
          id="vpnUrlInput"
          type="text"
          class="vpn-input"
          placeholder="登录 VPN 并进入校内 OA 后，复制地址栏的完整网址"
          autocomplete="off"
          spellcheck="false"
        />
      </div>
      <div class="vpn-row">
        <label class="vpn-label" for="vpnTicketInput">会话 Cookie</label>
        <input
          id="vpnTicketInput"
          type="text"
          class="vpn-input"
          placeholder="形如 name=value; name2=value2（F12 → Application → Cookies）"
          autocomplete="off"
          spellcheck="false"
        />
      </div>
      <div class="vpn-row">
        <button id="vpnApplyBtn" class="btn" type="button">应用并启用</button>
      </div>
    </details>

    <div class="vpn-row vpn-actions">
      <button id="vpnDisableBtn" class="btn" type="button">切回直连</button>
      <span id="vpnStatus" class="vpn-status"></span>
    </div>
  </div>

  <div class="toolbar">
    <div class="field">
      <div class="org-anchor">
        <button id="orgBtn" type="button" class="btn org-btn">组织：全部 ▾</button>
        <div id="orgPanel" class="org-panel hidden">
          <div class="org-panel-head">
            <input id="orgSearch" type="text" placeholder="搜索组织…" autocomplete="off" />
            <button id="orgClearBtn" type="button" class="btn">清空</button>
          </div>
          <div id="orgList" class="org-list"></div>
        </div>
      </div>
    </div>
    <div class="field">
      <div class="seg" id="timeSeg">
        <button type="button" class="seg-btn" data-range="today">今天</button>
        <button type="button" class="seg-btn" data-range="3d">近三天</button>
        <button type="button" class="seg-btn" data-range="7d">近一周</button>
        <button type="button" class="seg-btn" data-range="all">全部</button>
      </div>
    </div>
    <div class="field grow">
      <input id="keywordInput" type="text" placeholder="请输入关键字（可回车）" />
    </div>
    <div class="field">
      <select id="searchType">
        <option value="0">标题</option>
        <option value="1">组织</option>
        <option value="2">内容</option>
      </select>
    </div>
    <button id="searchBtn" class="btn btn-primary">查询</button>
  </div>

  <main class="content">
    <section class="list-pane" id="listPane">
      <div id="listStatus" class="list-status"></div>
      <ul id="list" class="list"></ul>
      <div id="listLoading" class="pane-overlay hidden" role="status" aria-live="polite">
        <div class="loading-card">
          <div class="spinner"></div>
          <div id="listLoadingText" class="loading-text">加载中…</div>
          <div id="listLoadingSub" class="loading-sub"></div>
        </div>
      </div>
      <div class="pager">
        <div id="pagerBrowse" class="pager-group">
          <button id="firstBtn" class="btn">首页</button>
          <button id="prevBtn" class="btn">上一页</button>
          <span id="pageInfo" class="page-info">第 1 / 1 页</span>
          <button id="nextBtn" class="btn">下一页</button>
          <span class="jump">
            跳至 <input id="jumpInput" type="number" min="1" class="jump-input" /> 页
            <button id="jumpBtn" class="btn">Go</button>
          </span>
        </div>
        <div id="pagerFeed" class="pager-group hidden">
          <button id="feedRefreshBtn" class="btn">刷新</button>
          <span id="feedInfo" class="page-info"></span>
        </div>
      </div>
    </section>

    <section class="detail-pane" id="detailPane">
      <div id="detail" class="detail">
        <div class="placeholder">← 选择左侧通知查看详情</div>
      </div>
      <div id="detailLoading" class="pane-overlay hidden" role="status" aria-live="polite">
        <div class="loading-card">
          <div class="spinner"></div>
          <div class="loading-text">加载详情中…</div>
        </div>
      </div>
    </section>
  </main>

  <div id="toast" class="toast hidden"></div>
`;

const $ = <T extends HTMLElement>(sel: string) =>
  document.querySelector(sel) as T;

const el = {
  totalInfo: $<HTMLElement>("#totalInfo"),
  refreshBtn: $<HTMLButtonElement>("#refreshBtn"),
  orgBtn: $<HTMLButtonElement>("#orgBtn"),
  orgPanel: $<HTMLElement>("#orgPanel"),
  orgSearch: $<HTMLInputElement>("#orgSearch"),
  orgList: $<HTMLElement>("#orgList"),
  orgClearBtn: $<HTMLButtonElement>("#orgClearBtn"),
  timeSeg: $<HTMLElement>("#timeSeg"),
  keywordInput: $<HTMLInputElement>("#keywordInput"),
  searchType: $<HTMLSelectElement>("#searchType"),
  searchBtn: $<HTMLButtonElement>("#searchBtn"),
  listStatus: $<HTMLElement>("#listStatus"),
  listLoading: $<HTMLElement>("#listLoading"),
  listLoadingText: $<HTMLElement>("#listLoadingText"),
  listLoadingSub: $<HTMLElement>("#listLoadingSub"),
  detailLoading: $<HTMLElement>("#detailLoading"),
  list: $<HTMLUListElement>("#list"),
  pagerBrowse: $<HTMLElement>("#pagerBrowse"),
  pagerFeed: $<HTMLElement>("#pagerFeed"),
  firstBtn: $<HTMLButtonElement>("#firstBtn"),
  prevBtn: $<HTMLButtonElement>("#prevBtn"),
  nextBtn: $<HTMLButtonElement>("#nextBtn"),
  pageInfo: $<HTMLElement>("#pageInfo"),
  jumpInput: $<HTMLInputElement>("#jumpInput"),
  jumpBtn: $<HTMLButtonElement>("#jumpBtn"),
  feedRefreshBtn: $<HTMLButtonElement>("#feedRefreshBtn"),
  feedInfo: $<HTMLElement>("#feedInfo"),
  detail: $<HTMLElement>("#detail"),
  detailPane: $<HTMLElement>("#detailPane"),
  listPane: $<HTMLElement>("#listPane"),
  toast: $<HTMLElement>("#toast"),
  vpnBtn: $<HTMLButtonElement>("#vpnBtn"),
  vpnPanel: $<HTMLElement>("#vpnPanel"),
  vpnLoginBtn: $<HTMLButtonElement>("#vpnLoginBtn"),
  vpnAutoHint: $<HTMLElement>("#vpnAutoHint"),
  vpnProgress: $<HTMLElement>("#vpnProgress"),
  vpnProgressText: $<HTMLElement>("#vpnProgressText"),
  vpnManual: $<HTMLDetailsElement>("#vpnManual"),
  vpnUrlInput: $<HTMLInputElement>("#vpnUrlInput"),
  vpnTicketInput: $<HTMLInputElement>("#vpnTicketInput"),
  vpnApplyBtn: $<HTMLButtonElement>("#vpnApplyBtn"),
  vpnDisableBtn: $<HTMLButtonElement>("#vpnDisableBtn"),
  vpnStatus: $<HTMLElement>("#vpnStatus"),
};

// ---------- 工具 ----------
let toastTimer: number | undefined;
function toast(msg: string) {
  el.toast.textContent = msg;
  el.toast.classList.remove("hidden");
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => el.toast.classList.add("hidden"), 2600);
}

function isMobile() {
  return window.matchMedia("(max-width: 760px)").matches;
}

/**
 * 处理 Android 状态栏遮挡：给顶栏补上状态栏高度的内边距。
 *
 * 背景：Android 15+ 强制 edge-to-edge，网页会绘制到状态栏之下（实测顶栏
 * 从屏幕 y=0 就开始，被状态栏的时间/信号图标压住）。
 *
 * 取值优先级：
 *  1. `env(safe-area-inset-top)`——Chromium 支持，正常应能拿到真实值；
 *  2. 实测「探测元素撑满视口后的高度 − window.innerHeight」；
 *  3. 按设备像素比换算：多数 Android 状态栏为 24dp，即 24 × dpr 物理像素。
 *     设备像素比接近整数时，CSS 像素与物理像素 1:1，可直接用 dpr × 24。
 *
 * 桌面端三者都会得到 0，不产生任何影响。
 * 测量结果写入 `document.title` 前缀，便于在真机上一眼确认取值来源。
 */
function applyStatusBarInset() {
  // Android 15+ 强制 edge-to-edge，顶栏会被状态栏压住，需要补上状态栏高度。
  //
  // 实测数据（vivo / Android 16 / dpr=3.5）：
  //   env(safe-area-inset-top) = 40 CSS px  ← 可用，但它等于「顶栏基础内边距
  //                                            + 边框 + 状态栏」，不是纯状态栏高度
  //   视口测量(100vh − innerHeight) = 0      ← 完全失效，不可用
  // 因此以 safe-area 为准，减去顶栏自身的基础内边距(10px)与边框(1px)。
  const BASE_PAD = 10;
  const BASE_BORDER = 1;

  let safeArea = 0;
  try {
    const probe = document.createElement("div");
    probe.style.cssText =
      "position:fixed;top:0;left:0;width:0;height:env(safe-area-inset-top,0px);visibility:hidden;";
    document.body.appendChild(probe);
    safeArea = probe.getBoundingClientRect().height;
    probe.remove();
  } catch {
    /* 忽略 */
  }

  let inset = 0;
  if (safeArea > 0) {
    inset = safeArea - BASE_PAD - BASE_BORDER;
  }
  // 兜底：Android 状态栏通常为 24dp
  if (inset <= 0 && /Android/i.test(navigator.userAgent)) {
    inset = 24;
  }
  const safe = Math.max(0, Math.min(Math.round(inset), 40));
  document.documentElement.style.setProperty("--status-bar-inset", `${safe}px`);
}

/**
 * 显示 / 隐藏明显的加载提示遮罩。
 * 遮罩覆盖所在面板、挡住误点击，因此必须在 finally 里关闭，避免请求异常时卡死界面。
 *
 * @param which  "detail" 时用详情面板的遮罩，否则用列表面板的遮罩
 * @param text    主文案，如「加载中…」
 * @param sub     副文案（如抓取页进度），传空字符串表示不显示
 */
function setLoading(
  on: boolean,
  which: "list" | "detail" = "list",
  text = "加载中…",
  sub = "",
) {
  if (which === "detail") {
    el.detailLoading.classList.toggle("hidden", !on);
    return;
  }
  if (on) {
    el.listLoadingText.textContent = text;
    el.listLoadingSub.textContent = sub;
    el.listLoadingSub.classList.toggle("hidden", sub === "");
  }
  el.listLoading.classList.toggle("hidden", !on);
}

// ---------- 时间范围 ----------

/** 按 Asia/Shanghai 取「今天」0 点，避免依赖本机时区设置。 */
function shanghaiToday(): Date {
  const s = new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date());
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
}

/** 当前时间范围对应的截止日（含当天）；「全部」返回 null。 */
function cutoffFor(range: TimeRange): Date | null {
  const opt = TIME_OPTIONS.find((o) => o.value === range);
  if (!opt || opt.daysBack === null) return null;
  const t = shanghaiToday();
  t.setDate(t.getDate() - opt.daysBack);
  return t;
}

/**
 * 解析列表里的时间文本。站方只给「今天 HH:MM」「昨天 HH:MM」「YYYY-MM-DD」这类
 * 相对/绝对形式；识别不了返回 null，调用方按「不过滤」处理（宁可多显示，不漏掉）。
 */
function parseNoticeDate(text: string, today: Date): Date | null {
  const s = text.replace(/\u00a0/g, " ").trim();
  const shift = (n: number) => {
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
  m = /^(\d{1,2}):(\d{2})/.exec(s); // 只给时间 → 今天
  if (m) return shift(0);
  return null;
}

function inFollowedOrgs(item: NoticeItem, followed: Set<string>): boolean {
  return followed.size === 0 || followed.has(item.org);
}

/** 置顶通知豁免时间过滤（按需求：保留且始终排在最前面）。 */
function inTimeRange(
  item: NoticeItem,
  cutoff: Date | null,
  today: Date,
): boolean {
  if (!cutoff || item.pinned) return true;
  const d = parseNoticeDate(item.time, today);
  return d === null ? true : d.getTime() >= cutoff.getTime();
}

/** 置顶优先，其余按发布时间倒序；同一时间保持原顺序（sort 稳定）。 */
function compareItems(today: Date) {
  return (a: NoticeItem, b: NoticeItem): number => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    const da = parseNoticeDate(a.time, today);
    const db = parseNoticeDate(b.time, today);
    return (db ? db.getTime() : 0) - (da ? da.getTime() : 0);
  };
}

function dedupe(items: NoticeItem[]): NoticeItem[] {
  const seen = new Set<string>();
  const out: NoticeItem[] = [];
  for (const it of items) {
    if (seen.has(it.id)) continue;
    seen.add(it.id);
    out.push(it);
  }
  return out;
}

// ---------- 偏好持久化 ----------

function loadPrefs() {
  try {
    const rawOrgs = localStorage.getItem(STORAGE_ORGS);
    if (rawOrgs) {
      const arr: unknown = JSON.parse(rawOrgs);
      if (Array.isArray(arr)) {
        state.followedOrgs = arr.filter(
          (x): x is string => typeof x === "string",
        );
      }
    }
    const t = localStorage.getItem(STORAGE_TIME);
    if (t && TIME_OPTIONS.some((o) => o.value === t)) {
      state.timeRange = t as TimeRange;
    }
    const mode = localStorage.getItem(STORAGE_ACCESS_MODE);
    if (mode === "vpn" || mode === "direct") state.accessMode = mode;
    state.vpnUrl = localStorage.getItem(STORAGE_VPN_URL) ?? "";
    state.vpnTicket = localStorage.getItem(STORAGE_VPN_TICKET) ?? "";
    // 旧版本可能存成短地址（base URL），这里补上 defaultroot，避免拼接后 404
    if (state.vpnUrl && !state.vpnUrl.includes("defaultroot")) {
      state.vpnUrl = `${state.vpnUrl.replace(/\/+$/, "")}/defaultroot/`;
    }
  } catch {
    /* 本地配置损坏时忽略，走默认值 */
  }
}

function savePrefs() {
  try {
    localStorage.setItem(STORAGE_ORGS, JSON.stringify(state.followedOrgs));
    localStorage.setItem(STORAGE_TIME, state.timeRange);
    localStorage.setItem(STORAGE_ACCESS_MODE, state.accessMode);
    localStorage.setItem(STORAGE_VPN_URL, state.vpnUrl);
    localStorage.setItem(STORAGE_VPN_TICKET, state.vpnTicket);
  } catch {
    /* 忽略写入失败，不影响使用 */
  }
}

// ---------- 访问模式（直连 / 网页版 VPN） ----------

/**
 * 当前访问模式对应的参数，交给 Rust 侧构造请求地址。
 *
 * VPN 模式下若还没填地址，返回 null —— 调用方需要先提示用户去配置，
 * 否则后端会因地址为空直接报错。
 */
function currentAccess(): Access | null {
  if (state.accessMode !== "vpn") return { mode: "direct" };
  const prefix = state.vpnUrl.trim();
  if (!prefix) return null;
  return {
    mode: "vpn",
    prefix,
    cookies: state.vpnTicket.trim() || undefined,
  };
}

/** 是否为 VPN 模式且配置完备（地址与 Cookie 都就绪）。 */
function vpnReady(): boolean {
  return state.accessMode === "vpn" && !!state.vpnUrl.trim();
}

/** 更新顶栏按钮文字与状态提示。 */
function updateVpnUi() {
  const on = state.accessMode === "vpn";
  el.vpnBtn.textContent = on ? "VPN" : "直连";
  el.vpnBtn.classList.toggle("active", on);

  if (!on) {
    el.vpnStatus.textContent = "当前：直连校内 OA（若在校外请改用 VPN）";
    el.vpnStatus.classList.remove("warn");
    return;
  }
  if (!vpnReady()) {
    el.vpnStatus.textContent = "已选 VPN，但尚未完成登录配置";
    el.vpnStatus.classList.add("warn");
    return;
  }
  el.vpnStatus.textContent = state.vpnTicket.trim()
    ? "当前：经 VPN 访问（会话由登录窗口自动获取）"
    : "当前：经 VPN 访问（无会话，可能提示需要登录）";
  el.vpnStatus.classList.remove("warn");
}

/** 切换面板显隐，并在打开时把已保存的值回填到手动配置输入框。 */
function toggleVpnPanel(show?: boolean) {
  const willShow = show ?? el.vpnPanel.classList.contains("hidden");
  el.vpnPanel.classList.toggle("hidden", !willShow);
  if (willShow) {
    el.vpnUrlInput.value = state.vpnUrl;
    el.vpnTicketInput.value = state.vpnTicket;
  }
}

// ---------- 一键登录 VPN ----------

/** 登录期进度计时器，仅用于长时间无结果时自动复位 UI。 */
let vpnProgressTimer: number | undefined;

/** 是否正在等待内置登录窗口返回结果。 */
let vpnLoginPending = false;
let vpnResultUnlisten: (() => void) | undefined;

function setVpnProgress(on: boolean, text = "等待登录…") {
  el.vpnProgress.classList.toggle("hidden", !on);
  el.vpnLoginBtn.disabled = on;
  if (on) {
    el.vpnProgressText.textContent = text;
    el.vpnAutoHint.textContent =
      "请在窗口里完成登录，之后会自动进入 OA 并关闭窗口，无需其他操作。";
  } else {
    el.vpnAutoHint.textContent =
      "点击后在应用内登录，登录成功即自动进入 OA 并启用，无需手动复制或点击任何内容。";
  }
}

/** 处理内置登录窗口的回传结果。 */
function onVpnLoginResult(res: VpnLoginResult) {
  vpnLoginPending = false;
  setVpnProgress(false);
  if (vpnProgressTimer) {
    clearTimeout(vpnProgressTimer);
    vpnProgressTimer = undefined;
  }

  if (!res.ok) {
    const msg = res.message ?? "登录未完成";
    el.vpnStatus.textContent = msg;
    el.vpnStatus.classList.add("warn");
    toast(`VPN 登录未完成：${msg}`);
    return;
  }

  state.vpnUrl = res.prefix ?? "";
  state.vpnTicket = res.cookies ?? "";
  state.accessMode = "vpn";
  savePrefs();
  updateVpnUi();
  el.vpnUrlInput.value = state.vpnUrl;
  el.vpnTicketInput.value = state.vpnTicket;
  toggleVpnPanel(false);
  toast("VPN 登录成功，已自动启用");
  // 由主窗口主动关闭登录窗口。
  //
  // 为什么不在 Rust 侧关：实测 Android 上后台线程里的 destroy()/close() 都不生效
  // （诊断条显示流程已走到"正在关闭窗口"，窗口却一直存在）。主窗口运行在正常的
  // Tauri 上下文里，由它调用 close_vpn_login 更可靠。
  void closeVpnLogin()
    .then(() => console.log("[vpn] 登录窗口已关闭"))
    .catch((e) => console.warn("[vpn] 关闭登录窗口失败：", e));
  void reload();
}

/** 发起一键登录。 */
async function startVpnLoginFlow() {
  if (vpnLoginPending) {
    toast("登录窗口已打开，请在其中完成登录");
    return;
  }

  // 事件监听只需注册一次
  if (!vpnResultUnlisten) {
    try {
      vpnResultUnlisten = await listen<VpnLoginResult>(VPN_LOGIN_EVENT, (e) =>
        onVpnLoginResult(e.payload),
      );
    } catch (e) {
      toast(`无法监听登录结果：${String(e)}`);
      return;
    }
  }

  try {
    await startVpnLogin();
  } catch (e) {
    toast(`无法打开登录窗口：${String(e)}`);
    return;
  }

  vpnLoginPending = true;
  setVpnProgress(true);
  // 后端最长等待 5 分钟；这里多给 1 分钟后自动复位，避免 UI 卡在"等待登录"
  vpnProgressTimer = window.setTimeout(() => {
    vpnLoginPending = false;
    setVpnProgress(false);
  }, 360_000);
}

/** 用户取消登录。 */
async function cancelVpnLogin() {
  if (!vpnLoginPending) {
    toggleVpnPanel(false);
    return;
  }
  try {
    await closeVpnLogin();
  } catch {
    /* 窗口可能已关闭，忽略 */
  }
  vpnLoginPending = false;
  setVpnProgress(false);
  if (vpnProgressTimer) {
    clearTimeout(vpnProgressTimer);
    vpnProgressTimer = undefined;
  }
  toast("已取消登录");
}

/**
 * 把后端返回的错误转成用户能看懂的话，并给出下一步动作。
 * - NEED_VPN_LOGIN：网页 VPN 未登录 / 会话过期
 * - NETWORK_UNREACHABLE：直连连不上，多半是没连校园网
 */
function describeError(e: unknown): string {
  const msg = String(e);
  if (msg.includes(NEED_VPN_LOGIN)) {
    return "VPN 会话已失效，请点右上角「直连/VPN」重新一键登录";
  }
  if (msg.includes(NETWORK_UNREACHABLE)) {
    return "连不上校内网。若当前不在校园网，请点右上角「直连/VPN」改用 VPN 访问";
  }
  return msg;
}

// ---------- 列表加载 ----------

function currentQueryKey(): string {
  return [
    state.followedOrgs.join(","),
    state.timeRange,
    state.keyword,
    state.searchType,
    // 切换访问模式后列表内容会变，必须进 key，否则不会回到顶部也不触发重载判断
    state.accessMode,
    state.vpnUrl,
  ].join("\u0001");
}

function buildOpts(page: number): ListOptions {
  return {
    // 恰好关注 1 个组织时交给服务端精确筛选（分页更准、请求更少）；
    // ≥2 个组织服务端不支持（orgname 单值），改为本地过滤。
    org: state.followedOrgs.length === 1 ? state.followedOrgs[0] : undefined,
    page,
    keyword: state.keyword || undefined,
    searchType: state.searchType,
    // 站方 searchDate 必须配合关键词才生效，这里统一由客户端按今天/近三天/近一周过滤
    dateRange: undefined,
    access: currentAccess() ?? undefined,
  };
}

/**
 * 启动加载前的访问模式校验。
 * VPN 模式下必须先填地址，否则宁可拦住也不静默退回直连——那会请求到校内 OA，
 * 在校外表现为连不上，用户很难判断原因。
 */
function ensureAccessReady(): boolean {
  if (state.accessMode !== "vpn") return true;
  if (state.vpnUrl.trim()) return true;
  toast("已切到 VPN 访问，但还没有填写 VPN 地址");
  toggleVpnPanel(true);
  return false;
}

/** 判断一页是否已经翻过时间线（只看非置顶项——置顶往往是几天前的旧通知）。 */
function pageReachedCutoff(
  items: NoticeItem[],
  cutoff: Date,
  today: Date,
): boolean {
  let oldest: number | null = null;
  for (const it of items) {
    if (it.pinned) continue;
    const d = parseNoticeDate(it.time, today);
    if (!d) continue;
    const t = d.getTime();
    if (oldest === null || t < oldest) oldest = t;
  }
  return oldest !== null && oldest < cutoff.getTime();
}

function setPagerMode() {
  const feed = state.timeRange !== "all";
  el.pagerBrowse.classList.toggle("hidden", feed);
  el.pagerFeed.classList.toggle("hidden", !feed);
}

/** 关注流模式（今天 / 近三天 / 近一周）：自动往后抓取，直到越过时间线。 */
async function loadFeed() {
  if (state.loadingList) return;
  if (!ensureAccessReady()) return;
  state.loadingList = true;
  setLoading(true, "list", "加载中…");

  const range = state.timeRange as Exclude<TimeRange, "all">;
  const cap = FEED_PAGE_CAP[range];
  const today = shanghaiToday();
  const cutoff = cutoffFor(range);
  const followed = new Set(state.followedOrgs);
  const label = TIME_OPTIONS.find((o) => o.value === range)?.label ?? "";

  const collected: NoticeItem[] = [];
  let fetched = 0;
  let reachedEnd = false;
  try {
    for (let page = 1; page <= cap; page++) {
      el.listStatus.textContent = `正在抓取第 ${page} 页…`;
      setLoading(true, "list", "加载中…", `正在抓取第 ${page} 页…`);
      const res = await fetchList(buildOpts(page));
      fetched = page;
      collected.push(...res.items);
      if (page >= res.totalPages) {
        reachedEnd = true;
        break;
      }
      if (cutoff && pageReachedCutoff(res.items, cutoff, today)) break;
    }

    const items = dedupe(collected)
      .filter(
        (i) => inFollowedOrgs(i, followed) && inTimeRange(i, cutoff, today),
      )
      .sort(compareItems(today));

    state.items = items;
    state.page = 1;
    state.total = items.length;
    state.totalPages = 1;
    renderList();

    el.listStatus.textContent =
      items.length === 0
        ? `关注范围内暂无通知（${label}）`
        : `关注范围 · ${label} · 共 ${items.length} 条`;
    el.feedInfo.textContent = reachedEnd
      ? "已到列表末尾"
      : `已抓取 ${fetched} 页，已覆盖所选时间范围`;
    el.totalInfo.textContent = items.length ? `共 ${items.length} 条` : "";
    setPagerMode();
    // 关注流每次都是「重新取一份最新视图」，固定回到顶部
    state.listRenderKey = `${currentQueryKey()}\u0001feed`;
    el.list.scrollTop = 0;
  } catch (e) {
    el.listStatus.textContent = "加载失败，请检查网络后重试";
    toast(`加载失败：${describeError(e)}`);
  } finally {
    state.loadingList = false;
    setLoading(false, "list");
  }
}

/** 浏览模式（全部时间）：沿用服务端分页，本地按关注组织过滤。 */
async function loadBrowse(page: number) {
  if (state.loadingList) return;
  if (!ensureAccessReady()) return;
  state.loadingList = true;
  setLoading(true, "list", "加载中…");

  const followed = new Set(state.followedOrgs);
  try {
    let target = page;
    let skipped = 0;
    for (;;) {
      el.listStatus.textContent = "加载中…";
      // 关注组织过滤后为空时会连跳若干页，这里把页码透出来，避免看起来像卡住
      setLoading(true, "list", "加载中…", `正在读取第 ${target} 页…`);
      const res = await fetchList(buildOpts(target));
      const items = res.items.filter((i) => inFollowedOrgs(i, followed));

      state.page = res.page;
      state.totalPages = res.totalPages;
      state.total = res.total;

      const lastPage = res.page >= res.totalPages;
      if (items.length > 0 || lastPage || skipped >= MAX_SKIP_EMPTY_PAGES) {
        state.items = items;
        renderList();
        el.listStatus.textContent =
          items.length === 0
            ? `本页没有关注组织的通知（第 ${res.page} / ${res.totalPages} 页）`
            : followed.size > 0
              ? `共 ${res.total} 条，第 ${res.page} / ${res.totalPages} 页（本页命中 ${items.length} 条）`
              : `共 ${res.total} 条，第 ${res.page} / ${res.totalPages} 页`;
        el.pageInfo.textContent = `第 ${res.page} / ${res.totalPages} 页`;
        el.jumpInput.max = String(res.totalPages || 1);
        el.firstBtn.disabled = res.page <= 1;
        el.prevBtn.disabled = res.page <= 1;
        el.nextBtn.disabled = lastPage;
        el.totalInfo.textContent = res.total ? `共 ${res.total} 条` : "";
        setPagerMode();

        const key = `${currentQueryKey()}\u0001${res.page}`;
        if (key !== state.listRenderKey) {
          state.listRenderKey = key;
          el.list.scrollTop = 0;
        }
        return;
      }

      // 本页按关注组织过滤后为空 → 自动往后翻，避免用户连点“下一页”却什么都看不到
      target = res.page + 1;
      skipped++;
    }
  } catch (e) {
    el.listStatus.textContent = "加载失败，请检查网络后重试";
    toast(`加载失败：${describeError(e)}`);
  } finally {
    state.loadingList = false;
    setLoading(false, "list");
  }
}

/** 按当前模式重新加载。 */
async function reload() {
  if (state.timeRange === "all") await loadBrowse(1);
  else await loadFeed();
}

function renderList() {
  el.list.innerHTML = "";
  for (const it of state.items) {
    const li = document.createElement("li");
    li.className = "item" + (it.id === state.currentId ? " active" : "");
    const title = document.createElement("div");
    title.className = "item-title";
    const titleText = document.createElement("span");
    titleText.className = "title-text";
    titleText.textContent = it.title;
    title.appendChild(titleText);
    if (it.pinned) {
      const pin = document.createElement("span");
      pin.className = "badge pin";
      pin.textContent = "置顶";
      title.prepend(pin);
    }
    if (it.isNew) {
      const nw = document.createElement("span");
      nw.className = "badge new";
      nw.textContent = "新";
      title.appendChild(nw);
    }
    const meta = document.createElement("div");
    meta.className = "item-meta";
    const org = document.createElement("span");
    org.className = "org";
    org.textContent = it.org;
    const time = document.createElement("span");
    time.className = "time";
    time.textContent = it.time;
    meta.append(org, time);
    li.append(title, meta);
    li.addEventListener("click", () => openDetail(it.id));
    el.list.appendChild(li);
  }
}

// ---------- 详情 ----------
async function openDetail(id: string) {
  state.currentId = id;
  renderList();
  if (isMobile()) {
    el.listPane.classList.add("hidden-mobile");
    el.detailPane.classList.add("show-mobile");
    // 推入一条历史记录，让 Android 返回键/返回手势能回到列表而不是直接退出应用
    if (!history.state?.jluOaDetail) {
      history.pushState({ jluOaDetail: true }, "");
    }
  }
  // 打开新的通知时，详情面板回到顶部（否则会沿用上一条的滚动位置）
  el.detailPane.scrollTop = 0;
  state.loadingDetail = true;
  setLoading(true, "detail");
  el.detail.innerHTML = '<div class="placeholder">加载详情中…</div>';
  try {
    const d = await fetchDetail(id, currentAccess() ?? undefined);
    renderDetail(d);
  } catch (e) {
    el.detail.innerHTML = '<div class="placeholder">详情加载失败</div>';
    toast(`详情加载失败：${describeError(e)}`);
  } finally {
    state.loadingDetail = false;
    setLoading(false, "detail");
  }
}

function renderDetail(d: NoticeDetail) {
  const header = document.createElement("div");
  header.className = "detail-head";
  const back = document.createElement("button");
  back.className = "btn back-btn";
  back.textContent = "← 返回";
  back.addEventListener("click", () => {
    // 有我们推入的历史记录时走 history.back()，让 popstate 统一收尾
    if (history.state?.jluOaDetail) history.back();
    else backToList();
  });
  const hTitle = document.createElement("h2");
  hTitle.className = "detail-title";
  hTitle.textContent = d.title;
  const meta = document.createElement("div");
  meta.className = "detail-meta";
  const orgSpan = document.createElement("span");
  orgSpan.className = "detail-org";
  orgSpan.textContent = d.org;
  const timeSpan = document.createElement("span");
  timeSpan.className = "detail-time";
  timeSpan.textContent = d.time;
  meta.append(orgSpan, timeSpan);
  const srcBtn = document.createElement("button");
  srcBtn.className = "btn btn-link";
  srcBtn.textContent = "在浏览器中打开原文";
  srcBtn.addEventListener("click", () =>
    openUrl(
      `${BASE}PortalInformation!getInformation.action?id=${d.id}&channelId=${CHANNEL_ID}`,
    ),
  );
  meta.append(srcBtn);
  header.append(back, hTitle, meta);

  const body = document.createElement("div");
  body.className = "detail-body";
  const content = document.createElement("div");
  content.className = "detail-content";
  content.innerHTML = d.contentHtml;
  rewriteUrls(content);
  normalizeFontSizes(content);
  void inlineImages(content);
  body.appendChild(content);

  if (d.attachments.length > 0) {
    const attWrap = document.createElement("div");
    attWrap.className = "attachments";
    const attTitle = document.createElement("div");
    attTitle.className = "attachments-title";
    attTitle.textContent = `附件（${d.attachments.length}）`;
    attWrap.appendChild(attTitle);
    for (const a of d.attachments) {
      const row = document.createElement("button");
      row.className = "attachment-item";
      row.textContent = `📎 ${a.name}`;
      row.addEventListener("click", () => downloadAttachment(d.id, a));
      attWrap.appendChild(row);
    }
    body.appendChild(attWrap);
  }

  el.detail.innerHTML = "";
  el.detail.append(header, body);
}

function rewriteUrls(root: HTMLElement) {
  root.querySelectorAll("img[src]").forEach((img) => {
    const src = img.getAttribute("src")!;
    if (src && !/^data:/i.test(src)) {
      img.setAttribute("src", new URL(src, BASE).href);
    }
  });
  root.querySelectorAll("a[href]").forEach((a) => {
    const href = a.getAttribute("href")!;
    if (href && !/^(https?:|mailto:|javascript:|#)/i.test(href)) {
      a.setAttribute("href", new URL(href, BASE).href);
    }
  });
}

/** 正文可读字号下限（px）。 */
const MIN_CONTENT_FONT_PX = 15;

/**
 * 通知正文来自 Word 导出的 HTML，字号是绝对值且各通知不一致
 * （常见 9pt≈12px、14pt≈18.7px、15pt、16pt…）。这里把小于下限的字号抬到下限，
 * 避免部分通知「字号很小」；上标/下标等本来就更小的元素保持原样。
 * 按文档顺序遍历，父元素抬高后子元素继承，不会重复处理。
 */
function normalizeFontSizes(root: HTMLElement) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const el = node as HTMLElement;
    const tag = el.tagName;
    if (tag === "SUP" || tag === "SUB" || tag === "SMALL") continue;
    const px = parseFloat(window.getComputedStyle(el).fontSize);
    if (Number.isFinite(px) && px > 0 && px < MIN_CONTENT_FONT_PX) {
      el.style.fontSize = `${MIN_CONTENT_FONT_PX}px`;
    }
  }
}

/**
 * 正文图片改由后端（Rust）抓取后内联为 data URL。
 *
 * 原因：app 前端运行在 `tauri://localhost`，与 `https://oa.jlu.edu.cn` 是跨源；
 * 图片若交给 WebView 直接加载，会受 WebView 的跨源策略 / 证书信任 / 代理环境
 * 差异影响而加载失败。后端抓取走的是与正文 HTML 完全相同、已验证可用的通道。
 * 抓取失败时保留原始地址，交给 WebView 兜底尝试。
 */
async function inlineImages(root: HTMLElement) {
  const images = Array.from(
    root.querySelectorAll<HTMLImageElement>("img[src]"),
  );
  await Promise.allSettled(
    images.map(async (img) => {
      const src = img.getAttribute("src");
      if (!src || src.startsWith("data:")) return;
      if (!/^https:\/\/oa\.jlu\.edu\.cn\//i.test(src)) return;
      try {
        img.src = await fetchImage(src, currentAccess() ?? undefined);
      } catch {
        /* 保留原始 URL，交给 WebView 兜底 */
      }
    }),
  );
}

async function downloadAttachment(
  informationId: string,
  a: { filename: string; name: string },
) {
  try {
    const attachmentUrl = await buildAttachmentUrl(
      informationId,
      a.filename,
      a.name,
      currentAccess() ?? undefined,
    );
    await openUrl(attachmentUrl);
  } catch (e) {
    toast(`附件下载失败：${String(e)}`);
  }
}

// ---------- 组织多选（关注白名单） ----------
let orgSearchTimer: number | undefined;
/** 面板当前候选组织（预置列表 + 在线模糊匹配结果）。 */
let orgCandidates: string[] = [...ORG_PRESETS];

function updateOrgBtn() {
  const n = state.followedOrgs.length;
  el.orgBtn.textContent =
    n === 0
      ? "组织：全部 ▾"
      : n === 1
        ? `组织：${state.followedOrgs[0]} ▾`
        : `组织：已关注 ${n} 个 ▾`;
  el.orgBtn.classList.toggle("active", n > 0);
}

function renderOrgList(filter = "") {
  const q = filter.trim();
  const selected = new Set(state.followedOrgs);

  let names = orgCandidates;
  if (q) names = orgCandidates.filter((n) => n.includes(q));
  // 已关注的组织始终可见，避免“选了却找不到”
  const missingFollowed = state.followedOrgs.filter(
    (s) => !names.includes(s) && (q === "" || s.includes(q)),
  );
  names = [...missingFollowed, ...names];

  el.orgList.innerHTML = "";
  if (names.length === 0) {
    const empty = document.createElement("div");
    empty.className = "org-empty";
    empty.textContent = "没有匹配的组织";
    el.orgList.appendChild(empty);
    return;
  }

  for (const name of names) {
    const row = document.createElement("label");
    row.className = "org-item";
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = selected.has(name);
    cb.addEventListener("change", () => toggleOrg(name, cb.checked));
    const span = document.createElement("span");
    span.textContent = name;
    row.append(cb, span);
    el.orgList.appendChild(row);
  }
}

/** 勾选会连续触发，这里合并成一次重载，避免每点一下都重新抓一遍。 */
let reloadTimer: number | undefined;
function scheduleReload() {
  if (reloadTimer) clearTimeout(reloadTimer);
  reloadTimer = window.setTimeout(() => void reload(), 400);
}

function toggleOrg(name: string, checked: boolean) {
  const set = new Set(state.followedOrgs);
  if (checked) set.add(name);
  else set.delete(name);
  state.followedOrgs = [...set];
  savePrefs();
  updateOrgBtn();
  scheduleReload();
}

function openOrgPanel() {
  el.orgPanel.classList.remove("hidden");
  el.orgSearch.value = "";
  orgCandidates = [...ORG_PRESETS];
  renderOrgList();
}

function closeOrgPanel() {
  el.orgPanel.classList.add("hidden");
}

el.orgBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  if (el.orgPanel.classList.contains("hidden")) openOrgPanel();
  else closeOrgPanel();
});
// 面板内部点击不应触发全局「点空白关闭」
el.orgPanel.addEventListener("click", (e) => e.stopPropagation());
document.addEventListener("click", () => closeOrgPanel());

el.orgSearch.addEventListener("input", () => {
  const q = el.orgSearch.value.trim();
  renderOrgList(q);
  if (q.length >= 2) {
    if (orgSearchTimer) clearTimeout(orgSearchTimer);
    orgSearchTimer = window.setTimeout(async () => {
      try {
        const remote = await searchOrgs(q, currentAccess() ?? undefined);
        const merged = [...new Set([...ORG_PRESETS, ...remote])];
        for (const s of state.followedOrgs) {
          if (!merged.includes(s)) merged.push(s);
        }
        orgCandidates = merged;
        renderOrgList(el.orgSearch.value.trim());
      } catch {
        /* 在线补全失败不影响本地筛选 */
      }
    }, 350);
  }
});

el.orgClearBtn.addEventListener("click", () => {
  state.followedOrgs = [];
  savePrefs();
  updateOrgBtn();
  renderOrgList(el.orgSearch.value.trim());
  void reload();
});

// ---------- 时间范围快捷 ----------
function updateTimeSeg() {
  el.timeSeg.querySelectorAll<HTMLButtonElement>(".seg-btn").forEach((b) => {
    b.classList.toggle("active", b.dataset.range === state.timeRange);
  });
}

el.timeSeg.addEventListener("click", (e) => {
  const btn = (e.target as HTMLElement).closest<HTMLButtonElement>(".seg-btn");
  const range = btn?.dataset.range as TimeRange | undefined;
  if (!range || range === state.timeRange) return;
  state.timeRange = range;
  savePrefs();
  updateTimeSeg();
  void reload();
});

// ---------- 移动端返回 ----------
/** 详情是否以「移动端单栏」形式打开。 */
function isDetailOpenOnMobile(): boolean {
  return el.detailPane.classList.contains("show-mobile");
}

/** 从详情返回列表（移动端单栏）。 */
function backToList() {
  el.listPane.classList.remove("hidden-mobile");
  el.detailPane.classList.remove("show-mobile");
}

window.addEventListener("popstate", () => {
  if (isDetailOpenOnMobile()) backToList();
});

// Android 物理返回键 / 返回手势：Tauri 2 由 app 插件的 back-button 事件提供。
// 桌面端没有该事件，先按 UA 判断，避免无谓注册与报错。
if (/Android/i.test(navigator.userAgent)) {
  onBackButtonPress(({ canGoBack }) => {
    if (isDetailOpenOnMobile()) {
      backToList();
      // 清掉我们推入的那条历史记录，保持历史栈干净
      if (history.state?.jluOaDetail) history.back();
    } else if (canGoBack) {
      // 列表页：兜底清掉残留状态，避免返回键「按了没反应」
      history.back();
    }
    // 列表页且无历史可退时不做处理，交给系统默认行为（退出应用）
  }).catch(() => {
    /* 插件不可用时忽略 */
  });
}

// ---------- 事件绑定 ----------
el.refreshBtn.addEventListener("click", () => {
  if (state.timeRange === "all") void loadBrowse(state.page);
  else void loadFeed();
});
el.feedRefreshBtn.addEventListener("click", () => void loadFeed());
el.searchBtn.addEventListener("click", () => {
  state.keyword = el.keywordInput.value.trim();
  state.searchType = Number(el.searchType.value);
  state.page = 1;
  void reload();
});
el.keywordInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") el.searchBtn.click();
});
el.firstBtn.addEventListener("click", () => {
  if (state.page > 1) void loadBrowse(1);
});
el.prevBtn.addEventListener("click", () => {
  if (state.page > 1) void loadBrowse(state.page - 1);
});
el.nextBtn.addEventListener("click", () => {
  if (state.page < state.totalPages) void loadBrowse(state.page + 1);
});
el.jumpBtn.addEventListener("click", () => {
  const p = Number(el.jumpInput.value);
  if (p >= 1 && p <= state.totalPages) void loadBrowse(p);
  else toast("请输入有效页码");
});
el.jumpInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") el.jumpBtn.click();
});

// ---------- 访问模式（直连 / 网页版 VPN） ----------
el.vpnBtn.addEventListener("click", () => toggleVpnPanel());

// 一键登录：在应用内置窗口里登录，后端自动抓取并验证会话
el.vpnLoginBtn.addEventListener("click", () => void startVpnLoginFlow());

el.vpnApplyBtn.addEventListener("click", () => {
  const url = el.vpnUrlInput.value.trim();
  if (!url) {
    toast("请先粘贴登录 VPN 后进入 OA 的完整地址");
    el.vpnUrlInput.focus();
    return;
  }
  if (!/^https?:\/\//i.test(url)) {
    toast("VPN 地址需要以 https:// 开头");
    return;
  }
  state.vpnUrl = url;
  state.vpnTicket = el.vpnTicketInput.value.trim();
  state.accessMode = "vpn";
  savePrefs();
  updateVpnUi();
  toggleVpnPanel(false);
  toast("已启用 VPN 访问");
  void reload();
});

el.vpnDisableBtn.addEventListener("click", () => {
  // 等待登录中：这个按钮当作「取消」用
  if (vpnLoginPending) {
    void cancelVpnLogin();
    return;
  }
  if (state.accessMode === "direct") {
    toggleVpnPanel(false);
    return;
  }
  state.accessMode = "direct";
  savePrefs();
  updateVpnUi();
  toggleVpnPanel(false);
  toast("已切回直连");
  void reload();
});

// ---------- 启动 ----------
loadPrefs();
updateOrgBtn();
updateTimeSeg();
applyStatusBarInset();
window.addEventListener("resize", applyStatusBarInset);
window.addEventListener("orientationchange", applyStatusBarInset);
updateVpnUi();
void reload();
