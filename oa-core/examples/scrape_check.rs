//! 解析逻辑离线自检：读取本地 HTML 文件并打印解析结果。
//! 运行方式（无需网络、无需 C 编译器）：
//!   cargo run -p oa-core --example scrape_check --no-default-features -- <列表HTML> <详情HTML>

use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("用法: scrape_check <列表HTML> <详情HTML>");
        std::process::exit(1);
    }

    let list_html = fs::read_to_string(&args[1]).expect("读取列表 HTML 失败");
    let list = oa_core::parse_list(&list_html, 1);
    println!("=== LIST ===");
    println!(
        "total={} totalPages={} items={}",
        list.total,
        list.total_pages,
        list.items.len()
    );
    for it in list.items.iter().take(6) {
        println!(
            "- id={} pinned={} new={} | {} | {} | {}",
            it.id, it.pinned, it.is_new, it.org, it.time, it.title
        );
    }

    let detail_html = fs::read_to_string(&args[2]).expect("读取详情 HTML 失败");
    let d = oa_core::parse_detail(&detail_html, "70453835");
    println!("=== DETAIL ===");
    println!("title={}", d.title);
    println!("org={} time={}", d.org, d.time);
    println!("content_html_len={}", d.content_html.len());
    let head = &d.content_html[..d.content_html.len().min(80)];
    let tail = &d.content_html[d.content_html.len().saturating_sub(40)..];
    println!("content_html_head={:?}", head);
    println!("content_html_tail={:?}", tail);
    println!("attachments={}", d.attachments.len());
    for a in &d.attachments {
        println!("  - name={} filename={}", a.name, a.filename);
    }

    let orgs = oa_core::parse_orgs(&list_html);
    println!("=== ORGS (from list, {}) ===", orgs.len());
    for o in orgs {
        println!("  {}", o);
    }
}
