//! Minimal semantic-version range checking used to decide whether a
/// dependency declared in a manifest refers to a workspace package.
///
/// Supports the subset of ranges that appear in practice inside monorepos:
/// `*`, `x`, `workspace:*` / `workspace:^1.2.3`, exact versions, `^` and `~`
/// ranges, comparison operators (`>=`, `>`, `<=`, `<`, `=`) and `||` unions.
use std::cmp::Ordering;

/// True when `version` satisfies `range`.
pub fn is_satisfied(range: &str, version: &str) -> bool {
    let range = normalize_protocol(range);
    range
        .split("||")
        .any(|alternative| satisfies_alternative(alternative.trim(), version))
}

fn normalize_protocol(range: &str) -> &str {
    range.strip_prefix("workspace:").unwrap_or(range)
}

fn satisfies_alternative(alternative: &str, version: &str) -> bool {
    match alternative {
        "" | "*" | "x" | "latest" => true,
        _ => alternative
            .split_whitespace()
            .all(|comparator| satisfies_comparator(comparator, version)),
    }
}

fn satisfies_comparator(comparator: &str, version: &str) -> bool {
    let parsed_version = ParsedVersion::parse(version);
    let (operator, rest) = split_operator(comparator);

    // A bare operator (`^`, `~`, `>=`, ...) places no constraint.
    if !operator.is_empty() && rest.is_empty() {
        return true;
    }

    match operator {
        ">=" => rest == "x" || ordering(parsed_version, rest) != Ordering::Less,
        ">" => rest != "x" && ordering(parsed_version, rest) == Ordering::Greater,
        "<=" => rest == "x" || ordering(parsed_version, rest) != Ordering::Greater,
        "<" => rest != "x" && ordering(parsed_version, rest) == Ordering::Less,
        "=" => matches_exact(rest, &parsed_version),
        "^" => matches_caret(rest, &parsed_version),
        "~" => matches_tilde(rest, &parsed_version),
        _ => matches_exact(comparator, &parsed_version),
    }
}

fn split_operator(comparator: &str) -> (&str, &str) {
    for op in [">=", "<=", ">", "<", "=", "^", "~"] {
        if let Some(rest) = comparator.strip_prefix(op) {
            return (op, rest);
        }
    }
    ("", comparator)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ParsedVersion {
    major: u64,
    minor: u64,
    patch: u64,
    wildcard_minor: bool,
    wildcard_patch: bool,
}

impl ParsedVersion {
    fn parse(input: &str) -> Self {
        let core = input.split(['-', '+']).next().unwrap_or(input);
        let mut parts = core.split('.');

        let major = next_number(&mut parts).unwrap_or(0);
        let minor_token = parts.next().unwrap_or("");
        let patch_token = parts.next().unwrap_or("");

        let wildcard_minor = matches!(minor_token, "" | "x" | "X" | "*");
        let wildcard_patch = wildcard_minor || matches!(patch_token, "" | "x" | "X" | "*");

        ParsedVersion {
            major,
            minor: if wildcard_minor {
                0
            } else {
                minor_token.parse().unwrap_or(0)
            },
            patch: if wildcard_patch {
                0
            } else {
                patch_token.parse().unwrap_or(0)
            },
            wildcard_minor,
            wildcard_patch,
        }
    }
}

fn next_number(parts: &mut dyn Iterator<Item = &str>) -> Option<u64> {
    parts.next()?.parse().ok()
}

fn ordering(version: ParsedVersion, other: &str) -> Ordering {
    let other = ParsedVersion::parse(other);
    let (a, b) = (
        (version.major, version.minor, version.patch),
        (other.major, other.minor, other.patch),
    );
    a.cmp(&b)
}

fn matches_exact(expected: &str, version: &ParsedVersion) -> bool {
    let expected = ParsedVersion::parse(expected);

    if expected.major != version.major {
        return false;
    }
    if !expected.wildcard_minor && expected.minor != version.minor {
        return false;
    }
    if !expected.wildcard_patch && expected.patch != version.patch {
        return false;
    }
    true
}

fn matches_caret(lower_bound: &str, version: &ParsedVersion) -> bool {
    let bound = ParsedVersion::parse(lower_bound);

    if version.major != bound.major || ordering(*version, lower_bound) == Ordering::Less {
        return false;
    }

    if bound.major > 0 {
        return true;
    }
    // ^0.y.z allows changes that do not modify y.z-leftmost non-zero digit.
    if bound.minor > 0 {
        return bound.minor == version.minor;
    }
    bound.minor == version.minor && bound.patch == version.patch
}

fn matches_tilde(lower_bound: &str, version: &ParsedVersion) -> bool {
    let bound = ParsedVersion::parse(lower_bound);

    bound.major == version.major
        && (bound.wildcard_minor || bound.minor == version.minor)
        && ordering(*version, lower_bound) != Ordering::Less
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_matches_everything() {
        assert!(is_satisfied("*", "1.2.3"));
        assert!(is_satisfied("", "9.9.9"));
        assert!(is_satisfied("latest", "0.0.1"));
    }

    #[test]
    fn workspace_protocol_is_stripped() {
        assert!(is_satisfied("workspace:*", "1.0.0"));
        assert!(is_satisfied("workspace:^", "1.0.0"));
        assert!(is_satisfied("workspace:^1.2.3", "1.5.0"));
        assert!(!is_satisfied("workspace:^2.0.0", "1.5.0"));
    }

    #[test]
    fn caret_respects_major() {
        assert!(is_satisfied("^1.2.3", "1.9.0"));
        assert!(!is_satisfied("^1.2.3", "2.0.0"));
        assert!(!is_satisfied("^1.2.3", "1.2.2"));
    }

    #[test]
    fn caret_zero_major_locks_minor() {
        assert!(is_satisfied("^0.2.3", "0.2.9"));
        assert!(!is_satisfied("^0.2.3", "0.3.0"));
        assert!(is_satisfied("^0.0.3", "0.0.3"));
        assert!(!is_satisfied("^0.0.3", "0.0.4"));
    }

    #[test]
    fn tilde_allows_patch_bumps() {
        assert!(is_satisfied("~1.2.3", "1.2.9"));
        assert!(!is_satisfied("~1.2.3", "1.3.0"));
        assert!(!is_satisfied("~1.2.3", "1.2.2"));
    }

    #[test]
    fn exact_and_wildcards() {
        assert!(is_satisfied("1.2.3", "1.2.3"));
        assert!(!is_satisfied("1.2.3", "1.2.4"));
        assert!(is_satisfied("1.x", "1.9.9"));
        assert!(!is_satisfied("1.x", "2.0.0"));
        assert!(is_satisfied("1", "1.4.2"));
    }

    #[test]
    fn comparators_and_unions() {
        assert!(is_satisfied(">=1.0.0", "1.0.0"));
        assert!(is_satisfied(">1.0.0 <2.0.0", "1.5.0"));
        assert!(!is_satisfied(">1.0.0 <2.0.0", "2.0.0"));
        assert!(is_satisfied("^1.0.0 || ^2.0.0", "2.3.0"));
        assert!(is_satisfied("^1.0.0 || ^2.0.0", "1.1.0"));
        assert!(!is_satisfied("^1.0.0 || ^2.0.0", "3.0.0"));
    }

    #[test]
    fn prerelease_suffixes_ignored_for_ordering() {
        assert!(is_satisfied("^1.0.0", "1.2.3-beta.1"));
        assert!(is_satisfied("~2.1.0+build", "2.1.5"));
    }
}
