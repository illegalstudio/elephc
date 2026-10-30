//! Purpose:
//! RFC 3986 URI model used by `Uri\Rfc3986\Uri`: components, raw and normalized reads,
//! recomposition, equality, and reference resolution. Parsing lives in `parse`, syntax
//! normalization in `normalize`, and component mutation in `mutate`.
//!
//! Called from:
//! - `crate::UriValue` for every RFC 3986 operation exposed through the C ABI.
//!
//! Key details:
//! - The model mirrors uriparser, the library bundled by PHP's ext/uri: a raw component is
//!   kept verbatim, `get*` returns uriparser's normalized form, and serialization always
//!   canonicalizes IPv6 through its parsed address (expanded eight-group form).
//! - Hosts are stored without square brackets; `HostKind` decides bracket wrapping and
//!   whether normalization lowercases or percent-decodes them.

mod mutate;
mod normalize;
mod parse;

pub use mutate::{with_component, with_port};
pub use parse::parse;

/// A parsed RFC 3986 URI reference.
#[derive(Clone, Debug)]
pub struct RfcUri {
    /// Raw scheme, without the trailing colon.
    pub(crate) scheme: Option<String>,
    /// Authority when the reference has one (`//...`).
    pub(crate) authority: Option<Authority>,
    /// Raw path, including any leading slash.
    pub(crate) path: String,
    /// Raw query, without the leading `?`.
    pub(crate) query: Option<String>,
    /// Raw fragment, without the leading `#`.
    pub(crate) fragment: Option<String>,
}

/// The authority component of an RFC 3986 URI.
#[derive(Clone, Debug)]
pub(crate) struct Authority {
    /// Raw userinfo, without the trailing `@`.
    pub(crate) userinfo: Option<String>,
    /// Host text without square brackets.
    pub(crate) host: Host,
    /// Raw port digits; `Some("")` is the empty port after a colon.
    pub(crate) port: Option<String>,
}

/// A host and the grammar production it was parsed as.
#[derive(Clone, Debug)]
pub(crate) struct Host {
    /// Host text without square brackets; case preserved.
    pub(crate) text: String,
    /// Grammar production selected at parse time.
    pub(crate) kind: HostKind,
}

/// The host productions uriparser distinguishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostKind {
    /// Registered name, including anything that looks like an IPv4 address.
    RegName,
    /// IPv6 address enclosed in square brackets.
    Ipv6,
    /// IPvFuture address enclosed in square brackets.
    IpvFuture,
}

/// Component selectors shared with the C ABI.
pub const COMPONENT_SCHEME: i64 = 0;
/// Selects the username half of the userinfo.
pub const COMPONENT_USERNAME: i64 = 1;
/// Selects the password half of the userinfo.
pub const COMPONENT_PASSWORD: i64 = 2;
/// Selects the whole userinfo.
pub const COMPONENT_USERINFO: i64 = 3;
/// Selects the host.
pub const COMPONENT_HOST: i64 = 4;
/// Selects the path.
pub const COMPONENT_PATH: i64 = 6;
/// Selects the query.
pub const COMPONENT_QUERY: i64 = 7;
/// Selects the fragment.
pub const COMPONENT_FRAGMENT: i64 = 8;

/// Read modes shared with the C ABI.
pub const MODE_RAW: i64 = 0;
/// Normalized Unicode components; identical to ASCII for RFC 3986.
pub const MODE_NORMALIZED_UNICODE: i64 = 2;

impl RfcUri {
    /// Returns whether this reference has a scheme (and can therefore act as a base).
    pub fn is_absolute(&self) -> bool {
        self.scheme.is_some()
    }

    /// Returns one component as PHP sees it, or `None` for an absent component.
    pub fn component(&self, component: i64, mode: i64) -> Option<String> {
        let normalized = mode != MODE_RAW;
        match component {
            COMPONENT_SCHEME => self.scheme.as_ref().map(|value| {
                if normalized {
                    value.to_ascii_lowercase()
                } else {
                    value.clone()
                }
            }),
            COMPONENT_USERNAME => {
                let userinfo = self.userinfo(normalized)?;
                Some(split_username(&userinfo).to_string())
            }
            COMPONENT_PASSWORD => {
                let userinfo = self.userinfo(normalized)?;
                Some(split_password(&userinfo).to_string())
            }
            COMPONENT_USERINFO => self.userinfo(normalized),
            COMPONENT_HOST => self.host_value(normalized),
            COMPONENT_PATH => Some(if normalized {
                normalize::normalize_path(self)
            } else {
                self.path.clone()
            }),
            COMPONENT_QUERY => self
                .query
                .as_ref()
                .map(|value| normalize::normalize_component(value, normalized)),
            COMPONENT_FRAGMENT => self
                .fragment
                .as_ref()
                .map(|value| normalize::normalize_component(value, normalized)),
            _ => None,
        }
    }

    /// Returns the number of path segments PHP's uriparser retains for this URI.
    ///
    /// PHP materializes one parser node per segment, so this count converts to the PHP-side
    /// memory the URI object owns. The prelude uses it to charge the object's footprint.
    pub fn charge_units(&self) -> i64 {
        let segments = self.path.split('/').count() as i64;
        let non_empty = self.path.split('/').filter(|part| !part.is_empty()).count() as i64;
        // One parser node is 32 bytes plus its copied segment text; the empty segments of a
        // slash-only path share the node overhead without a text copy. Each array cell is
        // eight bytes, so four cells cover the node and two cover a non-empty segment.
        4 * segments + 2 * non_empty
    }

    /// Returns the port as a PHP integer, or `None` for an absent or empty port.
    pub fn port_value(&self) -> Option<i64> {
        let text = self.authority.as_ref()?.port.as_ref()?;
        if text.is_empty() {
            return None;
        }
        parse::port_to_i64(text).ok()
    }

    /// Returns the raw or normalized userinfo.
    fn userinfo(&self, normalized: bool) -> Option<String> {
        self.authority
            .as_ref()?
            .userinfo
            .as_ref()
            .map(|value| normalize::normalize_component(value, normalized))
    }

    /// Returns the raw or normalized host, brackets included where required.
    fn host_value(&self, normalized: bool) -> Option<String> {
        let host = &self.authority.as_ref()?.host;
        let text = if normalized {
            normalize::normalize_host(host)
        } else {
            host.text.clone()
        };
        Some(match host.kind {
            HostKind::Ipv6 | HostKind::IpvFuture => format!("[{text}]"),
            HostKind::RegName => text,
        })
    }

    /// Recomposes the URI in the requested mode, optionally dropping the fragment.
    pub fn to_string(&self, normalized: bool, exclude_fragment: bool) -> String {
        let mut out = String::new();
        if let Some(scheme) = &self.scheme {
            if normalized {
                out.push_str(&scheme.to_ascii_lowercase());
            } else {
                out.push_str(scheme);
            }
            out.push(':');
        }
        if let Some(authority) = &self.authority {
            out.push_str("//");
            if let Some(userinfo) = &authority.userinfo {
                out.push_str(&normalize::normalize_component(userinfo, normalized));
                out.push('@');
            }
            match authority.host.kind {
                HostKind::Ipv6 => {
                    out.push('[');
                    out.push_str(&normalize::serialize_ipv6(&authority.host.text));
                    out.push(']');
                }
                HostKind::IpvFuture => {
                    out.push('[');
                    if normalized {
                        out.push_str(&authority.host.text.to_ascii_lowercase());
                    } else {
                        out.push_str(&authority.host.text);
                    }
                    out.push(']');
                }
                HostKind::RegName => {
                    let host = normalize::normalize_component(&authority.host.text, normalized);
                    if normalized {
                        out.push_str(&normalize::lowercase_except_percent(&host));
                    } else {
                        out.push_str(&host);
                    }
                }
            }
            if let Some(port) = &authority.port {
                out.push(':');
                out.push_str(&normalize::normalize_port(port, normalized));
            }
        }
        if normalized {
            out.push_str(&normalize::normalize_path(self));
        } else {
            out.push_str(&self.path);
        }
        if let Some(query) = &self.query {
            out.push('?');
            out.push_str(&normalize::normalize_component(query, normalized));
        }
        if !exclude_fragment {
            if let Some(fragment) = &self.fragment {
                out.push('#');
                out.push_str(&normalize::normalize_component(fragment, normalized));
            }
        }
        out
    }

    /// Returns whether two references are equivalent under normalized comparison.
    pub fn equals(&self, other: &RfcUri, include_fragment: bool) -> bool {
        self.to_string(true, !include_fragment) == other.to_string(true, !include_fragment)
    }

    /// Resolves `reference` against this URI as the base, per RFC 3986 section 5.2.
    pub fn resolve(&self, reference: &RfcUri) -> Result<RfcUri, String> {
        if !self.is_absolute() {
            return Err("The specified base URI must be absolute".to_string());
        }
        if reference.scheme.is_some() {
            return Ok(reference.clone());
        }

        let mut target = RfcUri {
            scheme: self.scheme.clone(),
            authority: self.authority.clone(),
            path: String::new(),
            query: None,
            fragment: reference.fragment.clone(),
        };

        if reference.authority.is_some() {
            target.authority = reference.authority.clone();
            target.path = resolve_path(&reference.path, target.authority.is_some());
            target.query = reference.query.clone();
            return Ok(target);
        }

        if reference.path.is_empty() {
            target.path = self.path.clone();
            target.query = reference.query.clone().or_else(|| self.query.clone());
            return Ok(target);
        }

        if reference.path.starts_with('/') {
            target.path = resolve_path(&reference.path, target.authority.is_some());
        } else {
            let merged = if self.authority.is_some() && self.path.is_empty() {
                format!("/{}", reference.path)
            } else {
                match self.path.rfind('/') {
                    Some(index) => format!("{}{}", &self.path[..=index], reference.path),
                    None => reference.path.clone(),
                }
            };
            target.path = resolve_path(&merged, target.authority.is_some());
        }
        target.query = reference.query.clone();
        Ok(target)
    }
}

/// Applies RFC 3986 dot-segment removal to a resolved absolute path.
fn resolve_path(path: &str, has_host: bool) -> String {
    normalize::remove_dot_segments(path, path.starts_with('/') && !has_host, false, has_host)
}

/// Splits a userinfo value at the first colon and returns its username half.
fn split_username(userinfo: &str) -> &str {
    match userinfo.find(':') {
        Some(index) => &userinfo[..index],
        None => userinfo,
    }
}

/// Splits a userinfo value at the first colon and returns its password half.
fn split_password(userinfo: &str) -> &str {
    match userinfo.find(':') {
        Some(index) => &userinfo[index + 1..],
        None => "",
    }
}
