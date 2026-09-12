use regex::{Captures, Regex};
use serde::Serialize;
use std::{fmt, net::Ipv6Addr, str::FromStr};
use thiserror::Error;

const MAX_TEXT_BYTES: usize = 16 * 1024;

/// The reason a substring was removed. Markers are intentionally stable so a
/// preview UI can explain redactions without knowing the removed value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionKind {
    Path,
    Identity,
    Network,
    Secret,
    DeviceIdentifier,
}

impl RedactionKind {
    fn marker(self) -> &'static str {
        match self {
            Self::Path => "<redacted:path>",
            Self::Identity => "<redacted:identity>",
            Self::Network => "<redacted:network>",
            Self::Secret => "<redacted:secret>",
            Self::DeviceIdentifier => "<redacted:device-id>",
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SanitizeError {
    #[error("diagnostic text exceeds the {MAX_TEXT_BYTES}-byte limit")]
    TooLong,
    #[error("diagnostic text contains a forbidden control character")]
    ControlCharacter,
    #[error("sensitive literals must contain at least three non-whitespace characters")]
    InvalidSensitiveLiteral,
}

/// Text that has passed through the sanitizer. Its inner value is private so
/// model code cannot accidentally serialize unreviewed input.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SanitizedText(String);

impl SanitizedText {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SanitizedText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SanitizedText")
            .field(&self.0)
            .finish()
    }
}

#[derive(Clone)]
struct LiteralRule {
    matcher: Regex,
    kind: RedactionKind,
}

/// Stateful sanitizer for one bundle construction run.
///
/// Register values known by the application (for example the current account
/// name and home directory) with [`Sanitizer::add_sensitive_literal`]. They
/// are used only while sanitizing and are never included in a bundle.
pub struct Sanitizer {
    literal_rules: Vec<LiteralRule>,
    redaction_count: usize,
}

impl fmt::Debug for Sanitizer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Sanitizer")
            .field("literal_rule_count", &self.literal_rules.len())
            .field("redaction_count", &self.redaction_count)
            .finish()
    }
}

impl Default for Sanitizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Sanitizer {
    pub fn new() -> Self {
        Self {
            literal_rules: Vec::new(),
            redaction_count: 0,
        }
    }

    /// Add a literal identity or path known to the host application. Matching
    /// is case-insensitive to cover Windows path/account casing differences.
    pub fn add_sensitive_literal(
        &mut self,
        literal: impl AsRef<str>,
        kind: RedactionKind,
    ) -> Result<(), SanitizeError> {
        let literal = literal.as_ref().trim();
        if literal
            .chars()
            .filter(|character| !character.is_whitespace())
            .count()
            < 3
        {
            return Err(SanitizeError::InvalidSensitiveLiteral);
        }

        // Escaping makes the caller-provided value data, never regex syntax.
        let matcher = Regex::new(&format!("(?i){}", regex::escape(literal)))
            .expect("an escaped literal always forms a valid regex");
        self.literal_rules.push(LiteralRule { matcher, kind });
        Ok(())
    }

    pub fn sanitize(&mut self, input: impl AsRef<str>) -> Result<SanitizedText, SanitizeError> {
        let input = input.as_ref();
        if input.len() > MAX_TEXT_BYTES {
            return Err(SanitizeError::TooLong);
        }
        if input
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(SanitizeError::ControlCharacter);
        }

        let mut output = input.to_owned();
        for rule in &self.literal_rules {
            let replacements = rule.matcher.find_iter(&output).count();
            if replacements > 0 {
                output = rule
                    .matcher
                    .replace_all(&output, rule.kind.marker())
                    .into_owned();
                self.redaction_count += replacements;
            }
        }

        output = self.redact_patterns(output);
        Ok(SanitizedText(output))
    }

    pub(crate) fn redaction_count(&self) -> usize {
        self.redaction_count
    }

    fn replace_all(&mut self, input: String, pattern: &str, marker: RedactionKind) -> String {
        let regex = Regex::new(pattern).expect("built-in redaction regex is valid");
        let replacements = regex.find_iter(&input).count();
        self.redaction_count += replacements;
        regex.replace_all(&input, marker.marker()).into_owned()
    }

    fn redact_patterns(&mut self, mut output: String) -> String {
        // Recovery keys and credential-like labeled values precede generic
        // number/network handling so the strongest marker wins.
        output = self.replace_all(output, r"(?i)\b\d{6}(?:-\d{6}){7}\b", RedactionKind::Secret);
        output = self.replace_all(
            output,
            r"(?i)\b(?:recovery[-_ ]?key|bitlocker[-_ ]?key|password|passphrase|secret|access[-_ ]?token|api[-_ ]?key)\s*[:=]\s*[^\s,;]+",
            RedactionKind::Secret,
        );
        output = self.replace_all(
            output,
            r"(?i)\b(?:disk[-_ ]?serial|serial(?:[-_ ]?(?:number|no))?|wwn|world[-_ ]?wide[-_ ]?name)\s*[:=]\s*[^\s,;]+",
            RedactionKind::DeviceIdentifier,
        );
        output = self.replace_all(
            output,
            r"(?i)\b(?:user(?:name)?|account(?:[-_ ]?name)?)\s*[:=]\s*[^\s,;]+",
            RedactionKind::Identity,
        );
        output = self.replace_all(
            output,
            r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b",
            RedactionKind::Identity,
        );

        // Quoted paths may contain spaces. Unquoted paths intentionally stop
        // at whitespace, which errs toward removing at least the identifying
        // home-path portion rather than retaining it.
        output = self.replace_all(
            output,
            r#"(?i)[\"'][A-Z]:\\[^\"'\r\n]+[\"']"#,
            RedactionKind::Path,
        );
        output = self.replace_all(
            output,
            r#"(?i)(?:[A-Z]:\\|\\\\)[^\s\"'<>|]+"#,
            RedactionKind::Path,
        );
        output = self.replace_posix_paths(output);

        output = self.replace_all(
            output,
            r"(?i)\b(?:[0-9A-F]{2}[:-]){5}[0-9A-F]{2}\b",
            RedactionKind::Network,
        );
        output = self.replace_ipv4(output);
        output = self.replace_ipv6(output);
        output
    }

    fn replace_posix_paths(&mut self, input: String) -> String {
        let regex = Regex::new(r#"(?m)(^|[\s(=\"'])(/[^\s\"'<>|]+)"#)
            .expect("built-in path regex is valid");
        let replacements = regex.captures_iter(&input).count();
        self.redaction_count += replacements;
        regex
            .replace_all(&input, |captures: &Captures<'_>| {
                format!("{}{}", &captures[1], RedactionKind::Path.marker())
            })
            .into_owned()
    }

    fn replace_ipv4(&mut self, input: String) -> String {
        let regex =
            Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").expect("built-in IPv4 regex is valid");
        regex
            .replace_all(&input, |captures: &Captures<'_>| {
                let candidate = &captures[0];
                if candidate.parse::<std::net::Ipv4Addr>().is_ok() {
                    self.redaction_count += 1;
                    RedactionKind::Network.marker().to_owned()
                } else {
                    candidate.to_owned()
                }
            })
            .into_owned()
    }

    fn replace_ipv6(&mut self, input: String) -> String {
        // Require a colon and at least two hex groups. Validation by Ipv6Addr
        // avoids treating ordinary colon-delimited log text as an address.
        let regex = Regex::new(r"(?i)(?:\b|\[)([0-9A-F]{0,4}(?::[0-9A-F]{0,4}){2,7})(?:\b|\])")
            .expect("built-in IPv6 regex is valid");
        regex
            .replace_all(&input, |captures: &Captures<'_>| {
                let candidate = &captures[1];
                if Ipv6Addr::from_str(candidate).is_ok() {
                    self.redaction_count += 1;
                    RedactionKind::Network.marker().to_owned()
                } else {
                    captures[0].to_owned()
                }
            })
            .into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_control_characters_and_tiny_literals() {
        let mut sanitizer = Sanitizer::new();
        assert_eq!(
            sanitizer.add_sensitive_literal("ab", RedactionKind::Identity),
            Err(SanitizeError::InvalidSensitiveLiteral)
        );
        assert_eq!(
            sanitizer.sanitize("hello\0world"),
            Err(SanitizeError::ControlCharacter)
        );
    }

    #[test]
    fn redacts_sensitive_fixture() {
        let mut sanitizer = Sanitizer::new();
        sanitizer
            .add_sensitive_literal("diogo", RedactionKind::Identity)
            .unwrap();
        sanitizer
            .add_sensitive_literal(r"C:\Users\diogo", RedactionKind::Path)
            .unwrap();

        let fixture = concat!(
            "username=diogo email=diogo@example.test\n",
            "image=C:\\Users\\diogo\\Downloads\\omarchy.iso\n",
            "mac path='/Users/alice/My Images/omarchy.img'\n",
            "linux=/home/bob/private/file.log\n",
            "disk serial: WD-WCC4E1234567 WWN=0x5000cca123456789\n",
            "recovery-key=123456-234567-345678-456789-567890-678901-789012-890123\n",
            "peers 192.168.1.42, 2001:db8::5, AA:BB:CC:DD:EE:FF"
        );

        let safe = sanitizer.sanitize(fixture).unwrap();
        let output = safe.as_str().to_ascii_lowercase();
        for forbidden in [
            "diogo",
            "alice",
            "bob",
            "wd-wcc4e1234567",
            "0x5000cca123456789",
            "123456-234567",
            "192.168.1.42",
            "2001:db8::5",
            "aa:bb:cc:dd:ee:ff",
        ] {
            assert!(!output.contains(forbidden), "leaked {forbidden}: {output}");
        }
        assert!(output.contains("<redacted:path>"));
        assert!(output.contains("<redacted:identity>"));
        assert!(output.contains("<redacted:device-id>"));
        assert!(output.contains("<redacted:secret>"));
        assert!(output.contains("<redacted:network>"));
    }

    #[test]
    fn leaves_invalid_ipv4_version_text_alone() {
        let mut sanitizer = Sanitizer::new();
        assert_eq!(
            sanitizer.sanitize("version 999.2.3.4").unwrap().as_str(),
            "version 999.2.3.4"
        );
    }
}
