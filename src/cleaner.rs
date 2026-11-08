use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use std::sync::Mutex;

/// libxml2 (used for XPath targets) is not guaranteed thread-safe for
/// concurrent parsing/evaluation across OS threads — the `libxml` crate's own
/// docs call this out. `WatchDiff` runs many watchers concurrently on a
/// multi-threaded tokio runtime, so every XPath evaluation is serialized
/// behind this process-wide lock. CSS-selector extraction (`scraper`, pure
/// Rust) is unaffected and stays fully concurrent.
static XPATH_LOCK: Mutex<()> = Mutex::new(());

/// Strips script/style tags and extracts the target CSS selector or XPath
/// expression (or full body text if none given). A target starting with `/`
/// or `./` is treated as XPath and evaluated via libxml2's HTML parser
/// (tolerant of real-world malformed markup); anything else is a CSS
/// selector evaluated via `scraper`.
pub fn extract(html: &str, target: Option<&str>, ignore_selectors: &[String]) -> anyhow::Result<String> {
    if let Some(t) = target {
        if t.starts_with('/') || t.starts_with("./") {
            return extract_xpath(html, t);
        }
    }

    let document = Html::parse_document(html);

    let ignore_sels: Vec<Selector> = ignore_selectors
        .iter()
        .filter_map(|s| Selector::parse(s).ok())
        .collect();

    let root_el = if let Some(sel_str) = target {
        let selector = Selector::parse(sel_str)
            .map_err(|e| anyhow::anyhow!("invalid CSS selector '{sel_str}': {e:?}"))?;
        document.select(&selector).next()
    } else {
        Some(document.root_element())
    };

    let text = match root_el {
        Some(el) => collect_text(el, &ignore_sels),
        None => String::new(),
    };

    Ok(normalize_whitespace(&text))
}

fn extract_xpath(html: &str, xpath: &str) -> anyhow::Result<String> {
    use libxml::parser::Parser;
    use libxml::xpath::Context;

    let _guard = XPATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let parser = Parser::default_html();
    let doc = parser
        .parse_string(html)
        .map_err(|e| anyhow::anyhow!("failed to parse HTML for XPath evaluation: {e:?}"))?;

    let mut context = Context::new(&doc)
        .map_err(|_| anyhow::anyhow!("failed to create an XPath context for the document"))?;

    let nodes = context
        .findnodes(xpath, None)
        .map_err(|_| anyhow::anyhow!("invalid XPath expression '{xpath}'"))?;

    let text = nodes
        .iter()
        .map(|n| n.get_content())
        .collect::<Vec<_>>()
        .join(" ");

    Ok(normalize_whitespace(&text))
}

const STRIP_TAGS: &[&str] = &["script", "style", "noscript"];

fn is_ignored(el: &ElementRef, ignore_sels: &[Selector]) -> bool {
    ignore_sels.iter().any(|sel| sel.matches(el))
}

fn collect_text(root: ElementRef, ignore_sels: &[Selector]) -> String {
    let mut parts = Vec::new();
    walk(root, ignore_sels, &mut parts);
    parts.join(" ")
}

fn walk(el: ElementRef, ignore_sels: &[Selector], out: &mut Vec<String>) {
    let tag = el.value().name();
    if STRIP_TAGS.contains(&tag) || is_ignored(&el, ignore_sels) {
        return;
    }
    for child in el.children() {
        match child.value() {
            Node::Text(text) => {
                let t = text.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
            }
            Node::Element(_) => {
                if let Some(child_el) = ElementRef::wrap(child) {
                    walk(child_el, ignore_sels, out);
                }
            }
            _ => {}
        }
    }
}

fn normalize_whitespace(s: &str) -> String {
    let re = Regex::new(r"\s+").unwrap();
    re.replace_all(s.trim(), " ").into_owned()
}

/// Applies ignorePatterns (regex) and ignoreNumbers to text before diffing.
pub fn apply_ignores(text: &str, ignore_patterns: &[String], ignore_numbers: bool) -> String {
    let mut out = text.to_string();
    for pat in ignore_patterns {
        if let Ok(re) = Regex::new(pat) {
            out = re.replace_all(&out, "").into_owned();
        }
    }
    if ignore_numbers {
        let re    = Regex::new(r"\b\d+([.,]\d+)?\b").unwrap();
        out       = re.replace_all(&out, "").into_owned();
    }
    normalize_whitespace(&out)
}

/// Extracts a JSON sub-value using a simple `$.a.b[0].c` path expression.
pub fn json_path_extract(json_text: &str, path: &str) -> anyhow::Result<String> {
    let value: serde_json::Value    = serde_json::from_str(json_text)?;
    let mut current                 = &value;
    let path                        = path.trim_start_matches('$').trim_start_matches('.');
    if path.is_empty() {
        return Ok(serde_json::to_string_pretty(current)?);
    }
    for segment in path.split('.') {
        let (key, index) = split_index(segment);
        if let Some(k) = key {
            current = current
                .get(k)
                .ok_or_else(|| anyhow::anyhow!("jsonPath key '{k}' not found"))?;
        }
        if let Some(i) = index {
            current = current
                .get(i)
                .ok_or_else(|| anyhow::anyhow!("jsonPath index '{i}' out of range"))?;
        }
    }
    Ok(serde_json::to_string_pretty(current)?)
}

fn split_index(segment: &str) -> (Option<&str>, Option<usize>) {
    if let Some(open) = segment.find('[') {
        let key        = &segment[..open];
        let idx_str    = segment[open + 1..].trim_end_matches(']');
        let idx        = idx_str.parse::<usize>().ok();
        (if key.is_empty() { None } else { Some(key) }, idx)
    } else {
        (Some(segment), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_script_and_style() {
        let html    = "<html><body><script>evil()</script><style>.x{}</style><p>Hello</p></body></html>";
        let text    = extract(html, None, &[]).unwrap();
        assert_eq!(text, "Hello");
    }

    #[test]
    fn extracts_css_selector() {
        let html    = "<html><body><div class='price'>$19.99</div><div>other</div></body></html>";
        let text    = extract(html, Some(".price"), &[]).unwrap();
        assert_eq!(text, "$19.99");
    }

    #[test]
    fn extracts_xpath_on_well_formed_markup() {
        let html = "<html><body><div class='price'>$19.99</div><div>other</div></body></html>";
        let text = extract(html, Some("//div[@class='price']"), &[]).unwrap();
        assert_eq!(text, "$19.99");
    }

    #[test]
    fn extracts_xpath_on_malformed_real_world_html() {
        // Unclosed <br>, unquoted attribute, no explicit <html>/<body> — all
        // invalid XML, all routine on real web pages. libxml2's HTML mode
        // must tolerate this; a strict XML/XPath parser would reject it.
        let html = "<div class=price>$19.99<br><p>In stock";
        let text = extract(html, Some("//div[@class='price']"), &[]).unwrap();
        // libxml2's get_content() concatenates descendant text verbatim (no
        // inserted whitespace between elements, unlike the CSS/scraper path).
        assert_eq!(text, "$19.99In stock");
    }

    #[test]
    fn invalid_xpath_expression_errors_clearly() {
        let html = "<html><body></body></html>";
        let err = extract(html, Some("//div[unclosed"), &[]).unwrap_err();
        assert!(err.to_string().contains("XPath"));
    }

    #[test]
    fn ignore_numbers_strips_digits() {
        let out = apply_ignores("1,234 views updated at 14:52", &[], true);
        assert!(!out.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn json_path_extracts_nested_value() {
        let json     = r#"{"data": {"price": 19.99}}"#;
        let value    = json_path_extract(json, "$.data.price").unwrap();
        assert_eq!(value, "19.99");
    }
}
