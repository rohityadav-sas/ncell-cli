use std::borrow::Cow;

/// Check whether a character is a control character or a Unicode bidirectional
/// override/isolate character that could disrupt terminal rendering.
#[inline]
pub fn is_unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}'
        )
}

/// Strip control and bidi-override characters from a string.
/// Returns borrowed `Cow` without allocation when no unsafe characters are present.
pub fn sanitize(s: &str) -> Cow<'_, str> {
    if s.chars().any(is_unsafe_char) {
        Cow::Owned(s.chars().filter(|c| !is_unsafe_char(*c)).collect())
    } else {
        Cow::Borrowed(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_string_returns_borrowed() {
        let input = "Hello, Ncell 2026!";
        let result = sanitize(input);
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, input);
    }

    #[test]
    fn strips_control_characters() {
        let input = "Hello\x00\x1b[31mWorld\x07!";
        let result = sanitize(input);
        assert_eq!(result, "Hello[31mWorld!");
    }

    #[test]
    fn strips_bidi_override_characters() {
        let input = "Admin\u{202E}txt.exe";
        let result = sanitize(input);
        assert_eq!(result, "Admintxt.exe");
    }
}
