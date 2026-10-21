use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Msg(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("网络错误: {0}")]
    Net(#[from] reqwest::Error),
    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
}

/// 把 URL 里可能携带凭据的部分换成 `***`，供任何面向用户或日志的输出使用。
///
/// 两类位置会泄漏凭据：
/// 1. userinfo —— `https://user:token@host/path` 的前半段；
/// 2. 查询参数 —— `?token=...`、`?access_key=...` 这类把凭据塞进 query 的写法。
///
/// 当前内置源全是公开常量 URL，因此这是**防御性**的：一旦将来支持自定义
/// registry 或带凭据的镜像地址，错误信息不会成为第二条泄漏路径。宁可现在
/// 就把出口收窄，也不要等到有凭据时才回头补。
pub fn redact_url(url: &str) -> String {
    // 先切出 scheme:// 与其余部分；没有 scheme 的字符串按原样返回，
    // 因为无法可靠判断哪里是 host。
    let Some((scheme, rest)) = url.split_once("://") else {
        return redact_query(url);
    };
    // authority 到第一个 / ? # 为止。
    let end = rest
        .find(['/', '?', '#'])
        .unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let authority = match authority.rsplit_once('@') {
        // 只保留 host 部分，userinfo 整体替换。用 rsplit_once 是因为
        // 密码里允许出现 '@'，最后一个才是分隔符。
        Some((_, host)) => format!("***@{host}"),
        None => authority.to_string(),
    };
    format!("{scheme}://{authority}{}", redact_query(tail))
}

/// 把查询串里的敏感参数值替换掉，保留键名以便用户知道是哪个参数。
fn redact_query(text: &str) -> String {
    let Some((head, query)) = text.split_once('?') else {
        return text.to_string();
    };
    let (query, fragment) = match query.split_once('#') {
        Some((q, f)) => (q, Some(f)),
        None => (query, None),
    };
    let redacted: Vec<String> = query
        .split('&')
        .map(|pair| {
            let (key, value) = match pair.split_once('=') {
                Some((k, v)) => (k, Some(v)),
                None => (pair, None),
            };
            let sensitive = {
                let k = key.to_ascii_lowercase();
                ["token", "key", "secret", "password", "passwd", "auth", "sig", "signature", "credential"]
                    .iter()
                    .any(|needle| k.contains(needle))
            };
            match (sensitive, value) {
                (true, Some(_)) => format!("{key}=***"),
                _ => pair.to_string(),
            }
        })
        .collect();
    let mut out = format!("{head}?{}", redacted.join("&"));
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

impl Serialize for AppError {
    fn serialize<S>(&self, s: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        s.serialize_str(&self.to_string())
    }
}

impl AppError {
    pub fn msg(m: impl Into<String>) -> Self {
        AppError::Msg(m.into())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::redact_url;

    #[test]
    fn redact_url_removes_userinfo_and_sensitive_query_values() {
        // 公开 URL 原样通过：脱敏不能改变正常错误信息的可读性。
        assert_eq!(
            redact_url("https://nodejs.org/dist/v26.12.0/node.zip"),
            "https://nodejs.org/dist/v26.12.0/node.zip"
        );

        // userinfo 整体替换，host 与路径保留——用户仍能看出是哪个源失败。
        assert_eq!(
            redact_url("https://user:tok3n@example.test/tool.zip"),
            "https://***@example.test/tool.zip"
        );
        // 只有用户名也算凭据。
        assert_eq!(
            redact_url("https://user@example.test/tool.zip"),
            "https://***@example.test/tool.zip"
        );
        // 密码里含 '@' 时，最后一个才是分隔符。
        assert_eq!(
            redact_url("https://user:p@ss@example.test/a.zip"),
            "https://***@example.test/a.zip"
        );

        // 敏感查询参数只替换值，保留键名。
        assert_eq!(
            redact_url("https://example.test/a.zip?token=abc123&v=1"),
            "https://example.test/a.zip?token=***&v=1"
        );
        // 键名匹配大小写不敏感，且覆盖常见变体。
        for key in ["access_key", "secret", "password", "auth", "signature", "credential"] {
            let url = format!("https://example.test/a.zip?{key}=leaked");
            assert!(
                redact_url(&url).ends_with(&format!("{key}=***")),
                "{key} must be redacted: {}",
                redact_url(&url)
            );
        }

        // 无值参数与无关参数不动。
        assert_eq!(
            redact_url("https://example.test/a.zip?flag&page=2"),
            "https://example.test/a.zip?flag&page=2"
        );

        // fragment 保留在末尾。
        assert_eq!(
            redact_url("https://example.test/a.zip?token=x#frag"),
            "https://example.test/a.zip?token=***#frag"
        );

        // 没有 scheme 的字符串不会 panic，也不会被误当作 host 处理。
        assert_eq!(redact_url("not a url"), "not a url");
        assert_eq!(redact_url(""), "");
        // 只有 scheme 与 host。
        assert_eq!(redact_url("https://example.test"), "https://example.test");
    }
}
