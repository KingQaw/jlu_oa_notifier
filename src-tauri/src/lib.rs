use oa_core::{ListOptions, ListResult, NoticeDetail};

/// 拉取通知列表（组织筛选 / 关键词 / 日期范围 / 分页）。
#[tauri::command]
async fn fetch_list(opts: ListOptions) -> Result<ListResult, String> {
    oa_core::fetch_list(&opts).await
}

/// 拉取单条通知详情。
#[tauri::command]
async fn fetch_detail(id: String) -> Result<NoticeDetail, String> {
    oa_core::fetch_detail(&id).await
}

/// 按关键词模糊搜索组织名。
#[tauri::command]
async fn search_orgs(query: String) -> Result<Vec<String>, String> {
    oa_core::search_orgs(&query).await
}

/// 生成附件下载直链。
#[tauri::command]
async fn build_attachment_url(
    information_id: String,
    filename: String,
    name: String,
) -> Result<String, String> {
    oa_core::build_attachment_url(&information_id, &filename, &name).await
}

/// 抓取站内图片并返回 data URL（正文图片改由后端加载，规避 WebView 跨源/证书差异）。
#[tauri::command]
async fn fetch_image(url: String) -> Result<String, String> {
    oa_core::fetch_image(&url).await
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
            fetch_image
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
