//! Related-document links extracted from a document's HTML page.
//!
//! The Rada site has no anonymous JSON for the «related» lists, so we
//! parse the `show_html` page: anchors pointing at `/laws/show/<nreg>`
//! (or the short `go/<nreg>` form) name other acts referenced by this one.
//! Pure string scanning — no regexes, no HTML parser, fully unit-tested.

/// One related-document link found in the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedDoc {
    /// System number of the referenced act (`2341-14`, `254к/96-вр`…).
    pub nreg: String,
    /// Anchor text (the link title as authored on the page), whitespace-collapsed.
    pub title: String,
}

/// True when `s` may be an nreg: single token, has a digit, nreg-safe chars,
/// contains a separator. Mirrors the db-side heuristic but stays dependency-free.
fn looks_like_nreg(s: &str) -> bool {
    if s.is_empty() || !s.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/')) {
        return false;
    }
    let len = s.chars().count();
    (4..=30).contains(&len) && s.chars().any(|c| c.is_ascii_digit()) && s.contains(['-', '_', '/'])
}

/// Percent-decode (`%D0%BF` → `п`). Invalid escapes are kept literally.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hi = (b[i + 1] as char).to_digit(16);
            let lo = (b[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Strip a trailing `/edYYYYMMDD` edition path and any `#...`/`?` remainder,
/// returning the bare nreg part of a `laws/show/…` href tail.
fn nreg_from_show_tail(tail: &str) -> Option<String> {
    let t = tail
        .split('#')
        .next()?
        .split('?')
        .next()?
        .trim_end_matches('/');
    // laws/show/<nreg>[/ed<digits>]
    let base = match t.split_once("/ed") {
        Some((base, rest)) if rest.chars().all(|c| c.is_ascii_digit()) && !rest.is_empty() => {
            base
        }
        _ => t,
    };
    let base = percent_decode(base);
    if base.is_empty() || !looks_like_nreg(&base) {
        return None;
    }
    Some(base)
}

/// Collapse whitespace of an anchor's inner text.
fn clean_title(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string()
}

/// Extract related-document links from `html`, skipping `self_nreg`.
/// Keeps first-seen order, de-duplicates by nreg, caps at 50 entries.
pub fn extract_related(html: &str, self_nreg: &str) -> Vec<RelatedDoc> {
    let mut out: Vec<RelatedDoc> = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0usize;
    while i < html.len() {
        // Find the next "href=" (case-insensitive), then the quoted URL.
        let Some(h) = find_ci(bytes, i, b"href=") else { break };
        let mut j = h + 5;
        // skip spaces then the quote
        while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        if j >= bytes.len() || (bytes[j] != b'"' && bytes[j] != b'\'') {
            i = h + 5;
            continue;
        }
        let quote = bytes[j];
        let start = j + 1;
        let Some(rel_end) = html[start..].find(quote as char) else { break };
        let url = &html[start..start + rel_end];
        // Anchor text follows the closing '>' of this <a> tag.
        let title = html
            .get(start + rel_end..)
            .and_then(|rest| {
                let gt = rest.find('>')?;
                let after = &rest[gt + 1..];
                let close = after.find("</a>")?;
                Some(clean_title(&strip_tags(&after[..close])))
            })
            .unwrap_or_default();

        let nreg = if let Some(tail) = url_ci(url, "laws/show/") {
            nreg_from_show_tail(tail)
        } else if let Some(tail) = url_ci(url, "go/") {
            // Short link form: go/<nreg> — take it up to # / ? / extra path.
            let t = tail
                .split('#')
                .next()
                .unwrap_or("")
                .split('?')
                .next()
                .unwrap_or("")
                .trim_end_matches('/');
            let t = percent_decode(t);
            looks_like_nreg(&t).then_some(t)
        } else {
            None
        };

        if let Some(nreg) = nreg {
            if !nreg.eq_ignore_ascii_case(self_nreg) && !out.iter().any(|r| r.nreg == nreg) {
                out.push(RelatedDoc { nreg, title });
                if out.len() >= 50 {
                    break;
                }
            }
        }
        i = start + rel_end;
    }
    out
}

/// Find `needle` (lowercased ASCII) in `hay` starting at `from`, case-insensitive.
fn find_ci(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || from > hay.len() {
        return None;
    }
    let n = needle.len();
    'outer: for i in from..=hay.len().checked_sub(n)? {
        for (k, &nb) in needle.iter().enumerate() {
            if !hay[i + k].eq_ignore_ascii_case(&nb) {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

/// Case-insensitive substring lookup returning the tail after the match.
fn url_ci<'a>(url: &'a str, needle: &str) -> Option<&'a str> {
    let b = url.as_bytes();
    find_ci(b, 0, needle.as_bytes()).map(|i| &url[i + needle.len()..])
}

/// Cheap tag stripper for anchor text (`<b>…</b>`, `<nobr>…` etc.).
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html><body>
    <div class="list">
      <a href="https://zakon.rada.gov.ua/laws/show/2341-14#Text" class="siz13">
         <nobr> Кримінальний   кодекс
         України </nobr></a>
      <a href="/laws/show/2755-17/ed20241115#n3">Про професійну освіту</a>
      <a href="/laws/show/1178-2022-%D0%BF">Закон про медіа</a>
      <a href="https://zakon.rada.gov.ua/go/980_107">Рішення КСУ</a>
      <a href="/laws/show/2341-14">дубль того самого</a>
      <a href="/laws/show/1556-18">сам документ</a>
      <a href="/laws/sohm">збірки (не документ)</a>
      <a href="https://example.com/x.html">зовнішнє посилання</a>
      <a name="x">якір без href</a>
      <a href="/laws/show/0000">без роздільника</a>
    </div>
    </body></html>"#;

    #[test]
    fn extracts_show_ed_and_go_links_skipping_self() {
        let rel = extract_related(PAGE, "1556-18");
        let nregs: Vec<&str> = rel.iter().map(|r| r.nreg.as_str()).collect();
        assert_eq!(nregs, ["2341-14", "2755-17", "1178-2022-п", "980_107"]);
    }

    #[test]
    fn extracts_titles_and_collapses_whitespace() {
        let rel = extract_related(PAGE, "1556-18");
        let first = rel.iter().find(|r| r.nreg == "2341-14").expect("2341-14");
        assert_eq!(first.title, "Кримінальний кодекс України");
    }

    #[test]
    fn skips_self_duplicates_nonlaws_and_caps() {
        let rel = extract_related(PAGE, "1556-18");
        let names: Vec<&str> = rel.iter().map(|r| r.nreg.as_str()).collect();
        // self (1556-18) skipped; duplicate 2341-14 skipped; non-nreg hrefs skipped
        assert!(!names.contains(&"1556-18"), "{names:?}");
        assert_eq!(names.iter().filter(|n| **n == "2341-14").count(), 1, "{names:?}");
        assert!(!names.contains(&"sohm"), "{names:?}");
        assert!(!names.contains(&"0000"), "{names:?}");
        // 50-entry cap
        let mut big = String::new();
        for i in 0..60 {
            big.push_str(&format!("<a href=\"/laws/show/{i}-20\">t{i}</a>"));
        }
        assert_eq!(extract_related(&big, "x").len(), 50);
    }

    #[test]
    fn empty_and_garbage_pages() {
        assert!(extract_related("", "1-1").is_empty());
        assert!(extract_related("<a href=>x</a><a href=\"laws/show/\">y</a>", "1").is_empty());
    }

    /// Live validation against a real document page (network, run with
    /// `cargo test -p rada-core -- --ignored`). The Criminal Code page links
    /// to other acts; we assert we recover at least a handful of nregs.
    #[test]
    #[ignore]
    fn live_related_on_criminal_code() {
        let c = crate::api::RadaClient::with_pacing(std::time::Duration::from_secs(5));
        let html = c.show_html("2341-14").expect("show_html");
        let rel = extract_related(&html, "2341-14");
        eprintln!("related count = {}", rel.len());
        for r in rel.iter().take(15) {
            eprintln!("  {} — {}", r.nreg, r.title);
        }
        assert!(rel.len() >= 3, "expected several related links, got {}", rel.len());
        assert!(!rel.iter().any(|r| r.nreg == "2341-14"), "self must be skipped");
    }
}

