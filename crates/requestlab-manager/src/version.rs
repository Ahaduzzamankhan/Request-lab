//! Semantic versions: parsing, precedence and prerelease handling.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

/// One dotted prerelease identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Identifier {
    Numeric(u64),
    Alpha(String),
}

impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Identifier::Numeric(a), Identifier::Numeric(b)) => a.cmp(b),
            (Identifier::Numeric(_), Identifier::Alpha(_)) => Ordering::Less,
            (Identifier::Alpha(_), Identifier::Numeric(_)) => Ordering::Greater,
            (Identifier::Alpha(a), Identifier::Alpha(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Identifier::Numeric(value) => write!(f, "{value}"),
            Identifier::Alpha(value) => f.write_str(value),
        }
    }
}

/// `MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Vec<Identifier>,
}

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: Vec::new(),
        }
    }

    pub fn is_prerelease(&self) -> bool {
        !self.prerelease.is_empty()
    }

    /// The git tag this version is published under.
    pub fn tag(&self) -> String {
        format!("v{self}")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.prerelease.is_empty() {
            f.write_str("-")?;
            for (index, identifier) in self.prerelease.iter().enumerate() {
                if index > 0 {
                    f.write_str(".")?;
                }
                write!(f, "{identifier}")?;
            }
        }
        Ok(())
    }
}

impl FromStr for Version {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let trimmed = input.trim();
        let core_and_pre = trimmed
            .strip_prefix(['v', 'V'])
            .unwrap_or(trimmed)
            .split_once('+')
            .map(|(left, _)| left)
            .unwrap_or_else(|| trimmed.strip_prefix(['v', 'V']).unwrap_or(trimmed));
        let (core, pre) = match core_and_pre.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (core_and_pre, None),
        };

        let mut parts = core.split('.');
        let major = number(parts.next(), "major")?;
        let minor = number(parts.next(), "minor")?;
        let patch = number(parts.next(), "patch")?;
        if parts.next().is_some() {
            return Err(format!("'{input}' has more than three numeric components"));
        }

        let mut prerelease = Vec::new();
        if let Some(pre) = pre {
            if pre.is_empty() {
                return Err(format!("'{input}' has an empty prerelease suffix"));
            }
            for identifier in pre.split('.') {
                if identifier.is_empty() {
                    return Err(format!("'{input}' has an empty prerelease identifier"));
                }
                if identifier.chars().all(|c| c.is_ascii_digit()) {
                    let value = identifier
                        .parse::<u64>()
                        .map_err(|_| format!("'{identifier}' is not a valid numeric identifier"))?;
                    prerelease.push(Identifier::Numeric(value));
                } else if identifier
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
                {
                    prerelease.push(Identifier::Alpha(identifier.to_string()));
                } else {
                    return Err(format!("'{identifier}' has invalid characters"));
                }
            }
        }

        Ok(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }
}

fn number(part: Option<&str>, name: &str) -> Result<u64, String> {
    let part = part.ok_or_else(|| format!("missing {name} version component"))?;
    if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "'{part}' is not a numeric {name} version component"
        ));
    }
    part.parse::<u64>()
        .map_err(|_| format!("{name} component '{part}' is out of range"))
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(
                || match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
                    (true, true) => Ordering::Equal,
                    (true, false) => Ordering::Greater,
                    (false, true) => Ordering::Less,
                    (false, false) => self.prerelease.cmp(&other.prerelease),
                },
            )
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl serde::Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(input: &str) -> Version {
        input.parse().expect("test version")
    }

    #[test]
    fn parses_prefixed_prerelease_and_metadata() {
        assert_eq!(v("1.2.3"), Version::new(1, 2, 3));
        assert_eq!(v("v1.2.3"), Version::new(1, 2, 3));
        assert_eq!(v("1.0.0-rc.1+build.7"), v("1.0.0-rc.1"));
        assert!(v("1.0.0-beta.2").is_prerelease());
    }

    #[test]
    fn rejects_malformed_versions() {
        for input in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "1.2.x",
            "1.2.3-",
            "1.2.3-a..b",
            "one",
        ] {
            assert!(
                input.parse::<Version>().is_err(),
                "'{input}' must not parse"
            );
        }
    }

    #[test]
    fn orders_by_semver_precedence() {
        assert!(v("1.0.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("1.0.0-rc.1"));
        assert!(v("1.0.0-rc.1") > v("1.0.0-beta.9"));
        assert!(v("1.0.0-beta.10") > v("1.0.0-beta.9"));
        assert!(v("1.0.0-alpha") > v("1.0.0-1"));
    }

    #[test]
    fn formats_and_tags_round_trip() {
        for input in ["0.1.1", "1.2.3-beta.1"] {
            let parsed = v(input);
            assert_eq!(parsed.to_string(), input);
            assert_eq!(parsed.tag(), format!("v{input}"));
            assert_eq!(parsed.tag().parse::<Version>().unwrap(), parsed);
        }
    }
}
