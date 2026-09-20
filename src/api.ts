import { invoke } from "@tauri-apps/api/core";
import type { ListOptions, ListResult, NoticeDetail } from "./types";

/** 拉取通知列表（支持组织筛选、关键词、日期范围、分页）。 */
export function fetchList(opts: ListOptions): Promise<ListResult> {
  return invoke<ListResult>("fetch_list", { opts });
}

/** 拉取单条通知详情（标题、时间、组织、正文、附件）。 */
export function fetchDetail(id: string): Promise<NoticeDetail> {
  return invoke<NoticeDetail>("fetch_detail", { id });
}

/** 按关键词模糊搜索组织名（用于组织筛选下拉的在线补全）。 */
export function searchOrgs(query: string): Promise<string[]> {
  return invoke<string[]>("search_orgs", { query });
}

/** 生成附件下载直链（后端完成站内下载协议编码）。 */
export function buildAttachmentUrl(
  informationId: string,
  filename: string,
  name: string,
): Promise<string> {
  return invoke<string>("build_attachment_url", {
    informationId,
    filename,
    name,
  });
}

/** 由后端抓取站内图片，返回可直接显示的 data URL。 */
export function fetchImage(url: string): Promise<string> {
  return invoke<string>("fetch_image", { url });
}
