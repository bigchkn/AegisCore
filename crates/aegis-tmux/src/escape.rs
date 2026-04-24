/// Wraps `input` in single quotes for shell use, escaping internal single
/// quotes as `'\''`. Suitable for constructing shell commands passed to
/// tmux (e.g. the shell string in `pipe-pane -o`).
pub fn escape_for_send_keys(input: &str) -> String {
    format!("'{}'", input.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_string_wrapped() {
        assert_eq!(escape_for_send_keys("hello world"), "'hello world'");
    }

    #[test]
    fn single_quote_escaped() {
        assert_eq!(escape_for_send_keys("it's"), r"'it'\''s'");
    }

    #[test]
    fn empty_string() {
        assert_eq!(escape_for_send_keys(""), "''");
    }
}
