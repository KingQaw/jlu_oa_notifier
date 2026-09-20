use oa_core::{Access, ListOptions, ListResult, NoticeDetail};

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

/// 在系统浏览器中打开网页版 VPN，供用户登录并复制会话票据。
#[tauri::command]
async fn open_vpn_login(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    // 只允许打开吉大 VPN 域名，避免被当作任意 URL 打开器
    if !url.starts_with("https://vpn.jlu.edu.cn") && !url.starts_with("https://webvpn.jlu.edu.cn")
    {
        return Err("仅支持打开吉大 VPN 地址".to_string());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
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
            open_vpn_login
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
