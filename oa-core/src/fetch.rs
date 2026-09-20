//! 基于 reqwest 的 HTTP 抓取层（默认启用 `http` 特性）。
//!
//! 支持两种访问模式（见 [`Access`]）：
//! - **直连**：直接访问 `https://oa.jlu.edu.cn`，用于校内网/已连接 VPN 客户端的环境。
//! - **WebVPN 网关**：经 `https://vpn.jlu.edu.cn` 的网页版 VPN 转发，
//!   用于没有校园网时通过网页 VPN 访问。
//!
//! 网页 VPN 会把目标站点编码成一段**加密前缀**（形如
//! `https://vpn.jlu.edu.cn/https/<加密串>/defaultroot/...`）。这段加密串无法由
//! 本程序推导，因此这里不做任何解码/推导，而是把它作为「用户在地址栏里看到的
//! 权威值」原样保存并复用——站点前缀保持不变，后面拼上本站的相对路径即可。

use crate::models::{Access, ListOptions, ListResult, NoticeDetail};
use crate::parse::{parse_detail, parse_list, parse_orgs};

/// 直连模式下的站点根地址（含 `defaultroot/`）。
pub const BASE: &str = "https://oa.jlu.edu.cn/defaultroot/";
pub const CHANNEL_ID: &str = "179577";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

type Result<T> = std::result::Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// 去掉结尾多余的 `/`，避免拼接出 `//`。
fn trim_end_slash(s: &str) -> &str {
    s.trim_end_matches('/')
}

/// 取「站点根地址 + defaultroot/」形式的前缀。
///
/// 用户从浏览器地址栏复制来的 URL 形态不固定，可能停在
/// `.../PortalInformation!jldxList.action?channelId=179577` 这样的具体页面上，
/// 因此这里统一归一化到 `defaultroot/` 结尾：
/// - 含 `defaultroot` → 截取到 `defaultroot` 为止（丢弃后面的页面路径与查询串）
/// - 不含 `defaultroot` → 视为已是站点根，直接补上 `defaultroot/`
///
/// 这样用户在 __VPN__ 模式下无论粘贴哪个页面的 URL，都能正常使用。
fn normalize_root(raw: &str) -> Result<String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("VPN 地址为空".to_string());
    }
    let parsed = reqwest::Url::parse(s).map_err(|e| format!("VPN 地址不是合法的 URL：{e}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("VPN 地址必须是 http(s) 地址".to_string());
    }

    const MARK: &str = "defaultroot";
    match s.find(MARK) {
        Some(idx) => Ok(format!("{}/", trim_end_slash(&s[..idx + MARK.len()]))),
        None => Ok(format!("{}/defaultroot/", trim_end_slash(s))),
    }
}

/// 构造实际请求用的完整地址（默认直连）。
fn url_for(base: &str, relative: &str) -> String {
    format!("{base}{relative}")
}

/// 某个 VPN 站点前缀是否可安全复用（防 SSRF：只允许转发到吉大网关）。
///
/// 校验条件（注意：目标站点域名被网关**加密**在路径段里，明文是看不到的，
/// 因此不能靠"路径含 oa.jlu.edu.cn"来判断）：
/// - 必须是 https；
/// - 主机必须是吉大 VPN 网关；
/// - 转发路径必须以 `/https/` 或 `/http/` 开头，且加密段非空。
fn is_allowed_vpn_prefix(prefix: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(prefix) else {
        return false;
    };
    if u.scheme() != "https" {
        return false;
    }
    let host_ok = matches!(
        u.host_str(),
        Some("vpn.jlu.edu.cn") | Some("webvpn.jlu.edu.cn")
    );
    if !host_ok {
        return false;
    }

    let mut segs = u.path().split('/').filter(|s| !s.is_empty());
    let scheme_seg = segs.next();
    let secret = segs.next();
    let scheme_ok = matches!(scheme_seg, Some("https") | Some("http"));
    let secret_ok = secret.map(|s| !s.is_empty()).unwrap_or(false);
    scheme_ok && secret_ok
}

/// 从访问模式得出站点根前缀（含 `defaultroot/`）。
fn root_for(access: &Access) -> Result<String> {
    match access {
        Access::Direct => Ok(BASE.to_string()),
        Access::Vpn { prefix, .. } => {
            let root = normalize_root(prefix)?;
            if !is_allowed_vpn_prefix(&root) {
                return Err(
                    "VPN 地址不在允许范围内（需要 vpn.jlu.edu.cn 的 /https/ 转发地址，且指向 oa.jlu.edu.cn）"
                        .to_string(),
                );
            }
            Ok(root)
        }
    }
}

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        // VPN 网关的会话是 HttpOnly Cookie；这里不主动管理会话，而是把
        // 内置登录窗口抓到的 Cookie 串按请求带上（见 `apply_cookies`）。
        .build()
        .map_err(err)
}

/// 把网页 VPN 的会话 Cookie 附加到请求上。
///
/// 网页 VPN 用 HttpOnly Cookie 保存登录态，页面 JS 读不到，因此由应用内置的
/// 登录窗口在用户登录后自动抓取并传进来（见 tauri 层的 `vpn_login`）。
fn apply_cookies(
    mut req: reqwest::RequestBuilder,
    cookies: Option<&str>,
) -> reqwest::RequestBuilder {
    if let Some(c) = cookies {
        let c = c.trim().trim_end_matches(';');
        if !c.is_empty() {
            req = req.header(reqwest::header::COOKIE, c);
        }
    }
    req
}

/// 拉取通知列表。
pub async fn fetch_list(opts: &ListOptions) -> Result<ListResult> {
    let client = http_client()?;
    let page = opts.page.unwrap_or(1).max(1);
    let access = opts.access.clone().unwrap_or_default();
    let root = root_for(&access)?;

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

    let url = url_for(&root, "PortalInformation!jldxList.action");
    let resp = apply_cookies(client.get(&url), access.cookies())
        .query(&pairs)
        .send()
        .await
        .map_err(|e| map_request_error(err(e), &access))?;
    if is_login_redirect(resp.url().as_str()) {
        return Err(NEED_LOGIN.to_string());
    }
    let html = resp.text().await.map_err(err)?;
    Ok(parse_list(&html, page))
}

/// 拉取单条通知详情。
pub async fn fetch_detail(id: &str, access: &Access) -> Result<NoticeDetail> {
    let client = http_client()?;
    let root = root_for(access)?;
    let url = url_for(&root, "PortalInformation!getInformation.action");
    let resp = apply_cookies(client.get(&url), access.cookies())
        .query(&[("id", id), ("channelId", CHANNEL_ID)])
        .send()
        .await
        .map_err(|e| map_request_error(err(e), &access))?;
    if is_login_redirect(resp.url().as_str()) {
        return Err(NEED_LOGIN.to_string());
    }
    let html = resp.text().await.map_err(err)?;
    Ok(parse_detail(&html, id))
}

/// 按关键词模糊搜索组织名。
pub async fn search_orgs(query: &str, access: &Access) -> Result<Vec<String>> {
    let client = http_client()?;
    let root = root_for(access)?;
    let url = url_for(&root, "PortalInformation!jldxList.action");
    let resp = apply_cookies(client.get(&url), access.cookies())
        .query(&[
            ("channelId", CHANNEL_ID),
            ("searchnr", query),
            ("searchlx", "1"),
        ])
        .send()
        .await
        .map_err(|e| map_request_error(err(e), &access))?;
    if is_login_redirect(resp.url().as_str()) {
        return Err(NEED_LOGIN.to_string());
    }
    let html = resp.text().await.map_err(err)?;
    Ok(parse_orgs(&html))
}

/// 生成附件下载直链（完成站内下载协议编码）。
pub async fn build_attachment_url(
    information_id: &str,
    filename: &str,
    name: &str,
    access: &Access,
) -> Result<String> {
    let client = http_client()?;
    let root = root_for(access)?;
    let cookies = access.cookies();
    let date_temp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();

    // 1) 检查附件是否存在
    let resp = apply_cookies(
        client.post(url_for(&root, "rd/download/CheckExist.jsp")),
        cookies,
    )
    .form(&[("attsave", filename), ("dateTemp", date_temp.as_str())])
    .send()
    .await
    .map_err(|e| map_request_error(err(e), &access))?;
    if is_login_redirect(resp.url().as_str()) {
        return Err(NEED_LOGIN.to_string());
    }
    let v: serde_json::Value = resp.json().await.map_err(err)?;
    if v.get("exist").and_then(|x| x.as_str()) != Some("yes") {
        return Err("附件不存在".to_string());
    }

    // 2) 编码下载参数
    let res = format!("{}@{}@{}", filename, name, information_id);
    let resp = apply_cookies(
        client.post(url_for(&root, "rd/download/BASEEncoderAjax.jsp")),
        cookies,
    )
    .form(&[("res", res.as_str())])
    .send()
    .await
    .map_err(|e| map_request_error(err(e), &access))?;
    let encoded = resp.text().await.map_err(err)?.trim().to_string();
    if encoded.is_empty() {
        return Err("附件编码失败".to_string());
    }

    // 3) 最终直链（同样走当前访问模式，VPN 模式下由系统浏览器带着网关会话下载）
    Ok(url_for(
        &root,
        &format!("rd/download/attachdownload.jsp?res={encoded}"),
    ))
}

/// 抓取站内图片并编码成 `data:` URL。
///
/// 正文里的图片是 `<img src="/defaultroot/upload/html/xxx.png">` 这类绝对路径。
/// 前端把相对地址补全为绝对地址后交给 WebView 直接加载，会受 WebView 的
/// 跨源 / 证书 / 代理环境差异影响而加载失败；这里改由 Rust 侧用同一条已验证可用的
/// HTTP 通道抓取，再内联为 data URL，前端可直接显示。
///
/// 出于安全考虑只允许抓取本站（直连模式为 `oa.jlu.edu.cn`，VPN 模式为配置的
/// 网关前缀之下），避免被当作任意 URL 代理。
pub async fn fetch_image(url: &str, access: &Access) -> Result<String> {
    let root = root_for(access)?;
    let parsed = reqwest::Url::parse(url).map_err(err)?;

    // 允许两种来源：直连的 oa 站点，或当前配置的 VPN 网关前缀之下
    let ok_direct = parsed.scheme() == "https" && parsed.host_str() == Some("oa.jlu.edu.cn");
    let ok_vpn = url.starts_with(&root);
    if !ok_direct && !ok_vpn {
        return Err(
            "仅支持抓取本校站点（oa.jlu.edu.cn）或当前 VPN 网关前缀下的图片".to_string(),
        );
    }

    let client = http_client()?;
    let resp = apply_cookies(client.get(parsed), access.cookies())
        .send()
        .await
        .map_err(|e| map_request_error(err(e), &access))?;
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

/// 网页 VPN 未登录时的错误标识：前端据此提示"需要登录 VPN / 更新票据"，
/// 而不是笼统地报"加载失败"。
pub const NEED_LOGIN: &str = "NEED_VPN_LOGIN";

/// 让网关把给定的校内地址规范化成"转发地址"，返回最终地址。
///
/// 用途：应用内置的登录窗口需要知道 `<加密串>` 才能开始请求。这个加密串
/// 由网关自己生成，本程序无法推导，所以这里直接问网关：
/// 请求 `https://vpn.jlu.edu.cn/https/<校内地址>`，网关会把地址规范化，
/// 并在 `Location`（或最终地址）里给出带真实加密串的转发地址。
///
/// 之所以不依赖前端页面的导航事件，是因为网瑞达门户是 SPA，页面内部跳转
/// 未必触发导航回调；而这一步只依赖网关自身行为，更稳。
///
/// 出于安全考虑，只允许对固定的校内 host 做规范化（不接受任意 URL）。
pub async fn probe_forwarded_url(target: &str) -> Option<String> {
    const ALLOWED_HOSTS: [&str; 2] = ["oa.jlu.edu.cn", "vpn.jlu.edu.cn"];
    let parsed = reqwest::Url::parse(target).ok()?;
    if !ALLOWED_HOSTS.contains(&parsed.host_str()?) {
        return None;
    }

    // 网关的转发格式是 /https/<host>/<path>（注意不含 "https://" 前缀）
    let host_and_path = format!("{}{}", parsed.host_str()?, parsed.path());
    let gateway = format!("https://vpn.jlu.edu.cn/https/{host_and_path}");
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;

    let resp = client.get(&gateway).send().await.ok()?;
    // 网关通常用 302 把规范化后的地址放在 Location 里
    if let Some(loc) = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
    {
        if loc.contains("/https/") {
            return Some(loc.to_string());
        }
    }
    // 退而求其次：若直接返回了内容，则最终地址本身就是规范化结果
    let final_url = resp.url().to_string();
    if final_url.contains("/https/") && final_url.contains("defaultroot") {
        return Some(final_url);
    }
    None
}

/// 直连不可达（多半是没连校园网/没用 VPN）时的错误标识。
/// 前端据此提示"是否改用 VPN 访问"，比只显示底层错误友好得多。
pub const NETWORK_UNREACHABLE: &str = "NETWORK_UNREACHABLE";

fn is_login_redirect(final_url: &str) -> bool {
    final_url.contains("/login")
}

/// 判断底层请求错误是否属于「连不上」这类网络可达性问题。
fn looks_unreachable(e: &str) -> bool {
    let s = e.to_ascii_lowercase();
    // reqwest/hyper 在连不上时的典型措辞
    s.contains("error sending request")
        || s.contains("connection refused")
        || s.contains("connection reset")
        || s.contains("dns error")
        || s.contains("failed to lookup address")
        || s.contains("tcp connect error")
        || s.contains("network is unreachable")
        || s.contains("timed out")
}

/// 直连模式下把"连不上"翻译成可操作的提示；VPN 模式保持原样（网关可达但未登录已单独处理）。
fn map_request_error(e: String, access: &Access) -> String {
    if !access.is_vpn() && looks_unreachable(&e) {
        format!("{NETWORK_UNREACHABLE}（直连失败：{e}）")
    } else {
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用户在 VPN 模式下可能粘贴具体页面地址，需归一化到 defaultroot/。
    #[test]
    fn normalizes_vpn_url_to_defaultroot_root() {
        let got = normalize_root(
            "https://vpn.jlu.edu.cn/https/48714f7134/defaultroot/PortalInformation!jldxList.action?channelId=179577",
        )
        .unwrap();
        assert_eq!(
            got,
            "https://vpn.jlu.edu.cn/https/48714f7134/defaultroot/"
        );
    }

    /// 粘贴的地址若已停在目录层级，也要能正确补全。
    #[test]
    fn normalizes_vpn_url_without_defaultroot() {
        let got = normalize_root("https://vpn.jlu.edu.cn/https/48714f7134").unwrap();
        assert_eq!(got, "https://vpn.jlu.edu.cn/https/48714f7134/defaultroot/");
    }

    #[test]
    fn rejects_non_vpn_prefix() {
        // 非吉大网关主机一律拒绝
        assert!(!is_allowed_vpn_prefix("https://evil.example.com/https/oa.jlu.edu.cn/"));
        assert!(!is_allowed_vpn_prefix("https://vpn.other.edu.cn/https/oa.jlu.edu.cn/"));
        // 缺少 /https/ 转发路径段
        assert!(!is_allowed_vpn_prefix("https://vpn.jlu.edu.cn/oa.jlu.edu.cn/"));
        // 非 https
        assert!(!is_allowed_vpn_prefix("http://vpn.jlu.edu.cn/https/abc/"));
    }

    /// 回归：真实前缀的加密段里**不含**目标域名明文，
    /// 早期版本误把「路径含 oa.jlu.edu.cn」当作校验条件，导致真地址被拒。
    #[test]
    fn accepts_real_vpn_prefix() {
        let p = "https://vpn.jlu.edu.cn/https/48714f71342f7a336d582f7e2857373750cd3d1004df80a0b5971c1b1a/defaultroot/";
        assert!(is_allowed_vpn_prefix(p), "真实 VPN 前缀必须被接受");
    }

    /// 加密段非空的形如地址也应接受（用户可能粘贴不含 defaultroot 的形态）
    #[test]
    fn accepts_prefix_without_defaultroot() {
        assert!(is_allowed_vpn_prefix(
            "https://vpn.jlu.edu.cn/https/48714f71342f7a336d582f7e2857373750cd3d1004df80a0b5971c1b1a/"
        ));
    }

    #[test]
    fn url_for_joins_without_double_slash() {
        assert_eq!(
            url_for("https://oa.jlu.edu.cn/defaultroot/", "PortalInformation!jldxList.action"),
            "https://oa.jlu.edu.cn/defaultroot/PortalInformation!jldxList.action"
        );
    }

    #[test]
    fn cookies_are_exposed_in_vpn_mode() {
        let a = Access::Vpn {
            prefix: "x".into(),
            cookies: Some("a=1; b=2".into()),
        };
        assert_eq!(a.cookies(), Some("a=1; b=2"));
    }

    #[test]
    fn direct_mode_has_no_cookies() {
        assert_eq!(Access::Direct.cookies(), None);
    }
}
