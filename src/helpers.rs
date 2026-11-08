use crate::types::{Change, DiffReport};

/// One-line human summary of a report, e.g. `"Book price (2 change(s))"`.
pub fn report_summary(report: &DiffReport) -> String {
    format!("{} ({} change(s))", report.label, report.changes.len())
}

/// Human-readable rendering of a single change, e.g.
/// `[~] Changed: "19.99" -> "24.99"` or `[+] Added: "New article"`.
pub fn change_human(change: &Change) -> String {
    match (change.before.as_deref(), change.after.as_deref()) {
        (None, Some(a)) => format!("[+] Added: \"{a}\""),
        (Some(b), None) => format!("[-] Removed: \"{b}\""),
        (Some(b), Some(a)) => format!("[~] Changed: \"{b}\" -> \"{a}\""),
        (None, None) => format!("[ ] {}", change.kind),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DiffMode;

    #[test]
    fn formats_added_and_modified() {
        let changes = crate::diff::diff("a\nb", "a\nc", DiffMode::Line);
        for c in &changes {
            let s = change_human(c);
            assert!(s.starts_with("[+]") || s.starts_with("[-]") || s.starts_with("[~]"));
        }
    }
}
