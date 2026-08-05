use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const BIDI_CONTROLS: &[char] = &[
    '\u{061c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

pub fn terminal_text(input: &str, max_columns: usize) -> String {
    let mut output = String::with_capacity(input.len().min(max_columns));
    let mut columns = 0usize;

    for ch in input.chars() {
        if ch.is_control() || BIDI_CONTROLS.contains(&ch) {
            if columns + 1 > max_columns {
                break;
            }
            output.push('�');
            columns += 1;
            continue;
        }
        let width = ch.width().unwrap_or(0);
        if columns + width > max_columns {
            break;
        }
        output.push(ch);
        columns += width;
    }

    if UnicodeWidthStr::width(input) > columns && max_columns > 0 {
        while UnicodeWidthStr::width(output.as_str()) >= max_columns {
            output.pop();
        }
        output.push('…');
    }
    output
}

pub fn redact_uri(input: &str) -> String {
    let Some(scheme_end) = input.find("://") else {
        return terminal_text(input, 512);
    };
    let rest = &input[scheme_end + 3..];
    let Some(at) = rest.find('@') else {
        return terminal_text(input, 512);
    };
    format!(
        "{}://[redacted]@{}",
        terminal_text(&input[..scheme_end], 32),
        terminal_text(&rest[at + 1..], 480)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn strips_terminal_and_bidi_controls() {
        let value = terminal_text("safe\u{1b}]52;c;evil\u{7}\u{202e}txt", 80);
        assert!(!value.contains('\u{1b}'));
        assert!(!value.contains('\u{202e}'));
        assert!(value.contains("safe"));
    }

    #[test]
    fn redacts_uri_credentials() {
        assert_eq!(
            redact_uri("tcp://user:secret@example:2376"),
            "tcp://[redacted]@example:2376"
        );
    }

    proptest! {
        #[test]
        fn sanitized_text_never_contains_terminal_controls(input in ".{0,2048}") {
            let output = terminal_text(&input, 256);
            let safe = !output.chars().any(|ch| matches!(ch, '\u{001b}' | '\u{0007}' | '\u{202e}' | '\u{2066}' | '\u{2067}' | '\u{2068}' | '\u{2069}'));
            prop_assert!(safe, "sanitizer returned a forbidden control character");
        }
    }
}
