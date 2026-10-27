//! Release version handling for the platform updater.
//!
//! Releases are git tags `vMAJOR.MINOR.PATCH` and image tags `MAJOR.MINOR.PATCH`.
//! Pre-releases (`1.2.0-rc.1`) are ordered below their release and are never
//! offered as an update unless the running build is itself a pre-release.

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Dot-separated pre-release identifiers; empty for a stable release.
    pub pre: Vec<String>,
}

impl Version {
    /// Parse `1.2.3`, `v1.2.3` or `1.2.3-rc.1`. Build metadata (`+abc`) is ignored.
    /// Anything else (`latest`, `edge`, 7-char SHA, `1.2`) is rejected.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let s = s.strip_prefix('v').unwrap_or(s);
        let s = s.split('+').next()?;
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, p),
            None => (s, ""),
        };
        let mut it = core.split('.');
        let major = num(it.next()?)?;
        let minor = num(it.next()?)?;
        let patch = num(it.next()?)?;
        if it.next().is_some() {
            return None;
        }
        let pre: Vec<String> = if !s.contains('-') {
            Vec::new()
        } else {
            let ids: Vec<String> = pre.split('.').map(str::to_string).collect();
            if ids.iter().any(|i| i.is_empty()) {
                return None;
            }
            ids
        };
        Some(Version { major, minor, patch, pre })
    }

    pub fn is_stable(&self) -> bool {
        self.pre.is_empty()
    }
}

fn num(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Leading zeros are not valid semver.
    if s.len() > 1 && s.starts_with('0') {
        return None;
    }
    s.parse().ok()
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => cmp_pre(&self.pre, &other.pre),
            })
    }
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let o = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    a.len().cmp(&b.len())
}

/// The newest release among registry tags. Non-version tags are ignored;
/// pre-releases only count when `include_prerelease` is set.
pub fn latest_release<'a, I>(tags: I, include_prerelease: bool) -> Option<Version>
where
    I: IntoIterator<Item = &'a str>,
{
    tags.into_iter()
        .filter_map(Version::parse)
        .filter(|v| include_prerelease || v.is_stable())
        .max()
}

/// Whether `latest` should be offered to a build running `current`.
/// An unparseable `current` (`dev`, an old SHA-tagged build) is always
/// updatable to a release.
pub fn update_available(current: &str, latest: &Version) -> bool {
    match Version::parse(current) {
        Some(cur) => latest > &cur,
        None => true,
    }
}

/// Replace the tag of an `image[:tag]` reference. Handles registry ports
/// (`localhost:5000/app:1`) and digests are not supported.
pub fn with_tag(image_ref: &str, tag: &str) -> String {
    let name = match image_ref.rfind(':') {
        Some(p) if !image_ref[p + 1..].contains('/') => &image_ref[..p],
        _ => image_ref,
    };
    format!("{name}:{tag}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn parses_plain_and_prefixed() {
        assert_eq!(v("1.2.3"), v("v1.2.3"));
        assert_eq!(v("1.2.3").to_string(), "1.2.3");
        assert_eq!(v("1.2.3+build5").to_string(), "1.2.3");
        assert_eq!(v("1.2.3-rc.1").to_string(), "1.2.3-rc.1");
    }

    #[test]
    fn rejects_non_versions() {
        for s in ["latest", "edge", "a1b2c3d", "1.2", "1.2.3.4", "01.2.3", "1.2.x", "", "1.2.3-", "1.2.3-a..b"] {
            assert!(Version::parse(s).is_none(), "{s}");
        }
    }

    #[test]
    fn orders_numerically_not_lexically() {
        assert!(v("1.10.0") > v("1.9.9"));
        assert!(v("2.0.0") > v("1.99.99"));
        assert!(v("1.0.10") > v("1.0.9"));
    }

    #[test]
    fn prerelease_orders_below_release() {
        assert!(v("1.0.0-rc.1") < v("1.0.0"));
        assert!(v("1.0.0-rc.2") > v("1.0.0-rc.1"));
        assert!(v("1.0.0-rc.10") > v("1.0.0-rc.2"));
        assert!(v("1.0.0-alpha") < v("1.0.0-beta"));
        assert!(v("1.0.0-1") < v("1.0.0-alpha"));
        assert!(v("1.0.0-rc") < v("1.0.0-rc.1"));
    }

    #[test]
    fn latest_ignores_non_versions_and_prereleases() {
        let tags = ["latest", "edge", "abc1234", "1.2.0", "1.10.0", "1.11.0-rc.1", "1.9.0", "1.10"];
        assert_eq!(latest_release(tags, false), Some(v("1.10.0")));
        assert_eq!(latest_release(tags, true), Some(v("1.11.0-rc.1")));
        assert_eq!(latest_release(["latest", "abc1234"], false), None);
    }

    #[test]
    fn update_decision() {
        assert!(update_available("1.0.0", &v("1.0.1")));
        assert!(!update_available("1.0.1", &v("1.0.1")));
        assert!(!update_available("1.2.0", &v("1.0.1")));
        assert!(update_available("dev", &v("0.1.0")));
        assert!(update_available("1.0.0-rc.1", &v("1.0.0")));
    }

    #[test]
    fn retags_images() {
        assert_eq!(with_tag("user/app:latest", "1.2.3"), "user/app:1.2.3");
        assert_eq!(with_tag("user/app", "1.2.3"), "user/app:1.2.3");
        assert_eq!(with_tag("localhost:5000/app", "1.2.3"), "localhost:5000/app:1.2.3");
        assert_eq!(with_tag("localhost:5000/app:old", "1.2.3"), "localhost:5000/app:1.2.3");
    }
}
