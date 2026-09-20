// 与 Rust 后端（oa-core）约定的数据模型保持一致。

export interface NoticeItem {
  id: string;
  title: string;
  org: string;
  time: string;
  pinned: boolean;
  isNew: boolean;
}

export interface ListResult {
  items: NoticeItem[];
  total: number;
  page: number;
  totalPages: number;
}

export interface Attachment {
  filename: string;
  name: string;
}

export interface NoticeDetail {
  id: string;
  title: string;
  org: string;
  time: string;
  contentHtml: string;
  attachments: Attachment[];
}

/** 列表查询参数；searchType: 0=标题 1=组织 2=内容；dateRange: ""|"1"|"6"|"12" */
export interface ListOptions {
  org?: string;
  page?: number;
  keyword?: string;
  searchType?: number;
  dateRange?: string;
  access?: Access;
}

/**
 * 访问模式，与 Rust 侧 `oa_core::Access` 的 serde 表示一一对应
 * （内部标签 `mode`，variant 重命名为 camelCase）。
 *
 * - direct：直连 https://oa.jlu.edu.cn（校内网）
 * - vpn：经 https://vpn.jlu.edu.cn 的网页版 VPN 转发
 */
export type Access =
  | { mode: "direct" }
  | {
      mode: "vpn";
      /** 站点前缀，形如 https://vpn.jlu.edu.cn/https/<加密串>/defaultroot/ */
      prefix: string;
      /** 网页 VPN 会话票据（Cookie wengine_vpn_ticketvpn_jlu_edu_cn 的值） */
      ticket?: string;
    };

/** 后端在网页 VPN 未登录（或票据过期）时返回的错误标识。 */
export const NEED_VPN_LOGIN = "NEED_VPN_LOGIN";

/** 后端在直连不可达（多半是没连校园网/没用 VPN）时返回的错误标识。 */
export const NETWORK_UNREACHABLE = "NETWORK_UNREACHABLE";

/** 网页版 VPN 登录页。 */
export const VPN_LOGIN_URL = "https://vpn.jlu.edu.cn/login";
