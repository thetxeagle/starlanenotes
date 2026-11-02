use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

/// CSS is retained source, not template guidance or Markdown whitespace.
/// Ambiguous boundaries conservatively protect the complete submitted source.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CssSourceSpan {
    Absent,
    Exact(std::ops::Range<usize>),
    Ambiguous,
}

pub(crate) fn css_source_span(content: &str) -> CssSourceSpan {
    // ASCII folding retains byte offsets for arbitrary UTF-8 source.
    let folded = content.to_ascii_lowercase();
    let mut candidates = 0;
    let mut markers = Vec::new();
    let whitespace = |c: char| c.is_whitespace() || c == '\u{feff}';
    for (start, _) in folded.match_indices("<!--") {
        let mut inner = folded[start + 4..].trim_start_matches(whitespace);
        let closing = inner.starts_with('/');
        if closing { inner = inner[1..].trim_start_matches(whitespace); }
        if !inner.starts_with("starlane:custom") { continue; }
        candidates += 1;
        if let Some(tail) = inner.strip_prefix("starlane:custom-css") {
            let tail = tail.trim_start_matches(whitespace);
            if tail.starts_with("-->") {
                markers.push((start, content.len() - tail.len() + 3, closing));
            }
        }
    }
    if candidates == 0 { return CssSourceSpan::Absent; }
    if candidates == 2 && markers.len() == 2 && !markers[0].2 && markers[1].2 {
        return CssSourceSpan::Exact(markers[0].0..markers[1].1);
    }
    CssSourceSpan::Ambiguous
}

/// Publication excludes supported guidance nodes, never ordinary prose or code examples.
pub fn strip_template_guidance(content: &str) -> String {
    let protected = match css_source_span(content) {
        CssSourceSpan::Ambiguous => return content.to_string(),
        CssSourceSpan::Exact(span) => Some(span),
        CssSourceSpan::Absent => None,
    };
    let mut offset = 0;
    let mut fence: Option<(char, usize)> = None;
    let mut output = String::new();
    for line in content.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        if protected.as_ref().is_some_and(|span| start < span.end && offset > span.start) {
            output.push_str(line);
            continue;
        }
        let trimmed = line.trim();
        let indentation = line
            .chars()
            .take_while(|character| *character == ' ')
            .count();
        let indented_code = indentation >= 4 || line[indentation..].starts_with('\t');
        let marker_char = trimmed.chars().next();
        let run = marker_char
            .map(|character| trimmed.chars().take_while(|item| *item == character).count())
            .unwrap_or(0);
        if !indented_code && matches!(marker_char, Some('`') | Some('~')) && run >= 3 {
            match fence {
                Some((character, length))
                    if marker_char == Some(character)
                        && run >= length
                        && trimmed[run..].trim().is_empty() =>
                {
                    fence = None
                }
                None => fence = Some((marker_char.unwrap(), run)),
                _ => {}
            }
            output.push_str(line);
            continue;
        }
        if fence.is_none() && !indented_code && is_guidance(trimmed) {
            continue;
        }
        output.push_str(line);
    }
    output
}

fn is_guidance(line: &str) -> bool {
    let Some(inner) = line
        .strip_prefix("<!--")
        .and_then(|value| value.strip_suffix("-->"))
    else {
        return false;
    };
    let Some(payload) = inner.trim().strip_prefix("starlane-template:v1:guidance:") else {
        return false;
    };
    let Ok(bytes) = URL_SAFE_NO_PAD.decode(payload) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    value
        .get("id")
        .and_then(|value| value.as_str())
        .map(|id| !id.is_empty() && id.len() <= 160)
        .unwrap_or(false)
        && value.get("text").and_then(|value| value.as_str()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn css_source_is_opaque_while_guidance_outside_it_is_removed() {
        let payload = URL_SAFE_NO_PAD.encode(r#"{"id":"instruction","text":"Original CSS comment"}"#);
        let guidance = format!("<!-- starlane-template:v1:guidance:{payload} -->");
        let raw = format!("<!-- Starlane:custom-css -->\r\n{guidance}\r\n/* \0 */ @import 'https://invalid.example/original';\r\n{}\r\n<!-- /starlane:custom-css -->", "x".repeat(20_100));
        let source = format!("Before\n{guidance}\n{raw}\n{guidance}\nAfter\n");
        assert_eq!(strip_template_guidance(&source), format!("Before\n{raw}\nAfter\n"));
    }
    #[test]
    fn uncertain_css_boundaries_retain_whole_source_without_guessing() {
        let payload = URL_SAFE_NO_PAD.encode(r#"{"id":"instruction","text":"Keep original"}"#);
        let guidance = format!("<!-- starlane-template:v1:guidance:{payload} -->");
        for marker in ["<!-- starlane:custom-css", "<!-- starlane:custom-c", "<!-- /starlane:custom-css -->",
            "<!-- /starlane:custom-css -->\r\n<!-- starlane:custom-css -->",
            "<!-- starlane:custom-css --><!-- starlane:custom-css --><!-- /starlane:custom-css -->",
            "<!-- starlane:custom-css --><!-- /starlane:custom-css --><!-- starlane:custom-css --><!-- /starlane:custom-css -->"] {
            let source = format!(" \r\n{guidance}\r\n{marker}\r\n\0original \r\n");
            assert_eq!(strip_template_guidance(&source), source);
            assert_eq!(css_source_span(&source), CssSourceSpan::Ambiguous);
        }
    }

    #[test]
    fn publication_omits_only_supported_guidance_and_preserves_literal_examples() {
        let payload = URL_SAFE_NO_PAD.encode(r#"{"id":"instruction","text":"Describe impact"}"#);
        let marker = format!("<!-- starlane-template:v1:guidance:{payload} -->");
        let content = format!("Keep this prose.\n{marker}\n```markdown\n{marker}\n```\n    {marker}\n\t{marker}\n<!-- starlane-template:v2:guidance:{payload} -->\n<!-- ordinary comment -->\n");
        let result = strip_template_guidance(&content);
        assert!(result.starts_with("Keep this prose.\n```markdown"));
        assert_eq!(result.matches(&marker).count(), 3);
        assert!(result.contains("v2:guidance"));
        assert!(result.contains("ordinary comment"));
    }
}
