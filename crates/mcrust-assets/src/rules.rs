use std::collections::HashMap;

use serde::Deserialize;

/// OS/feature rules from `version.json`.
///
/// Vanilla starts with `allowed = rules.is_empty()`, then every matching rule
/// overwrites the decision with its action.
#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    #[serde(default = "default_action")]
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

fn default_action() -> String {
    "allow".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

impl Rule {
    fn matches(&self, features: &HashMap<String, bool>) -> bool {
        if let Some(os) = &self.os {
            if let Some(name) = &os.name
                && !name.eq_ignore_ascii_case(current_os_name())
            {
                return false;
            }
            if let Some(arch) = &os.arch
                && !arch.eq_ignore_ascii_case(current_arch())
            {
                return false;
            }
            // `version` is a Windows-only regex in the launcher. Minecraft 26.x
            // only uses it for legacy G1GC flags, so a missing match is safe.
            if let Some(version) = &os.version
                && !os_version_matches(version)
            {
                return false;
            }
        }

        if let Some(required) = &self.features {
            for (feature, expected) in required {
                if features.get(feature).copied().unwrap_or(false) != *expected {
                    return false;
                }
            }
        }

        true
    }
}

pub fn current_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

fn current_arch() -> &'static str {
    if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "x86_64"
    }
}

fn os_version_matches(_pattern: &str) -> bool {
    // Version ranges only gate legacy Windows JVM flags. Treat them as a match
    // on non-Windows and accept the modern range on Windows.
    true
}

/// Evaluates a list of vanilla rules against the active feature flags.
pub fn rules_allow(rules: &[Rule], features: &HashMap<String, bool>) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule.matches(features) {
            allowed = rule.action != "disallow";
        }
    }
    allowed
}

/// Picks the native classifier that applies to the current platform.
pub fn native_classifier() -> &'static str {
    if cfg!(target_os = "windows") {
        "natives-windows"
    } else if cfg!(target_os = "macos") {
        "natives-macos"
    } else {
        "natives-linux"
    }
}

/// macOS arm64 has its own classifier set.
pub fn native_classifier_for_arch() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "natives-macos-arm64"
    } else {
        native_classifier()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn empty_rules_allow() {
        assert!(rules_allow(&[], &HashMap::new()));
    }

    #[test]
    fn disallow_wins_when_listed_last() {
        let r = rules(
            r#"[
                {"action":"allow"},
                {"action":"disallow","os":{"name":"linux"}}
            ]"#,
        );
        let allowed = rules_allow(&r, &HashMap::new());
        if cfg!(target_os = "linux") {
            assert!(!allowed);
        } else {
            assert!(allowed);
        }
    }

    #[test]
    fn features_must_match() {
        let r = rules(r#"[{"action":"allow","features":{"is_demo_user":true}}]"#);
        assert!(!rules_allow(&r, &HashMap::new()));
        assert!(rules_allow(
            &r,
            &HashMap::from([("is_demo_user".to_string(), true)])
        ));
    }
}
