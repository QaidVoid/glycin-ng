//! Drop no-op empty `<g>` elements before handing a document to
//! `usvg`.
//!
//! resvg mis-computes the bounds of an isolated layer when a nested
//! `<svg>` inside a group with `opacity < 1` contains a childless,
//! transformed `<g>`: most of the content is then clipped away. The
//! shape that triggers it is exactly what GTK produces for symbolic
//! icons under a theme whose foreground colour has alpha, because
//! the recolor wrapper adds `<g opacity="...">` around the icon and
//! Inkscape-authored icons carry one empty `<g>` per unused layer:
//!
//! ```xml
//! <g opacity="0.75">
//!   <svg width="16" height="16">
//!     <g style="display:inline" transform="translate(-41,-447)"/>  <!-- empty -->
//!     <g style="display:inline" transform="translate(-41,-447)">...</g>
//!   </svg>
//! </g>
//! ```
//!
//! An empty group cannot paint anything, so removing it is
//! behaviour-preserving, with two exceptions this pass respects: a
//! group carrying a `filter` may still paint (`feFlood` and friends
//! need no input), and a group referenced elsewhere (`use`, `mask`,
//! `clip-path`) must stay reachable by id.
//!
//! Documents without the triggering shape are returned as `None` so
//! the caller keeps the original bytes.

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

/// Remove childless `<g>` elements, or `None` when the document
/// cannot hit the resvg bug or has nothing to remove.
pub(super) fn strip(input: &[u8]) -> Option<Vec<u8>> {
    if !worth_scanning(input) {
        return None;
    }

    let mut reader = Reader::from_reader(input);
    reader.config_mut().expand_empty_elements = false;
    let mut out: Vec<u8> = Vec::with_capacity(input.len());
    let mut writer = Writer::new(&mut out);
    let mut removed_any = false;
    // `<g>` elements opened but not yet written out, waiting to see
    // whether they turn out to be empty.
    let mut pending: Vec<BytesStart<'static>> = Vec::new();

    loop {
        let event = match reader.read_event() {
            Ok(ev) => ev,
            Err(_) => return None,
        };
        match event {
            Event::Empty(ref e) if is_group(e.name().as_ref()) => {
                if removable(e, input) {
                    removed_any = true;
                } else {
                    flush(&mut pending, &mut writer)?;
                    writer.write_event(Event::Empty(e.clone())).ok()?;
                }
            }
            Event::Start(ref e) if is_group(e.name().as_ref()) => {
                if removable(e, input) {
                    pending.push(e.to_owned());
                } else {
                    flush(&mut pending, &mut writer)?;
                    writer.write_event(Event::Start(e.clone())).ok()?;
                }
            }
            Event::End(ref e) if is_group(e.name().as_ref()) => {
                // A pending start with no content in between means the
                // group was empty after all, so drop the pair.
                if pending.pop().is_some() {
                    removed_any = true;
                } else {
                    writer.write_event(Event::End(e.clone())).ok()?;
                }
            }
            Event::Text(ref t) if t.iter().all(|b| b.is_ascii_whitespace()) => {
                // Whitespace alone does not make a group non-empty;
                // hold it so it is dropped along with the group.
                if pending.is_empty() {
                    writer.write_event(Event::Text(t.clone())).ok()?;
                }
            }
            Event::Eof => break,
            other => {
                flush(&mut pending, &mut writer)?;
                writer.write_event(other).ok()?;
            }
        }
    }

    removed_any.then_some(out)
}

/// Write out any groups that turned out to have content.
fn flush(pending: &mut Vec<BytesStart<'static>>, writer: &mut Writer<&mut Vec<u8>>) -> Option<()> {
    for start in pending.drain(..) {
        writer.write_event(Event::Start(start)).ok()?;
    }
    Some(())
}

fn is_group(name: &[u8]) -> bool {
    name == b"g" || name.ends_with(b":g")
}

/// A group is removable when nothing can paint through it and no one
/// can reference it.
fn removable(e: &BytesStart<'_>, document: &[u8]) -> bool {
    for attr in e.attributes().with_checks(false).flatten() {
        match attr.key.as_ref() {
            b"filter" => return false,
            b"id" => {
                if is_referenced(&attr.value, document) {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

/// Whether `#id` appears anywhere in the document, which would mean
/// a `use`, `mask`, `clip-path` or similar points at this group.
fn is_referenced(id: &[u8], document: &[u8]) -> bool {
    if id.is_empty() {
        return false;
    }
    let mut needle = Vec::with_capacity(id.len() + 1);
    needle.push(b'#');
    needle.extend_from_slice(id);
    document
        .windows(needle.len())
        .any(|w| w == needle.as_slice())
}

/// Cheap probe for the only shape that can hit the resvg bug: a
/// nested `<svg>` (more than one `<svg` in the document) together
/// with an opacity somewhere above it.
fn worth_scanning(input: &[u8]) -> bool {
    let mut svg_tags = 0;
    let mut i = 0;
    while let Some(pos) = find(&input[i..], b"<svg") {
        svg_tags += 1;
        if svg_tags > 1 {
            break;
        }
        i += pos + 4;
    }
    svg_tags > 1 && find(input, b"opacity").is_some()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = "xmlns=\"http://www.w3.org/2000/svg\"";

    fn doc(body: &str) -> String {
        format!("<svg {NS} width=\"16\" height=\"16\">{body}</svg>")
    }

    #[test]
    fn ignores_documents_without_nested_svg() {
        let d = doc("<g opacity=\"0.5\"/><rect width=\"4\" height=\"4\"/>");
        assert!(strip(d.as_bytes()).is_none());
    }

    #[test]
    fn ignores_documents_without_opacity() {
        let d = doc(&format!(
            "<svg {NS}><g/><rect width=\"4\" height=\"4\"/></svg>"
        ));
        assert!(strip(d.as_bytes()).is_none());
    }

    #[test]
    fn removes_self_closing_and_paired_empty_groups() {
        let d = doc(&format!(
            "<g opacity=\"0.75\"><svg {NS}><g transform=\"translate(-41,-447)\"/><g></g>\
             <g><rect width=\"4\" height=\"4\"/></g></svg></g>"
        ));
        let out = strip(d.as_bytes()).expect("should rewrite");
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("<rect"), "content must survive: {s}");
        assert!(!s.contains("translate(-41,-447)"), "empty group kept: {s}");
        // One group remains: the one wrapping the rect, plus the
        // opacity group.
        assert_eq!(s.matches("<g").count(), 2, "{s}");
    }

    #[test]
    fn keeps_group_with_filter() {
        let d = doc(&format!(
            "<g opacity=\"0.5\"><svg {NS}><g filter=\"url(#f)\"/>\
             <g><rect width=\"4\" height=\"4\"/></g></svg></g>"
        ));
        let out = strip(d.as_bytes());
        let s = out
            .map(|v| String::from_utf8(v).unwrap())
            .unwrap_or(d.clone());
        assert!(
            s.contains("filter=\"url(#f)\""),
            "filtered group dropped: {s}"
        );
    }

    #[test]
    fn keeps_referenced_group() {
        let d = doc(&format!(
            "<g opacity=\"0.5\"><svg {NS}><g id=\"layer1\"/><use href=\"#layer1\"/>\
             <g><rect width=\"4\" height=\"4\"/></g></svg></g>"
        ));
        let out = strip(d.as_bytes());
        let s = out
            .map(|v| String::from_utf8(v).unwrap())
            .unwrap_or(d.clone());
        assert!(s.contains("id=\"layer1\""), "referenced group dropped: {s}");
    }

    #[test]
    fn keeps_group_holding_only_whitespace_content() {
        // Whitespace-only groups are still empty and may go.
        let d = doc(&format!(
            "<g opacity=\"0.5\"><svg {NS}><g>\n  </g><g><rect width=\"4\" height=\"4\"/></g></svg></g>"
        ));
        let out = strip(d.as_bytes()).expect("should rewrite");
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("<rect"), "{s}");
    }
}
