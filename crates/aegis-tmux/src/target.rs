use crate::TmuxError;

/// A validated tmux target string in the form `session:window.pane`,
/// e.g. `"aegis:0.%3"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxTarget(String);

impl TmuxTarget {
    /// Parse a raw target string. Returns `Err` if the string is empty.
    pub fn parse(s: &str) -> Result<Self, TmuxError> {
        if s.is_empty() {
            return Err(TmuxError::InvalidTarget { raw: s.to_owned() });
        }
        Ok(Self(s.to_owned()))
    }

    /// Construct from components: `"session:window.pane"`.
    pub fn new(session: &str, window: u32, pane: &str) -> Self {
        Self(format!("{session}:{window}.{pane}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the session name component (everything before the first `:`).
    pub fn session(&self) -> &str {
        self.0.split(':').next().unwrap_or(&self.0)
    }
}

impl std::fmt::Display for TmuxTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_empty() {
        assert!(TmuxTarget::parse("").is_err());
    }

    #[test]
    fn new_formats_correctly() {
        let t = TmuxTarget::new("aegis", 0, "%3");
        assert_eq!(t.as_str(), "aegis:0.%3");
        assert_eq!(t.session(), "aegis");
    }

    #[test]
    fn display_matches_inner() {
        let t = TmuxTarget::parse("mysession:1.%5").unwrap();
        assert_eq!(t.to_string(), "mysession:1.%5");
    }
}
