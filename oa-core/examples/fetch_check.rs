//! 在线抓取自检：对真实网站执行列表/详情/组织搜索/附件直链。
//! 运行方式（需网络，用 zig 提供 C 编译器与链接器）：
//!   CC=.dev/bin/zigcc RUSTFLAGS="-C linker=$PWD/.dev/bin/zigcc" \
//!     cargo run -p oa-core --example fetch_check

use oa_core::{build_attachment_url, fetch_detail, fetch_list, search_orgs, ListOptions};

#[tokio::main]
async fn main() {
    // 1) 组织筛选：科研院
    let opts = ListOptions {
        org: Some("科研院".to_string()),
        page: Some(1),
        ..Default::default()
    };
    let list = fetch_list(&opts).await.expect("fetch_list");
    println!(
        "[组织筛选] org=科研院 total={} pages={} items={}",
        list.total,
        list.total_pages,
        list.items.len()
    );
    for it in list.items.iter().take(3) {
        println!("  - {} | {} | {}", it.org, it.time, it.title);
    }

    // 校验真正发往前端的 JSON 键名（前端按 camelCase 读取，必须为 totalPages/isNew）
    let list_json = serde_json::to_value(&list).unwrap();
    println!(
        "[JSON] ListResult keys = {:?}",
        list_json
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>())
    );
    println!(
        "[JSON] NoticeItem keys = {:?}",
        list_json["items"][0]
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>())
    );

    // 2) 关键词搜索
    let kw = ListOptions {
        keyword: Some("研究生".to_string()),
        search_type: Some(0),
        date_range: Some("1".to_string()),
        ..Default::default()
    };
    let k = fetch_list(&kw).await.expect("keyword list");
    println!("[关键词] 研究生(近一月) total={}", k.total);

    // 3) 详情 + 附件
    if let Some(first) = list.items.first() {
        let d = fetch_detail(&first.id).await.expect("fetch_detail");
        println!(
            "[详情] {} | {} | {} | content_len={} attachments={}",
            d.title,
            d.org,
            d.time,
            d.content_html.len(),
            d.attachments.len()
        );
        if let Some(a) = d.attachments.first() {
            let url = build_attachment_url(&d.id, &a.filename, &a.name)
                .await
                .expect("attachment url");
            println!("[附件] {} -> {}", a.name, url);
        }
        let detail_json = serde_json::to_value(&d).unwrap();
        println!(
            "[JSON] NoticeDetail keys = {:?}",
            detail_json
                .as_object()
                .map(|o| o.keys().cloned().collect::<Vec<_>>())
        );
    }

    // 4) 组织模糊搜索
    let orgs = search_orgs("党委").await.expect("search_orgs");
    println!("[组织搜索] 党委 -> {:?}", orgs);

    // 5) 正文图片内联（应返回 data URL）
    //    取自通知 70405568 正文中的图片
    let img_url = "https://oa.jlu.edu.cn/defaultroot/upload/html/20260917093318309.png";
    match oa_core::fetch_image(img_url).await {
        Ok(data) => println!(
            "[图片] 抓取成功 data URL 前缀 {:?} 总长 {}",
            &data[..data.len().min(48)],
            data.len()
        ),
        Err(e) => println!("[图片] 抓取失败: {e}"),
    }

    // 6) 安全校验：站外地址必须被拒绝
    match oa_core::fetch_image("https://example.com/a.png").await {
        Ok(_) => println!("[图片] 站外地址未被拒绝（不符合预期！）"),
        Err(e) => println!("[图片] 站外地址已拒绝: {e}"),
    }
}
