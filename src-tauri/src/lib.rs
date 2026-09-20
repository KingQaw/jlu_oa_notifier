use oa_core::{Access, ListOptions, ListResult, NoticeDetail};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager};

/// 网页版 VPN 入口。
const VPN_LOGIN_URL: &str = "https://vpn.jlu.edu.cn/login";
/// 与 oa-core 抓取层保持一致的 UA：直接访问 oa 主页面拿 defaultroot 加密段
/// 会比较稳定，UA 差异过大时网关可能拒绝。
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
const LOGIN_WINDOW_LABEL: &str = "vpn-login";
const LOGIN_TIMEOUT_SECS: u64 = 300;

/// 拉取通知列表（组织筛选 / 关键词 / 日期范围 / 分页）。
/// 访问模式随 `opts.access` 传入：不传即直连，传 Vpn 即走网页版 VPN。
#[tauri::command]
async fn fetch_list(opts: ListOptions) -> Result<ListResult, String> {
    oa_core::fetch_list(&opts).await
}

/// 拉取单条通知详情。
#[tauri::command]
async fn fetch_detail(id: String, access: Option<Access>) -> Result<NoticeDetail, String> {
    oa_core::fetch_detail(&id, &access.unwrap_or_default()).await
}

/// 按关键词模糊搜索组织名。
#[tauri::command]
async fn search_orgs(query: String, access: Option<Access>) -> Result<Vec<String>, String> {
    oa_core::search_orgs(&query, &access.unwrap_or_default()).await
}

/// 生成附件下载直链。
#[tauri::command]
async fn build_attachment_url(
    information_id: String,
    filename: String,
    name: String,
    access: Option<Access>,
) -> Result<String, String> {
    oa_core::build_attachment_url(&information_id, &filename, &name, &access.unwrap_or_default())
        .await
}

/// 抓取站内图片并返回 data URL（正文图片改由后端加载，规避 WebView 跨源/证书差异）。
#[tauri::command]
async fn fetch_image(url: String, access: Option<Access>) -> Result<String, String> {
    oa_core::fetch_image(&url, &access.unwrap_or_default()).await
}

/// 登录结果，回传给前端的 `vpn-login-result` 事件。
#[derive(Clone, serde::Serialize)]
struct VpnLoginResponse {
    ok: bool,
    /// 成功时的站点前缀（`.../https/<加密串>/defaultroot/`）
    prefix: Option<String>,
    /// 成功时可直接用于请求的 Cookie 串
    cookies: Option<String>,
    /// 失败原因（用户可读）
    message: Option<String>,
}

/// 从任意网关转发地址中提取 `<加密串>`。
///
/// 网瑞达把目标站点编码成一段加密串，形如
/// `https://vpn.jlu.edu.cn/https/<加密串>/<目标站内路径>`。
/// 关键点：**这段加密串与目标站点无关**——访问用户门户时它是
/// `.../https/<加密串>/user/portal`，访问 oa 时是
/// `.../https/<加密串>/defaultroot/...`，两处的 `<加密串>` 一致。
/// 因此登录成功后从用户门户地址里就能把它取出来，再拼上 oa 的相对路径即可。
fn extract_secret(url: &str) -> Option<String> {
    const MARK: &str = "/https/";
    let rest = url.split_once(MARK)?.1;
    let seg = rest.split('/').next()?;
    if seg.is_empty() {
        None
    } else {
        Some(seg.to_string())
    }
}

/// 用加密串拼出 oa 的站点根前缀。
fn oa_root_from_secret(secret: &str) -> String {
    format!("https://vpn.jlu.edu.cn/https/{secret}/defaultroot/")
}

/// 把已带 defaultroot 的网关地址截断成站点根前缀。
fn vpn_root_of(url: &str) -> Option<String> {
    const MARK: &str = "/defaultroot";
    let i = url.find(MARK)?;
    Some(format!("{}{}/", &url[..i], MARK))
}

/// 读取该地址在 webview 中的全部 Cookie，拼成请求可用的 Cookie 头。
///
/// `cookies_for_url` 是同步方法，返回的正是"会发给该 URL 的那些 Cookie"，
/// 因此域/路径匹配由 WebView 自己处理，我们不必手工筛选。
fn cookies_for(window: &tauri::WebviewWindow, url: &tauri::Url) -> Option<String> {
    let jar = window.cookies_for_url(url.clone()).ok()?;
    let joined = jar
        .iter()
        .map(|c| format!("{}={}", c.name(), c.value()))
        .collect::<Vec<_>>()
        .join("; ");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

/// 用候选会话真实调一次列表接口，确认它确实可用。
///
/// 只凭"页面上出现了 defaultroot 地址"是不够的：`/login` 本身也会被网关转发，
/// 用户可能还没登录成功；只有能真正取到数据才算成功。
async fn verify_session(prefix: &str, cookies: &str) -> bool {
    let access = Access::Vpn {
        prefix: prefix.to_string(),
        cookies: Some(cookies.to_string()),
    };
    oa_core::fetch_list(&ListOptions {
        access: Some(access),
        page: Some(1),
        ..Default::default()
    })
    .await
    .is_ok()
}

/// 打开内置登录窗口，用户登录后自动抓取并验证会话，结果通过事件回传。
///
/// 全程不需要用户手动复制地址或 Cookie：登录在应用内的真实页面上完成，
/// 会话由 webview 的 cookie 接口读取（HttpOnly 也能读到）。
#[tauri::command]
async fn vpn_login(app: tauri::AppHandle) -> Result<(), String> {
    // 已存在登录窗口时先关掉，避免出现两个
    if let Some(existing) = app.get_webview_window(LOGIN_WINDOW_LABEL) {
        let _ = existing.destroy();
    }

    // 注意：Windows 上在同步命令里创建 webview 会死锁，必须在独立线程中创建
    // （见 tauri WebviewWindowBuilder::new 的 Known issues）。
    let handle = app.clone();
    let closed = Arc::new(AtomicBool::new(false));

    std::thread::spawn(move || {
        let url = match VPN_LOGIN_URL.parse() {
            Ok(u) => u,
            Err(e) => {
                let _ = handle.emit(
                    "vpn-login-result",
                    VpnLoginResponse {
                        ok: false,
                        prefix: None,
                        cookies: None,
                        message: Some(format!("登录地址无效：{e}")),
                    },
                );
                return;
            }
        };

        let window = match tauri::WebviewWindowBuilder::new(
            &handle,
            LOGIN_WINDOW_LABEL,
            tauri::WebviewUrl::External(url),
        )
        .title("登录吉大 VPN（登录成功后本窗口会自动关闭）")
        .inner_size(920.0, 760.0)
        .user_agent(UA)
        .build()
        {
            Ok(w) => w,
            Err(e) => {
                let _ = handle.emit(
                    "vpn-login-result",
                    VpnLoginResponse {
                        ok: false,
                        prefix: None,
                        cookies: None,
                        message: Some(format!("无法打开登录窗口：{e}")),
                    },
                );
                return;
            }
        };

        // 用户手动关闭窗口时，优雅地报错而不是等到超时
        let closed_flag = closed.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                closed_flag.store(true, Ordering::SeqCst);
            }
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(LOGIN_TIMEOUT_SECS);
        let mut last_candidate: Option<String> = None;

        loop {
            if closed.load(Ordering::SeqCst) {
                let _ = handle.emit(
                    "vpn-login-result",
                    VpnLoginResponse {
                        ok: false,
                        prefix: None,
                        cookies: None,
                        message: Some("登录窗口已关闭，未检测到有效会话".to_string()),
                    },
                );
                return;
            }

            if std::time::Instant::now() > deadline {
                let _ = window.destroy();
                let _ = handle.emit(
                    "vpn-login-result",
                    VpnLoginResponse {
                        ok: false,
                        prefix: None,
                        cookies: None,
                        message: Some("登录超时（5 分钟），请重试".to_string()),
                    },
                );
                return;
            }

            // 当前地址：登录成功后网关会跳转，用户门户与 oa 页面都带着同一段加密串
            let current = window.url().ok().map(|u| u.to_string());

            // 候选前缀优先级：
            //  1. 当前地址已带 defaultroot（用户已经点到 oa）→ 直接截断
            //  2. 当前地址里的加密串（登录后停在用户门户也能拿到）→ 拼出 oa 前缀
            //
            // 刻意不做"请求明文 /https/oa.jlu.edu.cn/ 让网关规范化"的兜底：那种
            // 地址不含加密串，能否配合会话工作未经证实，不把不确定性放进主流程。
            let mut candidates: Vec<String> = Vec::new();
            if let Some(u) = &current {
                if let Some(root) = vpn_root_of(u) {
                    candidates.push(root);
                }
                if let Some(secret) = extract_secret(u) {
                    candidates.push(oa_root_from_secret(&secret));
                }
            }
            candidates.dedup();

            for root in candidates {
                if last_candidate.as_deref() == Some(root.as_str()) {
                    // 同一个前缀上一次已经验证失败，不必反复打
                    continue;
                }
                let Ok(cookie_url) = root.parse::<tauri::Url>() else {
                    continue;
                };
                let Some(cookies) = cookies_for(&window, &cookie_url) else {
                    continue;
                };

                if tauri::async_runtime::block_on(verify_session(&root, &cookies)) {
                    let _ = window.destroy();
                    let _ = handle.emit(
                        "vpn-login-result",
                        VpnLoginResponse {
                            ok: true,
                            prefix: Some(root),
                            cookies: Some(cookies),
                            message: None,
                        },
                    );
                    return;
                }
                last_candidate = Some(root);
            }

            std::thread::sleep(std::time::Duration::from_millis(800));
        }
    });

    Ok(())
}

/// 关闭内置登录窗口（用户取消时调用）。
#[tauri::command]
async fn close_vpn_login(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW_LABEL) {
        w.destroy().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            fetch_list,
            fetch_detail,
            search_orgs,
            build_attachment_url,
            fetch_image,
            vpn_login,
            close_vpn_login
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 加密串与目标站点无关：用户门户与 oa 地址里的那一段必须一致。
    #[test]
    fn extracts_secret_from_any_forwarded_url() {
        let portal = "https://vpn.jlu.edu.cn/https/48714f7134abcdef/user/portal";
        let oa = "https://vpn.jlu.edu.cn/https/48714f7134abcdef/defaultroot/PortalInformation!jldxList.action";
        assert_eq!(extract_secret(portal).unwrap(), "48714f7134abcdef");
        assert_eq!(extract_secret(oa).unwrap(), "48714f7134abcdef");
    }

    #[test]
    fn builds_oa_root_from_secret() {
        assert_eq!(
            oa_root_from_secret("abc123"),
            "https://vpn.jlu.edu.cn/https/abc123/defaultroot/"
        );
    }

    #[test]
    fn truncates_url_at_defaultroot() {
        assert_eq!(
            vpn_root_of(
                "https://vpn.jlu.edu.cn/https/abc/defaultroot/PortalInformation!jldxList.action?channelId=179577"
            )
            .unwrap(),
            "https://vpn.jlu.edu.cn/https/abc/defaultroot/"
        );
    }

    /// 登录页本身也在网关下，但不应被当成可用前缀。
    #[test]
    fn ignores_non_forwarded_urls() {
        assert_eq!(vpn_root_of("https://vpn.jlu.edu.cn/login"), None);
        assert_eq!(extract_secret("https://vpn.jlu.edu.cn/login"), None);
    }
}
