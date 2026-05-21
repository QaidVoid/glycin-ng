//! Minimal `xi:include` preprocessor for SVG inputs.
//!
//! GTK's symbolic-icon recolor pipeline wraps each icon in an outer
//! SVG that pulls the original via XInclude:
//!
//! ```xml
//! <svg xmlns="http://www.w3.org/2000/svg"
//!      xmlns:xi="http://www.w3.org/2001/XInclude" ...>
//!   <style>...recolor css...</style>
//!   <g opacity="...">
//!     <xi:include href="data:text/xml;base64,PHN2Zy..."/>
//!   </g>
//! </svg>
//! ```
//!
//! `resvg`/`usvg` does not implement XInclude (it is an XML-layer
//! feature, not an SVG-layer one), so the `<xi:include/>` element is
//! left as an unknown empty node and the icon renders blank. Upstream
//! glycin avoids this by using `librsvg`, which inherits XInclude
//! support from libxml2.
//!
//! Here we walk the input bytes with `quick-xml`, and for every
//! `xi:include` with an `href="data:text/xml;base64,..."` URI we
//! replace the element with the decoded payload's `<svg>` subtree
//! (XML prolog stripped). usvg is fine with nested `<svg>` elements
//! and treats them as their own viewport, which is what we want.
//!
//! Non-XInclude inputs are returned as `None` so the caller can use
//! the original byte slice unchanged.

use base64::Engine;
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

/// Run the XInclude pass over `input`. Returns the rewritten bytes
/// if any `xi:include` element was expanded, `None` otherwise.
pub(super) fn expand(input: &[u8]) -> Option<Vec<u8>> {
    // Fast path: most SVGs do not use XInclude. A literal substring
    // probe is cheap compared to a full XML parse.
    if !contains_xinclude(input) {
        return None;
    }

    let mut reader = Reader::from_reader(input);
    reader.config_mut().expand_empty_elements = false;
    let mut out: Vec<u8> = Vec::with_capacity(input.len());
    let mut writer = Writer::new(&mut out);

    let mut expanded_any = false;

    loop {
        let event = match reader.read_event() {
            Ok(ev) => ev,
            Err(_) => return None,
        };
        match event {
            Event::Empty(ref e) if is_xinclude_start(e) => {
                if let Some(payload) = extract_data_uri_payload(e)
                    && let Some(body) = strip_svg_prolog(&payload)
                {
                    if writer.write_event(Event::Text(body)).is_err() {
                        return None;
                    }
                    expanded_any = true;
                    continue;
                }
                // Couldn't expand; emit the element unchanged so we
                // do not corrupt the document.
                if writer.write_event(Event::Empty(e.clone())).is_err() {
                    return None;
                }
            }
            Event::Start(ref e) if is_xinclude_start(e) => {
                // GTK emits self-closing `<xi:include/>`; the
                // non-empty form is unusual but we still try.
                if let Some(payload) = extract_data_uri_payload(e)
                    && let Some(body) = strip_svg_prolog(&payload)
                {
                    if writer.write_event(Event::Text(body)).is_err() {
                        return None;
                    }
                    // Swallow content until matching End event.
                    let mut depth = 1;
                    while depth > 0 {
                        match reader.read_event() {
                            Ok(Event::Start(ref inner)) if is_xinclude_start(inner) => depth += 1,
                            Ok(Event::End(ref inner)) if is_xinclude_end(inner) => depth -= 1,
                            Ok(Event::Eof) => return None,
                            Err(_) => return None,
                            _ => {}
                        }
                    }
                    expanded_any = true;
                    continue;
                }
                if writer.write_event(Event::Start(e.clone())).is_err() {
                    return None;
                }
            }
            Event::Eof => break,
            other => {
                if writer.write_event(other).is_err() {
                    return None;
                }
            }
        }
    }

    if expanded_any { Some(out) } else { None }
}

fn contains_xinclude(input: &[u8]) -> bool {
    memchr_subseq(input, b"xi:include")
}

fn memchr_subseq(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > hay.len() {
        return needle.is_empty();
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

fn is_xinclude_start(e: &BytesStart<'_>) -> bool {
    matches!(e.name().as_ref(), b"xi:include" | b"include")
}

fn is_xinclude_end(e: &BytesEnd<'_>) -> bool {
    matches!(e.name().as_ref(), b"xi:include" | b"include")
}

fn extract_data_uri_payload<'a>(e: &BytesStart<'a>) -> Option<Vec<u8>> {
    for attr in e.attributes().with_checks(false).flatten() {
        if attr.key.as_ref() != b"href" {
            continue;
        }
        // attr.value is a Cow<[u8]> with XML escapes already
        // resolved against the document's encoding. `decode` would
        // turn entity refs into UTF-8, but the href is a base64
        // data URI so it has no entities to decode in practice.
        let v = attr.value.as_ref();
        let payload = strip_prefix(v, b"data:text/xml;base64,")
            .or_else(|| strip_prefix(v, b"data:image/svg+xml;base64,"))?;
        return base64::engine::general_purpose::STANDARD
            .decode(payload)
            .ok();
    }
    None
}

fn strip_prefix<'a>(input: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    if input.starts_with(prefix) {
        Some(&input[prefix.len()..])
    } else {
        None
    }
}

/// Drop a leading `<?xml ...?>` declaration (and any whitespace
/// before it) from an XML payload, returning the remainder as a
/// `quick_xml` `BytesText`. We emit the content as a text event
/// containing the raw XML bytes so that the writer pastes the inner
/// `<svg>...</svg>` subtree verbatim into the output document; that
/// is what `usvg` expects for nested SVGs.
fn strip_svg_prolog(payload: &[u8]) -> Option<quick_xml::events::BytesText<'static>> {
    let trimmed = trim_leading_whitespace(payload);
    let after_prolog = if let Some(rest) = strip_prefix(trimmed, b"<?xml") {
        let end = rest.iter().position(|&b| b == b'>')?;
        trim_leading_whitespace(&rest[end + 1..])
    } else {
        trimmed
    };

    let s = std::str::from_utf8(after_prolog).ok()?.to_owned();
    Some(quick_xml::events::BytesText::from_escaped(s))
}

fn trim_leading_whitespace(b: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    &b[i..]
}

#[cfg(test)]
mod tests {
    use super::*;

    const GTK_RECOLOR_WRAPPER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<svg version="1.1" xmlns="http://www.w3.org/2000/svg" xmlns:xi="http://www.w3.org/2001/XInclude" width="16" height="16">
  <style>path { fill: #fff !important; }</style>
  <g><xi:include href="data:text/xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPjxwYXRoIGQ9Ik0wIDBoMTZ2MTZIMHoiIGZpbGw9IiMyZTM0MzYiLz48L3N2Zz4="/></g>
</svg>"#;

    #[test]
    fn passes_through_when_no_xinclude() {
        let svg =
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"4\" height=\"4\"/></svg>";
        assert!(expand(svg).is_none());
    }

    #[test]
    fn inlines_data_uri_xinclude() {
        let out = expand(GTK_RECOLOR_WRAPPER.as_bytes()).expect("expanded");
        let s = std::str::from_utf8(&out).unwrap();
        assert!(!s.contains("xi:include"), "xi:include should be gone: {s}");
        assert!(s.contains("<svg xmlns=\"http://www.w3.org/2000/svg\"><path"));
    }

    #[test]
    fn leaves_unrecognized_href_intact() {
        // External-URL href that we cannot resolve. We leave the
        // element as-is so the upstream caller's error path stays
        // visible rather than silently dropping content.
        let svg = br#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xi="http://www.w3.org/2001/XInclude">
  <xi:include href="https://example.com/icon.svg"/>
</svg>"#;
        let out = expand(svg);
        assert!(out.is_none(), "no expansion -> caller uses original input");
    }
}
