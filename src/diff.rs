use crate::types::{Change, DiffMode};
use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;

pub fn diff(before: &str, after: &str, mode: DiffMode) -> Vec<Change> {
    match mode {
        DiffMode::Line => diff_line(before, after),
        DiffMode::Word => diff_word(before, after),
        DiffMode::Semantic => diff_semantic(before, after),
        DiffMode::Json => diff_json(before, after).unwrap_or_else(|_| diff_line(before, after)),
        DiffMode::Rss => diff_rss(before, after).unwrap_or_else(|| diff_line(before, after)),
    }
}

fn diff_line(before: &str, after: &str) -> Vec<Change> {
    let diff           = TextDiff::from_lines(before, after);
    let mut changes    = Vec::new();
    for group in diff.grouped_ops(0) {
        for op in group {
            for change in diff.iter_changes(&op) {
                match change.tag() {
                    ChangeTag::Delete => changes.push(Change {
                        kind: "removed".into(),
                        before: Some(change.to_string().trim_end().to_string()),
                        after: None,
                        context: None,
                    }),
                    ChangeTag::Insert => changes.push(Change {
                        kind: "added".into(),
                        before: None,
                        after: Some(change.to_string().trim_end().to_string()),
                        context: None,
                    }),
                    ChangeTag::Equal => {}
                }
            }
        }
    }
    changes
}

fn diff_word(before: &str, after: &str) -> Vec<Change> {
    let diff                                  = TextDiff::from_words(before, after);
    let mut changes                           = Vec::new();
    let mut pending_remove: Option<String>    = None;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => {
                pending_remove = Some(change.to_string());
            }
            ChangeTag::Insert => {
                if let Some(removed) = pending_remove.take() {
                    changes.push(Change {
                        kind: "modified".into(),
                        before: Some(removed.trim().to_string()),
                        after: Some(change.to_string().trim().to_string()),
                        context: None,
                    });
                } else {
                    changes.push(Change {
                        kind: "added".into(),
                        before: None,
                        after: Some(change.to_string().trim().to_string()),
                        context: None,
                    });
                }
            }
            ChangeTag::Equal => {
                if let Some(removed) = pending_remove.take() {
                    changes.push(Change {
                        kind: "removed".into(),
                        before: Some(removed.trim().to_string()),
                        after: None,
                        context: None,
                    });
                }
            }
        }
    }
    if let Some(removed) = pending_remove.take() {
        changes.push(Change {
            kind: "removed".into(),
            before: Some(removed.trim().to_string()),
            after: None,
            context: None,
        });
    }
    changes.retain(|c| {
        c.before.as_deref().unwrap_or("").chars().any(|ch| !ch.is_whitespace())
            || c.after.as_deref().unwrap_or("").chars().any(|ch| !ch.is_whitespace())
    });
    changes
}

fn diff_semantic(before: &str, after: &str) -> Vec<Change> {
    let before_blocks: Vec<&str>    = before.split('\n').map(str::trim).filter(|s| !s.is_empty()).collect();
    let after_blocks: Vec<&str>     = after.split('\n').map(str::trim).filter(|s| !s.is_empty()).collect();
    let diff                        = TextDiff::from_slices(&before_blocks, &after_blocks);
    let mut changes                 = Vec::new();
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => changes.push(Change {
                kind: "removed".into(),
                before: Some(change.value().to_string()),
                after: None,
                context: None,
            }),
            ChangeTag::Insert => changes.push(Change {
                kind: "added".into(),
                before: None,
                after: Some(change.value().to_string()),
                context: None,
            }),
            ChangeTag::Equal => {}
        }
    }
    changes
}

fn diff_json(before: &str, after: &str) -> anyhow::Result<Vec<Change>> {
    let b: serde_json::Value    = serde_json::from_str(before)?;
    let a: serde_json::Value    = serde_json::from_str(after)?;
    let mut changes             = Vec::new();
    walk_json("", &b, &a, &mut changes);
    Ok(changes)
}

fn walk_json(path: &str, before: &serde_json::Value, after: &serde_json::Value, out: &mut Vec<Change>) {
    use serde_json::Value;
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(bo), Value::Object(ao)) => {
            let mut keys: Vec<&String> = bo.keys().chain(ao.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let sub_path = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                match (bo.get(key), ao.get(key)) {
                    (Some(bv), Some(av)) => walk_json(&sub_path, bv, av, out),
                    (Some(bv), None) => out.push(Change {
                        kind: "removed".into(),
                        before: Some(bv.to_string()),
                        after: None,
                        context: Some(sub_path),
                    }),
                    (None, Some(av)) => out.push(Change {
                        kind: "added".into(),
                        before: None,
                        after: Some(av.to_string()),
                        context: Some(sub_path),
                    }),
                    (None, None) => {}
                }
            }
        }
        _ => out.push(Change {
            kind: "modified".into(),
            before: Some(scalar_str(before)),
            after: Some(scalar_str(after)),
            context: if path.is_empty() { None } else { Some(path.to_string()) },
        }),
    }
}

fn scalar_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone)]
struct FeedItem {
    id: String,
    title: String,
    link: String,
}

fn diff_rss(before: &str, after: &str) -> Option<Vec<Change>> {
    let before_items    = parse_feed(before)?;
    let after_items     = parse_feed(after)?;

    let before_map: HashMap<&str, &FeedItem>    = before_items.iter().map(|i| (i.id.as_str(), i)).collect();
    let after_map: HashMap<&str, &FeedItem>     = after_items.iter().map(|i| (i.id.as_str(), i)).collect();

    let mut changes = Vec::new();
    for item in &after_items {
        match before_map.get(item.id.as_str()) {
            None => changes.push(Change {
                kind: "added".into(),
                before: None,
                after: Some(item.title.clone()),
                context: Some(item.link.clone()),
            }),
            Some(old) if old.title != item.title => changes.push(Change {
                kind: "modified".into(),
                before: Some(old.title.clone()),
                after: Some(item.title.clone()),
                context: Some(item.link.clone()),
            }),
            _ => {}
        }
    }
    for item in &before_items {
        if !after_map.contains_key(item.id.as_str()) {
            changes.push(Change {
                kind: "removed".into(),
                before: Some(item.title.clone()),
                after: None,
                context: Some(item.link.clone()),
            });
        }
    }
    Some(changes)
}

fn parse_feed(xml: &str) -> Option<Vec<FeedItem>> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut items          = Vec::new();
    let mut in_item        = false;
    let mut current_tag    = String::new();
    let mut id             = String::new();
    let mut title          = String::new();
    let mut link           = String::new();
    let mut found_any      = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name     = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let local    = name.rsplit(':').next().unwrap_or(&name).to_string();
                if local == "item" || local == "entry" {
                    in_item      = true;
                    found_any    = true;
                    id.clear();
                    title.clear();
                    link.clear();
                }
                if in_item {
                    current_tag = local;
                    if current_tag == "link" {
                        if let Some(href) = e.attributes().filter_map(|a| a.ok()).find(|a| a.key.as_ref() == b"href") {
                            link = String::from_utf8_lossy(&href.value).into_owned();
                        }
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if in_item {
                    let text = t.unescape().unwrap_or_default().into_owned();
                    match current_tag.as_str() {
                        "guid" | "id" => id    = text,
                        "title" => title       = text,
                        "link" => {
                            if link.is_empty() {
                                link = text;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name     = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let local    = name.rsplit(':').next().unwrap_or(&name).to_string();
                if local == "item" || local == "entry" {
                    in_item         = false;
                    let final_id    = if id.is_empty() { link.clone() } else { id.clone() };
                    if !final_id.is_empty() {
                        items.push(FeedItem {
                            id: final_id,
                            title: title.clone(),
                            link: link.clone(),
                        });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return None,
            _ => {}
        }
    }

    if found_any {
        Some(items)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_diff_detects_add_and_remove() {
        let before     = "a\nb\nc";
        let after      = "a\nc\nd";
        let changes    = diff(before, after, DiffMode::Line);
        assert!(changes.iter().any(|c| c.before.as_deref() == Some("b")));
        assert!(changes.iter().any(|c| c.after.as_deref() == Some("d")));
    }

    #[test]
    fn word_diff_detects_modification() {
        let changes = diff("the price is 19.99", "the price is 24.99", DiffMode::Word);
        assert!(changes.iter().any(|c| c.kind == "modified"));
    }

    #[test]
    fn json_diff_reports_key_path() {
        let before     = r#"{"price": 19.99, "stock": {"available": true}}"#;
        let after      = r#"{"price": 24.99, "stock": {"available": false}}"#;
        let changes    = diff(before, after, DiffMode::Json);
        assert!(changes.iter().any(|c| c.context.as_deref() == Some("price")));
        assert!(changes.iter().any(|c| c.context.as_deref() == Some("stock.available")));
    }

    #[test]
    fn json_diff_no_changes_when_equal() {
        let json = r#"{"a": 1}"#;
        assert!(diff(json, json, DiffMode::Json).is_empty());
    }

    #[test]
    fn rss_diff_detects_added_and_removed_items() {
        let before = r#"<rss><channel>
            <item><guid>1</guid><title>Old</title><link>http://a</link></item>
        </channel></rss>"#;
        let after = r#"<rss><channel>
            <item><guid>2</guid><title>New</title><link>http://b</link></item>
        </channel></rss>"#;
        let changes = diff(before, after, DiffMode::Rss);
        assert!(changes.iter().any(|c| c.kind == "added" && c.after.as_deref() == Some("New")));
        assert!(changes.iter().any(|c| c.kind == "removed" && c.before.as_deref() == Some("Old")));
    }
}
