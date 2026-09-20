//! 在线抓取自检：对真实网站执行列表/详情/组织搜索/附件直链。
//!
//! 运行方式（需网络）：
//!   cargo run -p oa-core --example fetch_check
//!
//! 指定网页版 VPN 地址（校外无校园网时）：
//!   cargo run -p oa-core --example fetch_check -- <VPN地址> [会话票据]
//!
//! 其中 VPN 地址就是登录 VPN、进入校内 OA 后浏览器地址栏里的完整网址；
//! 会话 Cookie 串（形如 `a=1; b=2`，可省略，省略时只能访问匿名可用的资源）。
//! 正常使用应用时无需手动提供：内置登录窗口会自动抓取并验证。

use oa_core::{
    build_attachment_url, fetch_detail, fetch_image, fetch_list, search_orgs, Access, ListOptions,
};

/// 从命令行参数或环境变量解析访问模式。
///
/// 也支持环境变量 `OA_VPN_PREFIX` / `OA_VPN_TICKET`：VPN 地址里常含 `!`、`&` 等
/// 字符，经 Windows `cmd.exe` 传参需要繁琐转义，用环境变量最省事。
fn parse_access() -> Access {
    let mut args = std::env::args().skip(1);
    let prefix = args.next().or_else(|| std::env::var("OA_VPN_PREFIX").ok());
    match prefix {
        None => {
            println!("[访问模式] 直连 https://oa.jlu.edu.cn");
            Access::Direct
        }
        Some(prefix) => {
            let ticket = args.next().or_else(|| std::env::var("OA_VPN_TICKET").ok());
            println!(
                "[访问模式] 网页版 VPN，前缀={prefix}，票据={}",
                if ticket.is_some() { "已提供" } else { "未提供" }
            );
            Access::Vpn { prefix, cookies: ticket }
        }
    }
}

#[tokio::main]
async fn main() {
    // 自检：网关地址规范化（内置登录窗口依赖它拿到 <加密串>）
    if std::env::args().any(|a| a == "--probe") {
        println!("[探针] 规范化 https://oa.jlu.edu.cn/defaultroot/");
        match oa_core::probe_forwarded_url("https://oa.jlu.edu.cn/defaultroot/").await {
            Some(u) => println!("[探针] 得到: {u}"),
            None => println!("[探针] 未取到转发地址"),
        }
        return;
    }
    if std::env::args().any(|a| a == "--selftest") {
        println!("[自检] 已加载");
        return;
    }

    let access = parse_access();

    // 1) 组织筛选：科研院
    let opts = ListOptions {
        org: Some("科研院".to_string()),
        page: Some(1),
        access: Some(access.clone()),
        ..Default::default()
    };
    let list = match fetch_list(&opts).await {
        Ok(v) => v,
        Err(e) => {
            println!("[列表] 抓取失败: {e}");
            println!("提示：若为 {e} 或连接错误，请确认 VPN 地址/票据是否正确、是否已登录。");
            return;
        }
    };
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
    if let Some(first) = list_json["items"].get(0) {
        println!(
            "[JSON] NoticeItem keys = {:?}",
            first.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())
        );
    }

    // 2) 关键词搜索
    let kw = ListOptions {
        keyword: Some("研究生".to_string()),
        search_type: Some(0),
        date_range: Some("1".to_string()),
        access: Some(access.clone()),
        ..Default::default()
    };
    let k = fetch_list(&kw).await.expect("keyword list");
    println!("[关键词] 研究生(近一月) total={}", k.total);

    // 3) 详情 + 附件
    if let Some(first) = list.items.first() {
        let d = fetch_detail(&first.id, &access).await.expect("fetch_detail");
        println!(
            "[详情] {} | {} | {} | content_len={} attachments={}",
            d.title,
            d.org,
            d.time,
            d.content_html.len(),
            d.attachments.len()
        );
        if let Some(a) = d.attachments.first() {
            let url = build_attachment_url(&d.id, &a.filename, &a.name, &access)
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
    let orgs = search_orgs("党委", &access).await.expect("search_orgs");
    println!("[组织搜索] 党委 -> {:?}", orgs);

    // 5) 正文图片内联（应返回 data URL）；图片地址随访问模式变换前缀
    let img_url = match &access {
        Access::Direct => {
            "https://oa.jlu.edu.cn/defaultroot/upload/html/20260917093318309.png".to_string()
        }
        Access::Vpn { prefix, .. } => {
            // 把直连地址替换成 VPN 前缀下的等价地址
            let root = {
                let s = prefix.trim();
                match s.find("defaultroot") {
                    Some(i) => format!("{}/", &s[..i + "defaultroot".len()]),
                    None => format!("{}/defaultroot/", s.trim_end_matches('/')),
                }
            };
            format!("{root}upload/html/20260917093318309.png")
        }
    };
    match fetch_image(&img_url, &access).await {
        Ok(data) => println!(
            "[图片] 抓取成功 data URL 前缀 {:?} 总长 {}",
            &data[..data.len().min(48)],
            data.len()
        ),
        Err(e) => println!("[图片] 抓取失败: {e}"),
    }

    // 6) 安全校验：站外地址必须被拒绝
    match fetch_image("https://example.com/a.png", &access).await {
        Ok(_) => println!("[图片] 站外地址未被拒绝（不符合预期！）"),
        Err(e) => println!("[图片] 站外地址已拒绝: {e}"),
    }

    // 7) 安全校验：伪造的 VPN 前缀必须被拒绝
    let fake = Access::Vpn {
        prefix: "https://evil.example.com/https/oa.jlu.edu.cn/defaultroot/".to_string(),
        cookies: None,
    };
    match fetch_list(&ListOptions {
        access: Some(fake),
        ..Default::default()
    })
    .await
    {
        Ok(_) => println!("[安全] 伪造 VPN 前缀未被拒绝（不符合预期！）"),
        Err(e) => println!("[安全] 伪造 VPN 前缀已拒绝: {e}"),
    }
}
