use serde::{Deserialize, Serialize};

// 注意：以下所有结构体序列化后都会发给前端，前端 TypeScript 类型使用 camelCase。
// 因此必须加 `rename_all = "camelCase"`，否则 total_pages/content_html/is_new
// 等字段在前端会变成 undefined。

/// 单条通知（列表项）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeItem {
    pub id: String,
    pub title: String,
    pub org: String,
    pub time: String,
    pub pinned: bool,
    pub is_new: bool,
}

/// 列表查询结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResult {
    pub items: Vec<NoticeItem>,
    pub total: i64,
    pub page: u32,
    pub total_pages: i64,
}

/// 附件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub filename: String,
    pub name: String,
}

/// 通知详情。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeDetail {
    pub id: String,
    pub title: String,
    pub org: String,
    pub time: String,
    pub content_html: String,
    pub attachments: Vec<Attachment>,
}

/// 访问模式：直连校内 OA，或经网页版 VPN（WebVPN 网关）转发。
///
/// 网页 VPN 的目标站点被编码成一段加密前缀，本程序无法推导，因此 `prefix`
/// 直接采用用户在浏览器地址栏里看到的权威值（归一化到 `defaultroot/` 结尾），
/// 后续所有请求都由它拼接相对路径得到。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum Access {
    /// 直接访问 https://oa.jlu.edu.cn（校内网或已连 VPN 客户端）
    Direct,
    /// 经 https://vpn.jlu.edu.cn 的 WebVPN 转发
    Vpn {
        /// 站点前缀，例如
        /// `https://vpn.jlu.edu.cn/https/<加密串>/defaultroot/`
        prefix: String,
        /// 网页 VPN 的会话票据（Cookie `wengine_vpn_ticketvpn_jlu_edu_cn` 的值）。
        /// 该 Cookie 是 HttpOnly，WebView 的 JS 读不到，需用户在浏览器登录后复制；
        /// 为空时仅能访问匿名可用的资源。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ticket: Option<String>,
    },
}

impl Default for Access {
    fn default() -> Self {
        Access::Direct
    }
}

impl Access {
    /// 取出会话票据（仅 VPN 模式有）。
    pub fn ticket(&self) -> Option<&str> {
        match self {
            Access::Direct => None,
            Access::Vpn { ticket, .. } => ticket.as_deref(),
        }
    }

    /// 是否为 VPN 模式。
    pub fn is_vpn(&self) -> bool {
        matches!(self, Access::Vpn { .. })
    }
}

/// 列表查询参数。search_type: 0=标题 1=组织 2=内容；date_range: ""|"1"|"6"|"12"。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListOptions {
    pub org: Option<String>,
    pub page: Option<u32>,
    pub keyword: Option<String>,
    pub search_type: Option<u32>,
    pub date_range: Option<String>,
    /// 访问模式；不传按直连处理，保持旧调用方兼容。
    #[serde(default)]
    pub access: Option<Access>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 前端按 camelCase 读取字段，这里锁定序列化后的键名，防止再次回归。
    #[test]
    fn list_result_uses_camel_case_keys() {
        let result = ListResult {
            items: vec![NoticeItem {
                id: "1".into(),
                title: "标题".into(),
                org: "科研院".into(),
                time: "昨天 14:33".into(),
                pinned: true,
                is_new: true,
            }],
            total: 1,
            page: 1,
            total_pages: 2,
        };

        let json = serde_json::to_value(&result).unwrap();
        assert!(json.get("totalPages").is_some(), "缺少 totalPages: {json}");
        assert!(json.get("total_pages").is_none(), "不应存在 total_pages");
        assert!(json["items"][0].get("isNew").is_some(), "缺少 isNew: {json}");
    }

    #[test]
    fn notice_detail_uses_camel_case_keys() {
        let detail = NoticeDetail {
            id: "1".into(),
            title: "标题".into(),
            org: "科研院".into(),
            time: "2026年09月18日 14:33".into(),
            content_html: "<p>正文</p>".into(),
            attachments: vec![Attachment {
                filename: "a.pdf".into(),
                name: "附件.pdf".into(),
            }],
        };

        let json = serde_json::to_value(&detail).unwrap();
        assert!(json.get("contentHtml").is_some(), "缺少 contentHtml: {json}");
        assert!(json.get("content_html").is_none(), "不应存在 content_html");
        assert_eq!(json["attachments"][0]["filename"], "a.pdf");
    }
}
