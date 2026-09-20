import { invoke } from "@tauri-apps/api/core";
import type { Access, ListOptions, ListResult, NoticeDetail } from "./types";

/** 拉取通知列表（支持组织筛选、关键词、日期范围、分页、访问模式）。 */
export function fetchList(opts: ListOptions): Promise<ListResult> {
  return invoke<ListResult>("fetch_list", { opts });
}

/** 拉取单条通知详情。 */
export function fetchDetail(id: string, access?: Access): Promise<NoticeDetail> {
  return invoke<NoticeDetail>("fetch_detail", { id, access });
}

/** 按关键词模糊搜索组织名（用于组织筛选下拉的在线补全）。 */
export function searchOrgs(query: string, access?: Access): Promise<string[]> {
  return invoke<string[]>("search_orgs", { query, access });
}

/** 生成附件下载直链（后端完成站内下载协议编码）。 */
export function buildAttachmentUrl(
  informationId: string,
  filename: string,
  name: string,
  access?: Access,
): Promise<string> {
  return invoke<string>("build_attachment_url", {
    informationId,
    filename,
    name,
    access,
  });
}

/** 由后端抓取站内图片，返回可直接显示的 data URL。 */
export function fetchImage(url: string, access?: Access): Promise<string> {
  return invoke<string>("fetch_image", { url, access });
}

/** 在系统浏览器中打开网页版 VPN，供用户登录后取得会话票据。 */
export function openVpnLogin(url: string): Promise<void> {
  return invoke<void>("open_vpn_login", { url });
}

/**
 * 打开内置登录窗口。用户在其中正常登录后，后端会自动抓取并验证会话，
 * 结果通过 `vpn-login-result` 事件回传（见 types.ts 的 VpnLoginResult）。
 */
export function startVpnLogin(): Promise<void> {
  return invoke<void>("vpn_login");
}

/** 关闭内置登录窗口（用户取消时调用）。 */
export function closeVpnLogin(): Promise<void> {
  return invoke<void>("close_vpn_login");
}
