use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub name: String,
    pub version: String,
    pub reason: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    pub name: String,
    pub version: String,
    pub status: String,
    pub detail: Option<String>,
}

/// 把「还没走完」的条目落成终态。
///
/// 重装任务中途失败时，任务级状态会被置为 `error`，但条目级状态可能仍停在
/// `pending`（还没轮到）或 `installing`（正在装）。前端会**无条件渲染全部
/// 条目**，于是用户看到标题写「重装失败」、列表里却有包显示「安装中」或
/// 「等待」——两处说法互相矛盾，且报告落盘后也留着同样的矛盾。
///
/// 已完成的条目（`installed` / `failed` / `canceled`）一律不动：它们记录的是
/// 真实结果，覆盖掉就等于篡改证据。
pub fn settle_unfinished_items(items: &mut [ItemResult], reason: &str) {
    for item in items
        .iter_mut()
        .filter(|item| item.status == "pending" || item.status == "installing")
    {
        item.status = "failed".into();
        item.detail = Some(reason.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, status: &str, detail: Option<&str>) -> ItemResult {
        ItemResult {
            name: name.into(),
            version: "1.0".into(),
            status: status.into(),
            detail: detail.map(str::to_owned),
        }
    }

    /// 重装中途失败时，任务级会被置为 `error`，但前端**无条件渲染全部条目**。
    /// 若条目仍停在 `pending`/`installing`，界面会同时出现「重装失败」和
    /// 「安装中」，报告落盘后也留着同样的矛盾。
    #[test]
    fn settle_unfinished_items_closes_pending_and_installing_only() {
        let mut items = vec![
            item("done", "installed", None),
            item("running", "installing", None),
            item("waiting", "pending", None),
            item("broken", "failed", Some("原有原因")),
            item("stopped", "canceled", None),
        ];
        settle_unfinished_items(&mut items, "重装中断，该包未完成安装");

        let by_name = |name: &str| items.iter().find(|i| i.name == name).unwrap();

        // 未完成的两种状态被落成 failed，并带上统一原因。
        assert_eq!(by_name("running").status, "failed");
        assert_eq!(
            by_name("running").detail.as_deref(),
            Some("重装中断，该包未完成安装")
        );
        assert_eq!(by_name("waiting").status, "failed");
        assert_eq!(
            by_name("waiting").detail.as_deref(),
            Some("重装中断，该包未完成安装")
        );

        // 已完成的条目一律不动：覆盖它们等于篡改已记录的结果。
        assert_eq!(by_name("done").status, "installed");
        assert_eq!(by_name("done").detail, None);
        assert_eq!(by_name("broken").status, "failed");
        assert_eq!(
            by_name("broken").detail.as_deref(),
            Some("原有原因"),
            "已有失败原因不能被通用原因覆盖"
        );
        assert_eq!(by_name("stopped").status, "canceled");
        assert_eq!(by_name("stopped").detail, None);
    }

    /// 空列表与全部已完成都不能被改动，也不能 panic。
    #[test]
    fn settle_unfinished_items_is_a_noop_when_nothing_is_unfinished() {
        let mut empty: Vec<ItemResult> = Vec::new();
        settle_unfinished_items(&mut empty, "reason");
        assert!(empty.is_empty());

        let mut finished = vec![item("a", "installed", None), item("b", "failed", Some("x"))];
        settle_unfinished_items(&mut finished, "reason");
        assert_eq!(finished[0].status, "installed");
        assert_eq!(finished[1].detail.as_deref(), Some("x"));
    }
}
