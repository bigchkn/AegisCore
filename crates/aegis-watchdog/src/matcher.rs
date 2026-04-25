use aegis_core::{config::WatchdogPatterns, AegisError, DetectedEvent, Provider, Result};
use regex::{Regex, RegexBuilder};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PatternMatcher {
    rate_limit: Vec<Pattern>,
    auth_failure: Vec<Pattern>,
    sandbox_violation: Vec<Pattern>,
    task_complete: Vec<Pattern>,
}

#[derive(Debug, Clone)]
enum Pattern {
    Literal { raw: String, lowercase: String },
    Regex { raw: String, compiled: Regex },
}

impl PatternMatcher {
    pub fn new(patterns: &WatchdogPatterns) -> Result<Self> {
        Ok(Self {
            rate_limit: compile_group("rate_limit", &patterns.rate_limit)?,
            auth_failure: compile_group("auth_failure", &patterns.auth_failure)?,
            sandbox_violation: compile_group("sandbox_violation", &patterns.sandbox_violation)?,
            task_complete: compile_group("task_complete", &patterns.task_complete)?,
        })
    }

    pub fn detect(
        &self,
        agent_id: Uuid,
        provider: &dyn Provider,
        capture: &str,
    ) -> Option<DetectedEvent> {
        let lines: Vec<&str> = capture.lines().collect();

        detect_category(&lines, &self.auth_failure, |line| {
            provider.is_auth_error(line)
        })
        .map(|matched_pattern| DetectedEvent::AuthFailure {
            agent_id,
            matched_pattern,
        })
        .or_else(|| {
            detect_category(&lines, &self.sandbox_violation, |_| false).map(|matched_pattern| {
                DetectedEvent::SandboxViolation {
                    agent_id,
                    matched_pattern,
                }
            })
        })
        .or_else(|| {
            detect_category(&lines, &self.rate_limit, |line| {
                provider.is_rate_limit_error(line)
            })
            .map(|matched_pattern| DetectedEvent::RateLimit {
                agent_id,
                matched_pattern,
            })
        })
        .or_else(|| {
            detect_category(&lines, &self.task_complete, |line| {
                provider.is_task_complete(line)
            })
            .map(|matched_pattern| DetectedEvent::TaskComplete {
                agent_id,
                matched_pattern,
            })
        })
    }
}

fn compile_group(category: &str, patterns: &[String]) -> Result<Vec<Pattern>> {
    patterns
        .iter()
        .enumerate()
        .map(|(index, pattern)| compile_pattern(category, index, pattern))
        .collect()
}

fn compile_pattern(category: &str, index: usize, pattern: &str) -> Result<Pattern> {
    if let Some(expr) = pattern.strip_prefix("re:") {
        let compiled = RegexBuilder::new(expr)
            .case_insensitive(true)
            .build()
            .map_err(|error| AegisError::Config {
                field: format!("watchdog.patterns.{category}[{index}]"),
                reason: error.to_string(),
            })?;
        return Ok(Pattern::Regex {
            raw: pattern.to_string(),
            compiled,
        });
    }

    Ok(Pattern::Literal {
        raw: pattern.to_string(),
        lowercase: pattern.to_lowercase(),
    })
}

fn detect_category(
    lines: &[&str],
    configured: &[Pattern],
    provider_matcher: impl Fn(&str) -> bool,
) -> Option<String> {
    for line in lines {
        for pattern in configured {
            if pattern.matches(line) {
                return Some(pattern.raw().to_string());
            }
        }
        if provider_matcher(line) {
            return Some(line.trim().to_string());
        }
    }

    None
}

impl Pattern {
    fn matches(&self, line: &str) -> bool {
        match self {
            Self::Literal { lowercase, .. } => line.to_lowercase().contains(lowercase),
            Self::Regex { compiled, .. } => compiled.is_match(line),
        }
    }

    fn raw(&self) -> &str {
        match self {
            Self::Literal { raw, .. } | Self::Regex { raw, .. } => raw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::config::{EffectiveConfig, RawConfig, WatchdogPatterns};
    use aegis_providers::ProviderRegistry;

    fn patterns(
        rate_limit: &[&str],
        auth_failure: &[&str],
        sandbox_violation: &[&str],
        task_complete: &[&str],
    ) -> WatchdogPatterns {
        WatchdogPatterns {
            rate_limit: rate_limit.iter().map(|value| value.to_string()).collect(),
            auth_failure: auth_failure.iter().map(|value| value.to_string()).collect(),
            sandbox_violation: sandbox_violation
                .iter()
                .map(|value| value.to_string())
                .collect(),
            task_complete: task_complete
                .iter()
                .map(|value| value.to_string())
                .collect(),
        }
    }

    fn default_registry() -> ProviderRegistry {
        let cfg = EffectiveConfig::resolve(&RawConfig::default(), &RawConfig::default()).unwrap();
        ProviderRegistry::from_config(&cfg).unwrap()
    }

    #[test]
    fn literal_pattern_case_insensitive() {
        let matcher =
            PatternMatcher::new(&patterns(&["too many requests"], &[], &[], &[])).unwrap();
        let registry = default_registry();
        let provider = registry.get("codex").unwrap();

        let event = matcher
            .detect(
                Uuid::new_v4(),
                provider,
                "Server says TOO MANY REQUESTS right now",
            )
            .unwrap();

        assert!(matches!(event, DetectedEvent::RateLimit { .. }));
    }

    #[test]
    fn regex_pattern_prefix() {
        let matcher =
            PatternMatcher::new(&patterns(&["re:429\\s+too\\s+many"], &[], &[], &[])).unwrap();
        let registry = default_registry();
        let provider = registry.get("codex").unwrap();

        let event = matcher
            .detect(Uuid::new_v4(), provider, "error: 429   TOO MANY requests")
            .unwrap();

        assert!(matches!(event, DetectedEvent::RateLimit { .. }));
    }

    #[test]
    fn invalid_regex_is_config_error() {
        let error = PatternMatcher::new(&patterns(&["re:("], &[], &[], &[])).unwrap_err();

        match error {
            AegisError::Config { field, .. } => {
                assert_eq!(field, "watchdog.patterns.rate_limit[0]");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn match_priority_auth_over_rate_limit() {
        let matcher =
            PatternMatcher::new(&patterns(&["rate limit"], &["invalid api key"], &[], &[]))
                .unwrap();
        let registry = default_registry();
        let provider = registry.get("codex").unwrap();

        let capture = "warning: rate limit approaching\nfatal: invalid api key";
        let event = matcher.detect(Uuid::new_v4(), provider, capture).unwrap();

        assert!(matches!(event, DetectedEvent::AuthFailure { .. }));
    }

    #[test]
    fn provider_manifest_rate_limit_patterns_are_detected() {
        let matcher = PatternMatcher::new(&patterns(&[], &[], &[], &[])).unwrap();
        let registry = default_registry();
        let agent_id = Uuid::new_v4();

        for (name, definition) in &registry.manifest.providers {
            let provider = registry.get(name).unwrap();
            for sample in &definition.error_patterns.rate_limit {
                let capture = format!("provider output: {sample} (retry later)");
                let event = matcher.detect(agent_id, provider, &capture);
                assert!(
                    matches!(event, Some(DetectedEvent::RateLimit { .. })),
                    "provider `{name}` failed rate-limit sample `{sample}`"
                );
            }
        }
    }

    #[test]
    fn provider_manifest_auth_patterns_are_detected() {
        let matcher = PatternMatcher::new(&patterns(&[], &[], &[], &[])).unwrap();
        let registry = default_registry();
        let agent_id = Uuid::new_v4();

        for (name, definition) in &registry.manifest.providers {
            let provider = registry.get(name).unwrap();
            for sample in &definition.error_patterns.auth {
                let capture = format!("provider output: {sample} (check credentials)");
                let event = matcher.detect(agent_id, provider, &capture);
                assert!(
                    matches!(event, Some(DetectedEvent::AuthFailure { .. })),
                    "provider `{name}` failed auth sample `{sample}`"
                );
            }
        }
    }

    #[test]
    fn watchdog_config_patterns_extend_provider_patterns() {
        let matcher =
            PatternMatcher::new(&patterns(&["re:temporarily unavailable"], &[], &[], &[])).unwrap();
        let registry = default_registry();
        let provider = registry.get("codex").unwrap();
        let agent_id = Uuid::new_v4();

        let configured = matcher.detect(agent_id, provider, "service TEMPORARILY unavailable");
        assert!(matches!(configured, Some(DetectedEvent::RateLimit { .. })));

        let provider_owned = matcher.detect(agent_id, provider, "429 too many requests");
        assert!(matches!(
            provider_owned,
            Some(DetectedEvent::RateLimit { .. })
        ));
    }

    #[test]
    fn sandbox_and_task_complete_patterns_are_detected() {
        let matcher = PatternMatcher::new(&patterns(
            &[],
            &[],
            &["Operation not permitted"],
            &["[AEGIS:DONE]"],
        ))
        .unwrap();
        let registry = default_registry();
        let provider = registry.get("codex").unwrap();
        let agent_id = Uuid::new_v4();

        let sandbox = matcher.detect(agent_id, provider, "open failed: Operation not permitted");
        assert!(matches!(
            sandbox,
            Some(DetectedEvent::SandboxViolation { .. })
        ));

        let completion = matcher.detect(agent_id, provider, "status [AEGIS:DONE]");
        assert!(matches!(
            completion,
            Some(DetectedEvent::TaskComplete { .. })
        ));
    }
}
