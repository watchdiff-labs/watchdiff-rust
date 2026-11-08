use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashSet;

pub fn parse_sitemap_urls(xml: &str) -> HashSet<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut urls      = HashSet::new();
    let mut in_loc    = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name.rsplit(':').next().unwrap_or(&name) == "loc" {
                    in_loc = true;
                }
            }
            Ok(Event::Text(t)) => {
                if in_loc {
                    if let Ok(text) = t.unescape() {
                        urls.insert(text.into_owned());
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name.rsplit(':').next().unwrap_or(&name) == "loc" {
                    in_loc = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    urls
}

pub struct SitemapDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

pub fn diff_sitemaps(before: &HashSet<String>, after: &HashSet<String>) -> SitemapDiff {
    SitemapDiff {
        added: after.difference(before).cloned().collect(),
        removed: before.difference(after).cloned().collect(),
    }
}
