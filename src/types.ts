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
}
