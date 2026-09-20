//! 基于 reqwest 的 HTTP 抓取层（默认启用 `http` 特性）。

use crate::models::{ListOptions, ListResult, NoticeDetail};
use crate::parse::{parse_detail, parse_list, parse_orgs};

pub const BASE: &str = "https://oa.jlu.edu.cn/defaultroot/";
pub const CHANNEL_ID: &str = "179577";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

type Result<T> = std::result::Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(err)
}

/// 拉取通知列表。
pub async fn fetch_list(opts: &ListOptions) -> Result<ListResult> {
    let client = http_client()?;
    let page = opts.page.unwrap_or(1).max(1);

    let mut pairs: Vec<(&str, String)> = vec![("channelId", CHANNEL_ID.to_string())];
    if let Some(org) = &opts.org {
        if !org.trim().is_empty() {
            pairs.push(("orgname", org.trim().to_string()));
        }
    }
    if let Some(kw) = &opts.keyword {
        if !kw.trim().is_empty() {
            pairs.push(("searchnr", kw.trim().to_string()));
            pairs.push(("searchlx", opts.search_type.unwrap_or(0).to_string()));
            if let Some(d) = &opts.date_range {
                if !d.is_empty() {
                    pairs.push(("searchDate", d.clone()));
                }
            }
        }
    }
    if page > 1 {
        pairs.push(("startPage", page.to_string()));
    }

    let url = format!("{BASE}PortalInformation!jldxList.action");
    let resp = client
        .get(&url)
        .query(&pairs)
        .send()
        .await
        .map_err(err)?;
    let html = resp.text().await.map_err(err)?;
    Ok(parse_list(&html, page))
}

/// 拉取单条通知详情。
pub async fn fetch_detail(id: &str) -> Result<NoticeDetail> {
    let client = http_client()?;
    let url = format!("{BASE}PortalInformation!getInformation.action");
    let resp = client
        .get(&url)
        .query(&[("id", id), ("channelId", CHANNEL_ID)])
        .send()
        .await
        .map_err(err)?;
    let html = resp.text().await.map_err(err)?;
    Ok(parse_detail(&html, id))
}

/// 按关键词模糊搜索组织名。
pub async fn search_orgs(query: &str) -> Result<Vec<String>> {
    let client = http_client()?;
    let url = format!("{BASE}PortalInformation!jldxList.action");
    let resp = client
        .get(&url)
        .query(&[
            ("channelId", CHANNEL_ID),
            ("searchnr", query),
            ("searchlx", "1"),
        ])
        .send()
        .await
        .map_err(err)?;
    let html = resp.text().await.map_err(err)?;
    Ok(parse_orgs(&html))
}

/// 生成附件下载直链（完成站内下载协议编码）。
pub async fn build_attachment_url(
    information_id: &str,
    filename: &str,
    name: &str,
) -> Result<String> {
    let client = http_client()?;
    let date_temp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();

    // 1) 检查附件是否存在
    let resp = client
        .post(format!("{BASE}rd/download/CheckExist.jsp"))
        .form(&[("attsave", filename), ("dateTemp", date_temp.as_str())])
        .send()
        .await
        .map_err(err)?;
    let v: serde_json::Value = resp.json().await.map_err(err)?;
    if v.get("exist").and_then(|x| x.as_str()) != Some("yes") {
        return Err("附件不存在".to_string());
    }

    // 2) 编码下载参数
    let res = format!("{}@{}@{}", filename, name, information_id);
    let resp = client
        .post(format!("{BASE}rd/download/BASEEncoderAjax.jsp"))
        .form(&[("res", res.as_str())])
        .send()
        .await
        .map_err(err)?;
    let encoded = resp.text().await.map_err(err)?.trim().to_string();
    if encoded.is_empty() {
        return Err("附件编码失败".to_string());
    }

    // 3) 最终直链
    Ok(format!("{BASE}rd/download/attachdownload.jsp?res={encoded}"))
}

/// 抓取站内图片并编码成 `data:` URL。
///
/// 正文里的图片是 `<img src="/defaultroot/upload/html/xxx.png">` 这类绝对路径。
/// 前端把相对地址补全为绝对地址后交给 WebView 直接加载，会受 WebView 的
/// 跨源 / 证书 / 代理环境差异影响而加载失败；这里改由 Rust 侧用同一条已验证可用的
/// HTTP 通道抓取，再内联为 data URL，前端可直接显示。
///
/// 出于安全考虑只允许抓取本站（oa.jlu.edu.cn）的地址，避免被当作任意 URL 代理。
pub async fn fetch_image(url: &str) -> Result<String> {
    let parsed = reqwest::Url::parse(url).map_err(err)?;
    if parsed.scheme() != "https" || parsed.host_str() != Some("oa.jlu.edu.cn") {
        return Err("仅支持抓取 https://oa.jlu.edu.cn 下的图片".to_string());
    }

    let client = http_client()?;
    let resp = client.get(parsed).send().await.map_err(err)?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("图片请求失败：HTTP {status}"));
    }

    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "image/png".to_string());

    let bytes = resp.bytes().await.map_err(err)?;

    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{encoded}"))
}
