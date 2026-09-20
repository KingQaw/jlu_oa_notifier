//! 纯 Rust 的 HTML 解析逻辑，不依赖任何网络 / 系统库。
//! 可在 `--no-default-features` 下独立编译与测试。

use regex::Regex;
use scraper::{Html, Selector};
use std::sync::OnceLock;

use crate::models::{Attachment, ListResult, NoticeDetail, NoticeItem};

/// 从详情链接中提取 id（形如 `PortalInformation!getInformation.action?id=12345&channelId=...`）。
fn extract_id(href: &str) -> Option<String> {
    let start = href.find("id=")? + 3;
    let rest = &href[start..];
    let end = rest.find('&').unwrap_or(rest.len());
    let id = &rest[..end];
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// 解析列表页 HTML。`page` 为当前页码（1 起始）。
pub fn parse_list(html: &str, page: u32) -> ListResult {
    let doc = Html::parse_document(html);

    let item_sel = Selector::parse("#itemContainer .li").unwrap();
    let title_sel = Selector::parse("a.font14").unwrap();
    let org_sel = Selector::parse("a.column").unwrap();
    let time_sel = Selector::parse("span.time").unwrap();
    let new_sel = Selector::parse("img[src*='new.gif']").unwrap();
    let font_sel = Selector::parse("font").unwrap();

    let mut items = Vec::new();
    for li in doc.select(&item_sel) {
        let Some(title_el) = li.select(&title_sel).next() else {
            continue;
        };
        let href = title_el.value().attr("href").unwrap_or("");
        let Some(id) = extract_id(href) else {
            continue;
        };

        let raw_title: String = title_el.text().collect();
        let trimmed = raw_title.trim();
        let title = trimmed
            .strip_prefix("[置顶]")
            .unwrap_or(trimmed)
            .trim()
            .to_string();
        let pinned = title_el
            .select(&font_sel)
            .any(|f| f.text().collect::<String>().contains("置顶"));

        let org = li
            .select(&org_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let time = li
            .select(&time_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let is_new = li.select(&new_sel).next().is_some();

        items.push(NoticeItem {
            id,
            title,
            org,
            time,
            pinned,
            is_new,
        });
    }

    let (total, total_pages) = parse_pagination(html);
    ListResult {
        items,
        total,
        page,
        total_pages,
    }
}

fn re_total() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"共&nbsp;(\d+)&nbsp;条记录").unwrap())
}

fn re_pages() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"/(\d+)&nbsp;页").unwrap())
}

fn parse_pagination(html: &str) -> (i64, i64) {
    let total = re_total()
        .captures(html)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i64>().ok())
        .unwrap_or(0);
    let total_pages = re_pages()
        .captures(html)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i64>().ok())
        .unwrap_or(1);
    (total, total_pages.max(1))
}

/// 解析详情页 HTML。
pub fn parse_detail(html: &str, id: &str) -> NoticeDetail {
    let doc = Html::parse_document(html);

    let title_sel = Selector::parse(".content_t").unwrap();
    let time_sel = Selector::parse(".content_time").unwrap();
    let org_sel = Selector::parse(".content_time span").unwrap();
    let content_sel = Selector::parse(".content_font").unwrap();
    let att_sel = Selector::parse(".news_aboutFile span[title]").unwrap();

    let title = doc
        .select(&title_sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    let org = doc
        .select(&org_sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    let time_raw: String = doc
        .select(&time_sel)
        .next()
        .map(|e| e.text().collect::<String>())
        .unwrap_or_default();
    let time = time_raw
        .replace(&org, "")
        .trim()
        .trim_matches(|c: char| c == ' ' || c == '\u{a0}')
        .to_string();

    let content_html = doc
        .select(&content_sel)
        .next()
        .map(|e| e.inner_html())
        .unwrap_or_default();

    let mut attachments = Vec::new();
    for span in doc.select(&att_sel) {
        let filename = span.value().attr("id").unwrap_or("").to_string();
        let name = span.value().attr("title").unwrap_or("").to_string();
        if !filename.is_empty() {
            attachments.push(Attachment { filename, name });
        }
    }

    NoticeDetail {
        id: id.to_string(),
        title,
        org,
        time,
        content_html,
        attachments,
    }
}

/// 从列表页提取出现的组织名（去重、保序）。
pub fn parse_orgs(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let org_sel = Selector::parse("a.column").unwrap();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for el in doc.select(&org_sel) {
        let name = el.text().collect::<String>().trim().to_string();
        if !name.is_empty() && seen.insert(name.clone()) {
            out.push(name);
        }
    }
    out
}
