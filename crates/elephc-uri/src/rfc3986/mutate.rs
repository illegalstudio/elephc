//! Purpose:
//! Component mutation for `Uri\Rfc3986\Uri::with*()`: validates each value with the same
//! grammar the parser uses, applies uriparser's cross-component constraints, and returns a
//! new reference without touching the receiver.
//!
//! Called from:
//! - `crate::rfc3986::with_component` / `with_port` and therefore the C ABI.
//!
//! Key details:
//! - Removing a host is refused while a port or userinfo remains, matching uriparser's
//!   `uriSetHostAuto` diagnostics.
//! - Path disambiguation inserts `/.` after a leading `//` and prefixes `./` before a
//!   colon in the first segment of a scheme-less relative path, so the result re-parses as
//!   the same reference.

use super::parse::{
    is_valid_ipvfuture, is_valid_path_for_write, is_valid_query_or_fragment, is_valid_reg_name,
    is_valid_scheme, is_valid_userinfo,
};
use super::{
    Authority, Host, HostKind, RfcUri, COMPONENT_FRAGMENT, COMPONENT_HOST, COMPONENT_PASSWORD,
    COMPONENT_PATH, COMPONENT_QUERY, COMPONENT_SCHEME, COMPONENT_USERINFO, COMPONENT_USERNAME,
};

/// Writes one string-or-null component into a clone of `uri`.
pub fn with_component(
    uri: &RfcUri,
    component: i64,
    value: Option<&str>,
) -> Result<RfcUri, String> {
    let mut next = uri.clone();
    match component {
        COMPONENT_SCHEME => write_scheme(&mut next, value)?,
        COMPONENT_USERINFO => write_userinfo(&mut next, value)?,
        COMPONENT_USERNAME => {
            let userinfo = next
                .authority
                .as_ref()
                .and_then(|authority| authority.userinfo.clone());
            let username = value.unwrap_or("");
            let password = userinfo
                .as_deref()
                .map(super::split_password)
                .unwrap_or("");
            write_userinfo(
                &mut next,
                Some(&format!("{username}:{password}")),
            )?;
        }
        COMPONENT_PASSWORD => {
            let userinfo = next
                .authority
                .as_ref()
                .and_then(|authority| authority.userinfo.clone());
            let username = userinfo
                .as_deref()
                .map(super::split_username)
                .unwrap_or("");
            let password = value.unwrap_or("");
            write_userinfo(
                &mut next,
                Some(&format!("{username}:{password}")),
            )?;
        }
        COMPONENT_HOST => write_host(&mut next, value)?,
        COMPONENT_PATH => write_path(&mut next, value.unwrap_or(""))?,
        COMPONENT_QUERY => {
            if let Some(value) = value {
                if !is_valid_query_or_fragment(value) {
                    return Err("The specified query is malformed".to_string());
                }
                next.query = Some(value.to_string());
            } else {
                next.query = None;
            }
        }
        COMPONENT_FRAGMENT => {
            if let Some(value) = value {
                if !is_valid_query_or_fragment(value) {
                    return Err("The specified fragment is malformed".to_string());
                }
                next.fragment = Some(value.to_string());
            } else {
                next.fragment = None;
            }
        }
        _ => return Err("Failed to update the URI".to_string()),
    }
    if matches!(
        component,
        COMPONENT_SCHEME | COMPONENT_HOST | COMPONENT_PATH
    ) {
        disambiguate_path(&mut next);
    }
    Ok(next)
}

/// Writes the port into a clone of `uri`.
pub fn with_port(uri: &RfcUri, port: Option<i64>) -> Result<RfcUri, String> {
    let mut next = uri.clone();
    match port {
        None => {
            if let Some(authority) = &mut next.authority {
                authority.port = None;
            }
        }
        Some(port) => {
            if port < 0 {
                return Err("The specified port is malformed".to_string());
            }
            let Some(authority) = &mut next.authority else {
                return Err("Cannot set a port without having a host".to_string());
            };
            authority.port = Some(port.to_string());
        }
    }
    Ok(next)
}

/// Keeps a written path parseable as the same reference after a scheme or host change.
///
/// uriparser applies the same two repairs: a `//` prefix would become an authority, and a
/// colon in the first segment of a scheme-less path would become a scheme.
fn disambiguate_path(uri: &mut RfcUri) {
    if uri.authority.is_some() {
        return;
    }
    if let Some(rest) = uri.path.strip_prefix("//") {
        uri.path = format!("/.//{rest}");
        return;
    }
    if !uri.path.starts_with('/')
        && uri
            .path
            .split('/')
            .next()
            .is_some_and(|segment| segment.contains(':'))
    {
        uri.path = format!("./{}", uri.path);
    }
}

/// Writes the scheme component.
fn write_scheme(uri: &mut RfcUri, value: Option<&str>) -> Result<(), String> {
    match value {
        None => {
            uri.scheme = None;
            Ok(())
        }
        Some(value) if is_valid_scheme(value) => {
            uri.scheme = Some(value.to_string());
            Ok(())
        }
        Some(_) => Err("The specified scheme is malformed".to_string()),
    }
}

/// Writes the whole userinfo component.
fn write_userinfo(uri: &mut RfcUri, value: Option<&str>) -> Result<(), String> {
    match value {
        None => {
            if let Some(authority) = &mut uri.authority {
                authority.userinfo = None;
            }
            Ok(())
        }
        Some(value) => {
            let Some(authority) = &mut uri.authority else {
                return Err("Cannot set a userinfo without having a host".to_string());
            };
            if !is_valid_userinfo(value) {
                return Err("The specified userinfo is malformed".to_string());
            }
            authority.userinfo = Some(value.to_string());
            Ok(())
        }
    }
}

/// Writes the host component, creating or removing the authority as uriparser does.
fn write_host(uri: &mut RfcUri, value: Option<&str>) -> Result<(), String> {
    match value {
        None => {
            let Some(authority) = &uri.authority else {
                return Ok(());
            };
            if authority.port.is_some() {
                return Err("Cannot remove the host from a URI that has a port".to_string());
            }
            if authority.userinfo.is_some() {
                return Err("Cannot remove the host from a URI that has a userinfo".to_string());
            }
            uri.authority = None;
            Ok(())
        }
        Some(value) => {
            let host = parse_host_value(value)?;
            match &mut uri.authority {
                Some(authority) => authority.host = host,
                None => {
                    uri.authority = Some(Authority {
                        userinfo: None,
                        host,
                        port: None,
                    });
                    if !uri.path.is_empty() && !uri.path.starts_with('/') {
                        uri.path = format!("/{}", uri.path);
                    }
                }
            }
            Ok(())
        }
    }
}

/// Parses a host supplied to `withHost`, accepting bracketed IPv6/IPvFuture literals.
fn parse_host_value(value: &str) -> Result<Host, String> {
    if let Some(rest) = value.strip_prefix('[') {
        let Some(close) = rest.find(']') else {
            return Err("The specified host is malformed".to_string());
        };
        if close + 1 != rest.len() {
            return Err("The specified host is malformed".to_string());
        }
        let inside = &rest[..close];
        let kind = if inside.starts_with('v') || inside.starts_with('V') {
            if !is_valid_ipvfuture(inside) {
                return Err("The specified host is malformed".to_string());
            }
            HostKind::IpvFuture
        } else {
            if inside.parse::<std::net::Ipv6Addr>().is_err() {
                return Err("The specified host is malformed".to_string());
            }
            HostKind::Ipv6
        };
        return Ok(Host {
            text: inside.to_string(),
            kind,
        });
    }
    if !is_valid_reg_name(value) {
        return Err("The specified host is malformed".to_string());
    }
    Ok(Host {
        text: value.to_string(),
        kind: HostKind::RegName,
    })
}

/// Writes the path component with uriparser's re-parse disambiguation.
fn write_path(uri: &mut RfcUri, value: &str) -> Result<(), String> {
    if !is_valid_path_for_write(value, uri.authority.is_some()) {
        return Err("The specified path is malformed".to_string());
    }
    let mut path = value.to_string();
    if uri.authority.is_none() {
        if let Some(rest) = path.strip_prefix("//") {
            path = format!("/.//{rest}");
        } else if uri.scheme.is_none()
            && !path.starts_with('/')
            && path
                .split('/')
                .next()
                .is_some_and(|segment| segment.contains(':'))
        {
            path = format!("./{path}");
        }
    }
    uri.path = path;
    Ok(())
}
