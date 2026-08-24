use nodejs_semver::{Range, Version};

/// True when an npm version satisfies an npm-compatible range.
pub fn is_satisfied(range: &str, version: &str) -> bool {
    if matches!(range, "" | "*" | "x" | "X" | "latest") {
        return true;
    }

    let range = range.strip_prefix("workspace:").unwrap_or(range);
    if matches!(range, "" | "*" | "^" | "~") {
        return true;
    }

    let (Ok(range), Ok(version)) = (range.parse::<Range>(), version.parse::<Version>()) else {
        return false;
    };
    version.satisfies(&range)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_npm_range_semantics() {
        assert!(is_satisfied("workspace:^", "1.0.0"));
        assert!(is_satisfied("^1.2.3", "1.9.0"));
        assert!(is_satisfied("1.2.3 - 2.3.4", "2.0.0"));
        assert!(is_satisfied(">1.0.0 <2.0.0", "1.5.0"));
        assert!(!is_satisfied("^1.2.3", "2.0.0"));
        assert!(!is_satisfied("not-a-range", "1.0.0"));
        assert!(!is_satisfied("^1.0.0", "not-a-version"));
    }
}
