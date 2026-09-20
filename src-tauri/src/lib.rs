use oa_core::{Access, ListOptions, ListResult, NoticeDetail};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

/// 网页版 VPN 入口。
const VPN_LOGIN_URL: &str = "https://vpn.jlu.edu.cn/login";
const LOGIN_WINDOW_LABEL: &str = "vpn-login";
const LOGIN_TIMEOUT_SECS: u64 = 300;
/// 门户是 SPA，磁贴需等渲染/接口返回；最多尝试自动进入 OA 这么多次。
/// 每轮轮询约 800ms，40 次≈32 秒，足够覆盖门户加载较慢的情况（实测第 13 次成功）。
const AUTO_CLICK_MAX_TRIES: u32 = 40;

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

/// 从网关转发地址中取出 `<加密串>` 那一段（仅用于日志/诊断）。
///
/// ⚠️ 曾经的误判：一度以为这段加密串"与目标站点无关"、可以从门户地址里取一次
/// 再拼到 oa 前面。实测证明它是**按目标资源分别生成的**——同一会话下
/// 门户与 oa 的加密串并不相同：
///
/// ```text
/// 门户/tpass: 48714f71342f7a336d582f7e285737375 ccd605402c6dbebb4864756174f
/// oa:         48714f71342f7a336d582f7e2857373750cd3d1004df80a0b5971c1b1a
/// ```
///
/// 所以绝不能复用，也不能自己实现门户那套 AES 逻辑（见 `portal.js` 的
/// `wrdvpnKey`/`wrdvpnIV`，密钥由页面按会话注入）。正确做法是复用门户
/// 自己生成出来的 oa 地址——即用户在窗口里点进「吉大 OA」后地址栏里的那个。
#[allow(dead_code)]
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
///
/// 仅用于自检/诊断。实际流程不这样构造 URL，原因见 [`extract_secret`]。
#[allow(dead_code)]
fn oa_root_from_secret(secret: &str) -> String {
    format!("https://vpn.jlu.edu.cn/https/{secret}/defaultroot/")
}

/// 把已带 defaultroot 的网关地址截断成站点根前缀。
fn vpn_root_of(url: &str) -> Option<String> {
    const MARK: &str = "/defaultroot";
    let i = url.find(MARK)?;
    Some(format!("{}{}/", &url[..i], MARK))
}

/// 最近一次读取会话 Cookie 的诊断信息。
///
/// 为什么需要它：`log_diag` 写的是 `%TEMP%` 下的文件，而 **Android 上应用进程
/// 读不到该路径**，日志等于白写。真机排查只能靠界面呈现（配 adb uiautomator
/// dump 读取），所以这里把关键诊断结果暂存下来，在失败提示里带给用户。
static LAST_COOKIE_DIAG: Mutex<String> = Mutex::new(String::new());

fn set_cookie_diag(msg: String) {
    if let Ok(mut g) = LAST_COOKIE_DIAG.lock() {
        *g = msg.clone();
    }
    log_diag(&msg);
}

fn take_cookie_diag() -> String {
    LAST_COOKIE_DIAG
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
}

/// 读取该地址在 webview 中的全部 Cookie，拼成请求可用的 Cookie 头。
///
/// `cookies_for_url` 是同步方法，返回的正是"会发给该 URL 的那些 Cookie"，
/// 因此域/路径匹配由 WebView 自己处理，我们不必手工筛选。
fn cookies_for(window: &tauri::WebviewWindow, url: &tauri::Url) -> Option<String> {
    let jar = match window.cookies_for_url(url.clone()) {
        Ok(v) => v,
        Err(e) => {
            set_cookie_diag(format!(
                "cookies_for_url 失败：{e}（android={}）",
                cfg!(target_os = "android")
            ));
            return None;
        }
    };
    set_cookie_diag(format!(
        "cookies_for_url 返回 {} 个（android={}）",
        jar.len(),
        cfg!(target_os = "android")
    ));
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
/// 用户可能还没登录成功。这里要求**真的解析出条目**，因为"请求成功但 0 条"
/// 既可能是会话无效，也可能是页面结构变了——两者都不能算登录成功。
async fn verify_session(prefix: &str, cookies: &str) -> (bool, String) {
    let access = Access::Vpn {
        prefix: prefix.to_string(),
        cookies: Some(cookies.to_string()),
    };
    match oa_core::fetch_list(&ListOptions {
        access: Some(access),
        page: Some(1),
        ..Default::default()
    })
    .await
    {
        Ok(list) => {
            let n = list.items.len();
            let ok = n > 0;
            (
                ok,
                format!("前 {} 条，解析到 {n} 条（total={}）", list.items.len(), list.total),
            )
        }
        Err(e) => (false, format!("请求失败：{e}")),
    }
}

/// 简易时间戳（秒级），仅用于日志行前缀。
fn now_str() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "?".to_string())
}

/// 把登录过程的诊断信息写到临时目录，便于排查"登录了但取不到数据"。
fn log_diag(line: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("jlu-oa-vpn-login.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{line}");
    }
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

        // 门户里的磁贴是外链，点击时 WebView2 默认会尝试开新窗口；Tauri 默认
        // 不处理该请求，表现为"点了没反应"。这里让当前窗口自己加载，
        // 用户点击磁贴就能真正进入校内站点（例如吉大 OA）。
        let nav_handle = handle.clone();
        let window = match tauri::WebviewWindowBuilder::new(
            &handle,
            LOGIN_WINDOW_LABEL,
            tauri::WebviewUrl::External(url),
        )
        .title("登录 VPN 后点进「吉大 OA」")
        .inner_size(920.0, 760.0)
        // 注意：不要用 .center()——它是桌面专属方法，Android 上不存在
        // （实测 Android 构建报 E0599: no method named `center`）。
        // 不设置自定义 user_agent：WebView2 在自定义 UA 下出现过不渲染
        // （黑屏/白屏）的已知问题，而这里本来也不需要伪装 UA。
        //
        // 诊断用：右键可「检查元素」，出现黑屏时能直接看到网络与控制台报错。
        // 该窗口只用于登录，保留 devtools 的风险可接受。
        .devtools(true)
        // 被 target="_blank" / window.open 打开的链接：交给当前窗口导航，
        // 而不是让 WebView2 弹一个我们看不到的新窗口
        .on_new_window(move |new_url, _features| {
            if let Some(w) = nav_handle.get_webview_window(LOGIN_WINDOW_LABEL) {
                let _ = w.navigate(new_url);
            }
            tauri::webview::NewWindowResponse::Deny
        })
        // 只把 window.open 收敛到当前窗口。
        //
        // ⚠️ 刻意**不拦截 <a> 的点击**。曾经在这里用捕获阶段监听 +
        // preventDefault 把 target="_blank" 链接改成 location.href，
        // 结果绕过了门户自己的点击处理（门户要靠它生成加密地址并记一次访问），
        // 表现为"点击无反应"。让门户的原生点击照常执行才可靠。
        .initialization_script(
            r#"
            (function () {
              try {
                window.open = function (u) { if (u) location.href = u; return null; };
              } catch (e) {}
            })();
            "#,
        )
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
        // 连续取不到 Cookie 的次数：用于尽早失败并回报诊断，
        // 而不是让用户干等 5 分钟超时
        let mut no_cookie_strikes: u32 = 0;
        // 自动进入 OA 的尝试次数上限：门户是 SPA，磁贴要等渲染/接口回来才出现，
        // 所以允许重试；但别无限注入脚本。
        let mut auto_click_tries: u32 = 0;

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
                        message: Some(format!(
                        "登录超时（5 分钟）。诊断：{}",
                        take_cookie_diag()
                    )),
                    },
                );
                return;
            }

            // 当前地址：登录成功后网关会跳转
            let current = window.url().ok().map(|u| u.to_string());

            // 自动进入 OA：登录后停在门户页时，找出"吉大 OA"磁贴的链接并直接导航，
            // 省掉用户手动点击那一步。
            //
            // 关键：这里只是**读取门户自己生成好的链接**，不复制门户的加密逻辑
            // （加密串按目标资源用 AES 生成，密钥按会话注入，见 portal.js）。
            // 因此拿到的地址天然正确，也不怕网关升级改算法。
            let at_portal = current
                .as_deref()
                .map(|u| u.contains("/https/") && !u.contains("/defaultroot"))
                .unwrap_or(false);
            if at_portal && auto_click_tries < AUTO_CLICK_MAX_TRIES {
                auto_click_tries += 1;
                let script = format!(
                    r#"
                    (function () {{
                      var cur = location.href;
                      if (cur.indexOf('/defaultroot') >= 0) return 'already';
                      var as = document.querySelectorAll('a[href]');
                      for (var i = 0; i < as.length; i++) {{
                        var h = as[i].getAttribute('href') || '';
                        if (h.indexOf('/defaultroot') < 0) continue;
                        if (h === cur) continue;
                        location.href = as[i].href;
                        return 'ok';
                      }}
                      return 'none:' + as.length;
                    }})();
                    "#
                );
                log_diag(&format!(
                    "[{}] 在门户页，尝试自动进入 OA（第 {auto_click_tries} 次）",
                    now_str()
                ));
                let _ = window.eval(script);
            }

            // 候选前缀：只认"地址里已出现 defaultroot"，即已经真的在 OA 上。
            //
            // 刻意不"从当前地址提取加密串去拼 OA 前缀"：门户页面自身也在
            // /https/<加密串>/ 之下，而复用门户的串去拼 oa 是错的（两者不同）。
            let mut candidates: Vec<String> = Vec::new();
            if let Some(u) = &current {
                if let Some(root) = vpn_root_of(u) {
                    log_diag(&format!("[{}] 已进入 OA：{u}", now_str()));
                    candidates.push(root);
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
                    no_cookie_strikes += 1;
                    set_cookie_diag(format!(
                        "前缀 {root} 未取到任何 Cookie（第 {no_cookie_strikes} 次）"
                    ));
                    // 已进入 OA 却连续 3 次拿不到会话 Cookie，说明该平台的
                    // cookies_for_url 不可用（Android 文档标注 Unsupported），
                    // 此时继续等待毫无意义，直接失败并回报诊断。
                    if no_cookie_strikes >= 3 {
                        let _ = window.destroy();
                        let _ = handle.emit(
                            "vpn-login-result",
                            VpnLoginResponse {
                                ok: false,
                                prefix: None,
                                cookies: None,
                                message: Some(format!(
                                    "无法读取登录会话：{}",
                                    take_cookie_diag()
                                )),
                            },
                        );
                        return;
                    }
                    continue;
                };

                // 只记录 Cookie 的“名字”，不落盘具体值（避免把会话写进日志）
                let cookie_names = cookies
                    .split(';')
                    .filter_map(|kv| kv.split('=').next())
                    .map(|s| s.trim())
                    .collect::<Vec<_>>()
                    .join(",");
                log_diag(&format!(
                    "[{}] 尝试前缀 {root}｜Cookie 名: {cookie_names}",
                    now_str()
                ));

                let (ok, detail) =
                    tauri::async_runtime::block_on(verify_session(&root, &cookies));
                log_diag(&format!("[{}] 验证结果 ok={ok}｜{detail}", now_str()));

                if ok {
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

    /// 加密串按目标资源生成：门户与 oa 的加密串**不同**（实测数据）。
    /// 这条测试锁住那个曾经导致 bug 的误判——绝不能把门户的加密串复用到 oa。
    #[test]
    fn secret_differs_per_target_resource() {
        let portal = "https://vpn.jlu.edu.cn/https/48714f71342f7a336d582f7e285737375ccd605402c6dbebb4864756174f/user/portal";
        let oa = "https://vpn.jlu.edu.cn/https/48714f71342f7a336d582f7e2857373750cd3d1004df80a0b5971c1b1a/defaultroot/PortalInformation!jldxList.action";
        let p = extract_secret(portal).unwrap();
        let o = extract_secret(oa).unwrap();
        assert_ne!(p, o, "门户与 oa 的加密串不应相同");
        // 两者有共同前缀，但尾部不同
        assert!(p.starts_with("48714f71342f7a336d582f7e28573737"));
        assert!(o.starts_with("48714f71342f7a336d582f7e28573737"));
    }

    /// 只有地址里真的出现 defaultroot 才认作"已进入 OA"。
    #[test]
    fn only_defaultroot_counts_as_oa() {
        // 门户页面不能当成 OA 前缀
        assert_eq!(
            vpn_root_of("https://vpn.jlu.edu.cn/https/abc/user/portal"),
            None
        );
        assert_eq!(vpn_root_of("https://vpn.jlu.edu.cn/login"), None);
        // 真正的 OA 地址可以
        assert_eq!(
            vpn_root_of("https://vpn.jlu.edu.cn/https/abc/defaultroot/index.jsp").unwrap(),
            "https://vpn.jlu.edu.cn/https/abc/defaultroot/"
        );
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
