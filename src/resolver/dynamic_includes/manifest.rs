//! Purpose:
//! Loads `elephc.includes.toml`, the per-application manifest that closes the source graph of
//! include/require sites whose path is only known at runtime.
//!
//! Called from:
//! - `crate::resolver::dynamic_includes::lower_dynamic_include` and discovery.
//!
//! Key details:
//! - Each `[[site]]` names one site id (`<file relative to the root>@<Class::method>#<ordinal>`),
//!   pins the md5 of the file holding it, and says either `candidates = [globs]` (relative to the
//!   root) or `deny = "reason"`. Anything else is refused with a clear error.
//! - The manifest is the one passed with `--include-manifest`, or else the first
//!   `elephc.includes.toml` found walking up from the directory of the site's file. Its
//!   directory is the application root that ids and globs are relative to.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// File name searched for when no `--include-manifest` is given.
pub(crate) const MANIFEST_FILE: &str = "elephc.includes.toml";

/// What a manifest says about one site.
#[derive(Clone, Debug)]
pub(crate) enum SiteAction {
    /// Globs relative to the root; expanded at compile time.
    Candidates(Vec<String>),
    /// The site is refused at runtime with this reason.
    Deny(String),
}

/// One `[[site]]` entry.
#[derive(Clone, Debug)]
pub(crate) struct SiteEntry {
    pub(crate) md5: String,
    pub(crate) action: SiteAction,
}

/// A parsed manifest.
#[derive(Debug)]
pub(crate) struct Manifest {
    pub(crate) path: PathBuf,
    pub(crate) root: PathBuf,
    pub(crate) sites: HashMap<String, SiteEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    site: Vec<RawSite>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSite {
    id: String,
    md5: String,
    #[serde(default)]
    candidates: Option<Vec<String>>,
    #[serde(default)]
    deny: Option<String>,
}

impl Manifest {
    /// Parses the manifest at `path`.
    pub(crate) fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read include manifest '{}': {}", path.display(), e))?;
        let raw: RawManifest = toml::from_str(&text)
            .map_err(|e| format!("invalid include manifest '{}': {}", path.display(), e))?;
        if let Some(version) = raw.version {
            if version != 1 {
                return Err(format!(
                    "include manifest '{}': unsupported version {} (expected 1)",
                    path.display(),
                    version
                ));
            }
        }
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let root = canonical
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let mut sites = HashMap::new();
        for site in raw.site {
            let action = match (site.candidates, site.deny) {
                (Some(candidates), None) => {
                    if candidates.is_empty() {
                        return Err(format!(
                            "include manifest '{}': site '{}' has an empty `candidates` list \
                             (use `deny = \"reason\"` to refuse a site)",
                            path.display(),
                            site.id
                        ));
                    }
                    SiteAction::Candidates(candidates)
                }
                (None, Some(reason)) => SiteAction::Deny(reason),
                _ => {
                    return Err(format!(
                        "include manifest '{}': site '{}' must declare exactly one of \
                         `candidates = [...]` or `deny = \"reason\"`",
                        path.display(),
                        site.id
                    ))
                }
            };
            let md5 = site.md5.to_ascii_lowercase();
            if md5.len() != 32 || !md5.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!(
                    "include manifest '{}': site '{}' has an invalid md5 '{}'",
                    path.display(),
                    site.id,
                    site.md5
                ));
            }
            if sites
                .insert(site.id.clone(), SiteEntry { md5, action })
                .is_some()
            {
                return Err(format!(
                    "include manifest '{}': site '{}' is listed twice",
                    path.display(),
                    site.id
                ));
            }
        }
        Ok(Self {
            path: canonical,
            root,
            sites,
        })
    }

    /// Expands one candidate glob into the canonical paths of the FILES it matches, sorted.
    pub(crate) fn expand_glob(&self, pattern: &str) -> Result<Vec<PathBuf>, String> {
        if Path::new(pattern).is_absolute() {
            return Err(format!(
                "include manifest '{}': candidate '{}' must be relative to the application root",
                self.path.display(),
                pattern
            ));
        }
        let mut out = Vec::new();
        for expanded in expand_braces(pattern) {
            let segments: Vec<&str> = expanded.split('/').filter(|s| !s.is_empty()).collect();
            walk_glob(&self.root, &segments, &mut out);
        }
        let mut canonical: Vec<PathBuf> = out
            .into_iter()
            .filter(|p| p.is_file())
            .map(|p| p.canonicalize().unwrap_or(p))
            .collect();
        canonical.sort();
        canonical.dedup();
        Ok(canonical)
    }
}

/// Finds the manifest for a site in `file`: the override, else walking up from its directory.
pub(crate) fn locate(file: &Path, override_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = override_path {
        return Some(path.to_path_buf());
    }
    let mut dir = file.parent();
    while let Some(current) = dir {
        let candidate = current.join(MANIFEST_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = current.parent();
    }
    None
}

/// Lowercase hex md5 of a file's bytes.
pub(crate) fn file_md5(path: &Path) -> Result<String, String> {
    use md5::{Digest, Md5};
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|mut f| f.read_to_end(&mut bytes))
        .map_err(|e| format!("cannot read '{}' to check its md5: {}", path.display(), e))?;
    let digest = Md5::digest(&bytes);
    Ok(digest.iter().map(|b| format!("{:02x}", b)).collect())
}

/// Expands `{a,b}` alternatives (non-nested commas, nesting allowed) into plain patterns.
fn expand_braces(pattern: &str) -> Vec<String> {
    let bytes = pattern.as_bytes();
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let mut depth = 0usize;
    let mut close = None;
    let mut commas = Vec::new();
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            b',' if depth == 1 => commas.push(i),
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![pattern.to_string()];
    };
    let prefix = &pattern[..open];
    let suffix = &pattern[close + 1..];
    let mut bounds = vec![open];
    bounds.extend(commas);
    bounds.push(close);
    let mut out = Vec::new();
    for window in bounds.windows(2) {
        let alternative = &pattern[window[0] + 1..window[1]];
        for tail in expand_braces(&format!("{}{}{}", prefix, alternative, suffix)) {
            out.push(tail);
        }
    }
    out
}

/// Matches `segments` (one glob per path component, `**` = any depth) below `dir`.
fn walk_glob(dir: &Path, segments: &[&str], out: &mut Vec<PathBuf>) {
    let Some((first, rest)) = segments.split_first() else {
        out.push(dir.to_path_buf());
        return;
    };
    if *first == "**" {
        walk_glob(dir, rest, out);
        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut children: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            children.sort();
            for child in children {
                if child.is_dir() {
                    walk_glob(&child, segments, out);
                }
            }
        }
        return;
    }
    if !first.contains(['*', '?', '[']) {
        let next = dir.join(first);
        if next.exists() {
            walk_glob(&next, rest, out);
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<(String, PathBuf)> = entries
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    children.sort();
    for (name, path) in children {
        // Like a shell glob, a wildcard does not match a leading dot.
        if name.starts_with('.') && !first.starts_with('.') {
            continue;
        }
        if wildcard_match(first.as_bytes(), name.as_bytes()) {
            walk_glob(&path, rest, out);
        }
    }
}

/// `*`, `?` and `[...]` matching of one path component.
fn wildcard_match(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|i| wildcard_match(rest, &text[i..])),
        Some((b'?', rest)) => !text.is_empty() && wildcard_match(rest, &text[1..]),
        Some((b'[', rest)) => {
            let Some(end) = rest.iter().position(|&b| b == b']') else {
                return text.first() == Some(&b'[') && wildcard_match(rest, &text[1..]);
            };
            let Some((&c, text_rest)) = text.split_first() else {
                return false;
            };
            let class = &rest[..end];
            let (negated, class) = match class.split_first() {
                Some((b'!', tail)) | Some((b'^', tail)) => (true, tail),
                _ => (false, class),
            };
            let mut hit = false;
            let mut i = 0;
            while i < class.len() {
                if i + 2 < class.len() && class[i + 1] == b'-' {
                    if class[i] <= c && c <= class[i + 2] {
                        hit = true;
                    }
                    i += 3;
                } else {
                    if class[i] == c {
                        hit = true;
                    }
                    i += 1;
                }
            }
            hit != negated && wildcard_match(&rest[end + 1..], text_rest)
        }
        Some((&b, rest)) => text.first() == Some(&b) && wildcard_match(rest, &text[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braces_expand_in_order() {
        assert_eq!(
            expand_braces("bootstrap/cache/{packages,services}.php"),
            vec!["bootstrap/cache/packages.php", "bootstrap/cache/services.php"]
        );
        assert_eq!(expand_braces("a/*.php"), vec!["a/*.php"]);
    }

    #[test]
    fn wildcards_match_one_component() {
        assert!(wildcard_match(b"*.php", b"app.php"));
        assert!(!wildcard_match(b"*.php", b"app.phpx"));
        assert!(wildcard_match(b"a?c", b"abc"));
        assert!(wildcard_match(b"[a-c]x", b"bx"));
        assert!(!wildcard_match(b"[!a-c]x", b"bx"));
    }
}
