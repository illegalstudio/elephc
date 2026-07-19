//! Purpose:
//! PHP `session.use_trans_sid` URL rewriting for the `--web` prefork server.
//! Decides whether a response needs SID propagation, then appends the session
//! id to same-origin URLs in the HTML body and the `Location` header, and
//! injects a hidden SID field into `<form>` tags — mirroring PHP's own
//! `url_rewriter` output filter.
//!
//! Called from:
//! - `crate::worker`'s response-assembly path, once per request, between the
//!   handler run and the gzip decision (so rewriting happens on plaintext).
//!
//! Key details:
//! - Rewriting is fully gated by session config (`use_trans_sid=1`,
//!   `use_only_cookies=0`, an active session with a non-empty id) AND the
//!   absence of the session cookie on the request, so the default configuration
//!   and every cookie-carrying request pay nothing.
//! - Only same-origin URLs are rewritten (relative, protocol-relative to the
//!   request host, or absolute to a `trans_sid_hosts`/request-host match); this
//!   matches PHP and avoids leaking the SID to third-party hosts.
//! - The scanner is a focused tag/attribute walker, not a full HTML parser:
//!   when a construct is malformed or ambiguous it leaves the bytes untouched.

use std::os::raw::c_char;

/// Resolved activation state for one response: the session name/id plus the
/// parsed `trans_sid_tags` (tag→attr pairs; an empty attr means "inject a hidden
/// field") and `trans_sid_hosts` allow-list.
struct Activation {
    /// Session/cookie name (e.g. `PHPSESSID`) used as the SID query key.
    name: String,
    /// Current session id appended as the SID value.
    id: String,
    /// Parsed `tag=attr` pairs; an empty `attr` marks a form-injection tag.
    tags: Vec<(String, String)>,
    /// Extra hosts (besides the request host) whose absolute URLs may be rewritten.
    hosts: Vec<String>,
}

/// Reads a C string getter result into an owned `String`, treating NULL as empty.
///
/// Safety: `p` must be a valid NUL-terminated C string pointer or NULL, as
/// returned by the session config getters.
unsafe fn read_cstr(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
}

/// Computes whether URL rewriting is active for the current response, returning
/// the resolved session name/id and parsed tag/host config, or `None` when any
/// activation condition fails (the common case, so the caller returns early).
///
/// Conditions: `use_trans_sid == 1`, `use_only_cookies == 0`, a session was
/// established this request (non-empty id and name), and the request did not
/// already carry the session cookie (nothing to propagate).
///
/// Note on status: the web prelude auto-runs `session_write_close()` in its
/// per-request `finally`, which sets `session_status()` back to `PHP_SESSION_NONE`
/// (1) before this response hook runs — so a live `== PHP_SESSION_ACTIVE` check
/// would never pass here. The prelude instead resets all session state (including
/// the id) at the *start* of each request, so a non-empty id at this point
/// reliably means a session was started during this request — and that retained
/// id is what we use as the "a session was active" signal.
fn activation(req_cookie: Option<&str>) -> Option<Activation> {
    unsafe {
        if crate::session::elephc_web_session_get_use_trans_sid() != 1 {
            return None;
        }
        if crate::session::elephc_web_session_get_use_only_cookies() != 0 {
            return None;
        }
        // A session must have run this request. The id is the reliable signal:
        // the prelude clears it at request start and only session_start() sets it,
        // so a non-empty id here means an active session this request (even though
        // its status has since been flipped to NONE by the finally's write-close).
        let id = read_cstr(crate::session::elephc_web_session_get_id());
        if id.is_empty() {
            return None;
        }
        let name = read_cstr(crate::session::elephc_web_session_get_name());
        if name.is_empty() {
            return None;
        }
        // If the client already sent the session cookie, URL propagation is
        // unnecessary — the id round-trips via the cookie.
        if crate::session::elephc_web_session_get_use_cookies() == 1 {
            if let Some(cookie) = req_cookie {
                if cookie_contains_name(cookie, &name) {
                    return None;
                }
            }
        }
        let tags = parse_tags(&read_cstr(crate::session::elephc_web_session_get_trans_sid_tags()));
        let hosts = parse_hosts(&read_cstr(crate::session::elephc_web_session_get_trans_sid_hosts()));
        Some(Activation { name, id, tags, hosts })
    }
}

/// Rewrites the response body and `Location` header to propagate the session id
/// when `session.use_trans_sid` is active, returning the (possibly rewritten)
/// body. When inactive the body is returned unchanged with zero allocation.
///
/// `req_cookie`/`req_host` are the incoming request's `Cookie`/`Host` header
/// values. `resp_headers` is the taken response header list, mutated in place to
/// rewrite a same-origin `Location` and to drop a now-stale explicit
/// `Content-Length` when the body length changes (hyper recomputes it).
pub fn maybe_rewrite_response(
    req_cookie: Option<&str>,
    req_host: Option<&str>,
    resp_headers: &mut Vec<(String, String)>,
    body: Vec<u8>,
) -> Vec<u8> {
    let act = match activation(req_cookie) {
        Some(a) => a,
        None => return body,
    };
    // Same-origin redirects must carry the SID too.
    for (n, v) in resp_headers.iter_mut() {
        if n.eq_ignore_ascii_case("location") {
            if let Some(new) = rewrite_url(v, &act.name, &act.id, &act.hosts, req_host) {
                *v = new;
            }
        }
    }
    // Only HTML bodies are rewritten; a missing Content-Type is treated as HTML.
    if !content_type_is_html(resp_headers) {
        return body;
    }
    // Non-UTF-8 bodies are left byte-identical rather than risk corrupting them.
    let text = match String::from_utf8(body) {
        Ok(s) => s,
        Err(e) => return e.into_bytes(),
    };
    match rewrite_html(&text, &act.name, &act.id, &act.tags, &act.hosts, req_host) {
        Some(new_body) => {
            // The body length changed: drop any explicit Content-Length so the
            // hyper `Full<Bytes>` body recomputes the correct length.
            resp_headers.retain(|(n, _)| !n.eq_ignore_ascii_case("content-length"));
            new_body.into_bytes()
        }
        None => text.into_bytes(),
    }
}

/// Returns true when a `Cookie` header value carries a cookie named `name`
/// (i.e. the client already holds the session id, so no URL propagation needed).
fn cookie_contains_name(cookie: &str, name: &str) -> bool {
    cookie.split(';').any(|pair| {
        let key = pair.trim().split('=').next().unwrap_or("");
        key == name
    })
}

/// Parses a `trans_sid_tags` string (`"a=href,area=href,form="`) into lowercased
/// `(tag, attr)` pairs. An entry with no `=` or an empty attribute yields an
/// empty `attr`, marking the tag for hidden-field injection (`form`).
fn parse_tags(s: &str) -> Vec<(String, String)> {
    s.split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            let (tag, attr) = match entry.split_once('=') {
                Some((t, a)) => (t, a),
                None => (entry, ""),
            };
            let tag = tag.trim().to_ascii_lowercase();
            if tag.is_empty() {
                return None;
            }
            Some((tag, attr.trim().to_ascii_lowercase()))
        })
        .collect()
}

/// Parses a comma-separated `trans_sid_hosts` string into a list of non-empty,
/// trimmed host entries.
fn parse_hosts(s: &str) -> Vec<String> {
    s.split(',')
        .map(|h| h.trim())
        .filter(|h| !h.is_empty())
        .map(|h| h.to_string())
        .collect()
}

/// Returns true when the response should be treated as HTML for rewriting: the
/// `Content-Type` contains `text/html`, or there is no `Content-Type` header.
fn content_type_is_html(headers: &[(String, String)]) -> bool {
    match headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("content-type")) {
        None => true,
        Some((_, v)) => v.to_ascii_lowercase().contains("text/html"),
    }
}

/// HTML-escapes a string for use inside a double-quoted attribute value, so the
/// injected hidden field cannot break out of its attribute even for exotic
/// session names/ids.
fn html_attr_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// An opening HTML tag located by the scanner: its lowercased name and the byte
/// index just past its closing `>`.
struct OpenTag {
    /// Lowercased tag name (e.g. `a`, `form`).
    name: String,
    /// Byte index in the source just after the tag's `>`.
    end: usize,
}

/// Attempts to parse an opening tag starting at the `<` byte at `start`.
///
/// Returns `None` for closing tags (`</…`), comments/declarations (`<!…`),
/// processing instructions (`<?…`), a `<` that is not followed by a letter, or a
/// tag with no closing `>` (partial/malformed markup left untouched). Quoted
/// attribute values are respected so a `>` inside quotes does not end the tag.
fn find_open_tag(text: &str, start: usize) -> Option<OpenTag> {
    let b = text.as_bytes();
    let mut i = start + 1;
    if i >= b.len() {
        return None;
    }
    if !b[i].is_ascii_alphabetic() {
        return None;
    }
    let name_start = i;
    while i < b.len() && b[i].is_ascii_alphanumeric() {
        i += 1;
    }
    let name = text[name_start..i].to_ascii_lowercase();
    let mut quote = 0u8;
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return Some(OpenTag { name, end: i + 1 });
        }
        i += 1;
    }
    None
}

/// Scans `body` for the configured tags and rewrites their URL attributes (and
/// injects hidden SID fields into `<form>`s), returning the rewritten HTML, or
/// `None` if nothing changed (so the caller can avoid touching Content-Length).
fn rewrite_html(
    body: &str,
    name: &str,
    id: &str,
    tags: &[(String, String)],
    hosts: &[String],
    req_host: Option<&str>,
) -> Option<String> {
    let mut out = String::with_capacity(body.len() + 64);
    let mut pos = 0;
    let mut changed = false;
    while let Some(rel) = body[pos..].find('<') {
        let lt = pos + rel;
        out.push_str(&body[pos..lt]);
        match find_open_tag(body, lt) {
            Some(tag) => {
                let mut tag_text = body[lt..tag.end].to_string();
                let mut inject_form = false;
                for (t, a) in tags {
                    if *t != tag.name {
                        continue;
                    }
                    if a.is_empty() {
                        // Empty attr = hidden-field injection; only meaningful for forms.
                        if tag.name == "form" {
                            inject_form = true;
                        }
                    } else if let Some(new_tag) =
                        rewrite_attr(&tag_text, a, name, id, hosts, req_host)
                    {
                        tag_text = new_tag;
                        changed = true;
                    }
                }
                out.push_str(&tag_text);
                if inject_form {
                    out.push_str(&format!(
                        "<input type=\"hidden\" name=\"{}\" value=\"{}\" />",
                        html_attr_escape(name),
                        html_attr_escape(id)
                    ));
                    changed = true;
                }
                pos = tag.end;
            }
            None => {
                // Not a real opening tag: emit the '<' literally and continue.
                out.push('<');
                pos = lt + 1;
            }
        }
    }
    out.push_str(&body[pos..]);
    if changed {
        Some(out)
    } else {
        None
    }
}

/// Rewrites the first `attr` URL value inside a single opening tag string,
/// returning the modified tag or `None` if the attribute is absent or its URL is
/// not rewritten (external host, fragment, `mailto:`, already carries the SID…).
///
/// The attribute must be a real attribute (preceded by whitespace, followed by
/// optional whitespace then `=`); values may be double/single quoted or
/// unquoted. Byte scanning uses only ASCII delimiters, so UTF-8 inside a URL
/// value is preserved.
fn rewrite_attr(
    tag: &str,
    attr: &str,
    name: &str,
    id: &str,
    hosts: &[String],
    req_host: Option<&str>,
) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let al = attr.to_ascii_lowercase();
    let b = tag.as_bytes();
    let len = tag.len();
    let mut search = 0;
    while let Some(rel) = lower[search..].find(&al) {
        let idx = search + rel;
        search = idx + al.len();
        // Left boundary: the attribute name must follow whitespace.
        if idx == 0 || !b[idx - 1].is_ascii_whitespace() {
            continue;
        }
        // After the name: optional whitespace then '='.
        let mut j = idx + al.len();
        while j < len && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= len || b[j] != b'=' {
            continue;
        }
        j += 1;
        while j < len && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= len {
            return None;
        }
        let (vstart, vend) = if b[j] == b'"' || b[j] == b'\'' {
            let q = b[j];
            let vs = j + 1;
            let mut k = vs;
            while k < len && b[k] != q {
                k += 1;
            }
            if k >= len {
                return None; // unterminated quote: leave the tag untouched
            }
            (vs, k)
        } else {
            let vs = j;
            let mut k = vs;
            while k < len && !b[k].is_ascii_whitespace() && b[k] != b'>' && b[k] != b'/' {
                k += 1;
            }
            (vs, k)
        };
        let value = &tag[vstart..vend];
        if let Some(new_url) = rewrite_url(value, name, id, hosts, req_host) {
            let mut res = String::with_capacity(tag.len() + new_url.len());
            res.push_str(&tag[..vstart]);
            res.push_str(&new_url);
            res.push_str(&tag[vend..]);
            return Some(res);
        }
        return None;
    }
    None
}

/// Appends `name=id` to a same-origin URL, returning the rewritten URL or `None`
/// when the URL must be left alone: empty, fragment-only, already carrying the
/// SID param, or pointing off-origin / at a non-hierarchical scheme
/// (`mailto:`, `javascript:`, …).
///
/// The SID is inserted before any `#fragment`, using `?` when the URL has no
/// query or `&` otherwise.
fn rewrite_url(
    url: &str,
    name: &str,
    id: &str,
    hosts: &[String],
    req_host: Option<&str>,
) -> Option<String> {
    if url.is_empty() || url.starts_with('#') {
        return None;
    }
    let (main, frag) = match url.find('#') {
        Some(h) => (&url[..h], &url[h..]),
        None => (url, ""),
    };
    if has_query_param(main, name) {
        return None;
    }
    if !is_same_origin(main, hosts, req_host) {
        return None;
    }
    let sep = if main.contains('?') { '&' } else { '?' };
    Some(format!("{main}{sep}{name}={id}{frag}"))
}

/// Returns true when `main` (a URL minus any fragment) already has a query
/// parameter whose key equals `name`.
fn has_query_param(main: &str, name: &str) -> bool {
    let query = match main.split_once('?') {
        Some((_, q)) => q,
        None => return false,
    };
    query
        .split('&')
        .any(|pair| pair.split('=').next().map_or(false, |k| k == name))
}

/// Decides whether `main` is same-origin and therefore eligible for rewriting.
///
/// Relative URLs are same-origin; protocol-relative (`//host/…`) and absolute
/// (`scheme://host/…`) URLs are same-origin only when their host matches the
/// allow-list or request host; non-hierarchical schemes (`mailto:`,
/// `javascript:`, `tel:`, `data:`) are never same-origin.
fn is_same_origin(main: &str, hosts: &[String], req_host: Option<&str>) -> bool {
    // Protocol-relative: //host/path
    if let Some(rest) = main.strip_prefix("//") {
        let host = rest.split(|c| c == '/' || c == '?').next().unwrap_or("");
        return host_matches(host, hosts, req_host);
    }
    // Absolute: scheme://host/path
    if let Some(pos) = main.find("://") {
        let scheme = &main[..pos];
        if !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        {
            let after = &main[pos + 3..];
            let host = after.split(|c| c == '/' || c == '?').next().unwrap_or("");
            return host_matches(host, hosts, req_host);
        }
    }
    // Non-hierarchical scheme (mailto:, javascript:, tel:, data:): a ':' appears
    // before any '/' or '?' and the prefix looks like a URL scheme.
    if let Some(colon) = main.find(':') {
        let before = &main[..colon];
        let has_path = before.contains('/') || before.contains('?');
        if !has_path
            && before.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && before
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        {
            return false;
        }
    }
    // Otherwise relative → same origin.
    true
}

/// Returns true when an absolute URL `host` (optionally with a `:port`) is
/// allowed: it matches an entry in `hosts` when that list is non-empty, else it
/// matches the request `Host`. Ports are ignored on both sides for the
/// host-key comparison.
fn host_matches(host: &str, hosts: &[String], req_host: Option<&str>) -> bool {
    if host.is_empty() {
        return true;
    }
    let hk = host.split(':').next().unwrap_or(host);
    if !hosts.is_empty() {
        return hosts.iter().any(|h| {
            let ek = h.split(':').next().unwrap_or(h);
            ek.eq_ignore_ascii_case(hk) || h.eq_ignore_ascii_case(host)
        });
    }
    match req_host {
        Some(rh) => {
            let rk = rh.split(':').next().unwrap_or(rh);
            rk.eq_ignore_ascii_case(hk)
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a tag config vector from `(tag, attr)` string pairs for tests.
    fn tags(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(t, a)| (t.to_string(), a.to_string()))
            .collect()
    }

    /// A relative `<a href>` gains the SID as a new `?` query parameter.
    #[test]
    fn rewrites_relative_anchor() {
        let t = tags(&[("a", "href")]);
        let out = rewrite_html(
            "<a href=\"/x\">n</a>",
            "PHPSESSID",
            "ID",
            &t,
            &[],
            Some("example.com"),
        )
        .unwrap();
        assert_eq!(out, "<a href=\"/x?PHPSESSID=ID\">n</a>");
    }

    /// An existing query string gets the SID appended with `&`.
    #[test]
    fn appends_to_existing_query() {
        let t = tags(&[("a", "href")]);
        let out = rewrite_html(
            "<a href=\"/x?a=1\">n</a>",
            "PHPSESSID",
            "ID",
            &t,
            &[],
            Some("example.com"),
        )
        .unwrap();
        assert_eq!(out, "<a href=\"/x?a=1&PHPSESSID=ID\">n</a>");
    }

    /// A `<form>` gets a hidden SID input injected right after the opening tag.
    #[test]
    fn injects_hidden_form_field() {
        let t = tags(&[("form", "")]);
        let out = rewrite_html(
            "<form action=\"/p\"></form>",
            "PHPSESSID",
            "ID",
            &t,
            &[],
            Some("example.com"),
        )
        .unwrap();
        assert_eq!(
            out,
            "<form action=\"/p\"><input type=\"hidden\" name=\"PHPSESSID\" value=\"ID\" /></form>"
        );
    }

    /// An off-host absolute URL is never rewritten (SID must not leak off-origin).
    #[test]
    fn leaves_off_host_absolute_untouched() {
        let t = tags(&[("a", "href")]);
        let out = rewrite_html(
            "<a href=\"https://evil.com/\">x</a>",
            "PHPSESSID",
            "ID",
            &t,
            &[],
            Some("example.com"),
        );
        assert!(out.is_none(), "off-host URL should be left untouched");
    }

    /// Fragment-only and `mailto:` links are left untouched.
    #[test]
    fn leaves_fragment_and_mailto_untouched() {
        let t = tags(&[("a", "href")]);
        assert!(rewrite_html(
            "<a href=\"#top\">x</a>",
            "PHPSESSID",
            "ID",
            &t,
            &[],
            Some("example.com")
        )
        .is_none());
        assert!(rewrite_html(
            "<a href=\"mailto:a@b.com\">x</a>",
            "PHPSESSID",
            "ID",
            &t,
            &[],
            Some("example.com")
        )
        .is_none());
    }

    /// A URL already carrying the SID param is not rewritten a second time.
    #[test]
    fn leaves_url_with_existing_sid_untouched() {
        let t = tags(&[("a", "href")]);
        let out = rewrite_html(
            "<a href=\"/x?PHPSESSID=abc\">x</a>",
            "PHPSESSID",
            "ID",
            &t,
            &[],
            Some("example.com"),
        );
        assert!(out.is_none(), "URL with existing SID should be untouched");
    }

    /// The SID is inserted before a `#fragment`, preserving the fragment.
    #[test]
    fn inserts_sid_before_fragment() {
        let out = rewrite_url("/x#section", "PHPSESSID", "ID", &[], Some("example.com")).unwrap();
        assert_eq!(out, "/x?PHPSESSID=ID#section");
    }

    /// An absolute URL to the request host is same-origin and gets the SID.
    #[test]
    fn rewrites_absolute_to_request_host() {
        let out = rewrite_url(
            "http://example.com/p",
            "PHPSESSID",
            "ID",
            &[],
            Some("example.com"),
        )
        .unwrap();
        assert_eq!(out, "http://example.com/p?PHPSESSID=ID");
    }

    /// An absolute URL to a configured `trans_sid_hosts` entry is rewritten even
    /// when it is not the request host.
    #[test]
    fn rewrites_absolute_to_configured_host() {
        let hosts = vec!["cdn.example.com".to_string()];
        let out = rewrite_url(
            "https://cdn.example.com/a",
            "PHPSESSID",
            "ID",
            &hosts,
            Some("example.com"),
        )
        .unwrap();
        assert_eq!(out, "https://cdn.example.com/a?PHPSESSID=ID");
    }

    /// A protocol-relative URL matching the request host is rewritten.
    #[test]
    fn rewrites_protocol_relative_same_host() {
        let out = rewrite_url("//example.com/p", "PHPSESSID", "ID", &[], Some("example.com")).unwrap();
        assert_eq!(out, "//example.com/p?PHPSESSID=ID");
    }

    /// Host matching ignores the port on both the URL and the request Host.
    #[test]
    fn host_match_ignores_port() {
        let out = rewrite_url(
            "http://127.0.0.1:8080/p",
            "PHPSESSID",
            "ID",
            &[],
            Some("127.0.0.1:8080"),
        )
        .unwrap();
        assert_eq!(out, "http://127.0.0.1:8080/p?PHPSESSID=ID");
    }

    /// A single-quoted attribute value is rewritten in place.
    #[test]
    fn rewrites_single_quoted_attr() {
        let t = tags(&[("a", "href")]);
        let out = rewrite_html(
            "<a href='/x'>n</a>",
            "PHPSESSID",
            "ID",
            &t,
            &[],
            Some("example.com"),
        )
        .unwrap();
        assert_eq!(out, "<a href='/x?PHPSESSID=ID'>n</a>");
    }

    /// A `<` that is not a real tag (e.g. a stray comparison) is preserved and
    /// does not derail scanning of a later valid tag.
    #[test]
    fn preserves_stray_lt_and_still_rewrites() {
        let t = tags(&[("a", "href")]);
        let out = rewrite_html(
            "1 < 2 <a href=\"/x\">n</a>",
            "PHPSESSID",
            "ID",
            &t,
            &[],
            Some("example.com"),
        )
        .unwrap();
        assert_eq!(out, "1 < 2 <a href=\"/x?PHPSESSID=ID\">n</a>");
    }

    /// A cookie header carrying the session name is detected (activation would abort).
    #[test]
    fn detects_session_cookie() {
        assert!(cookie_contains_name("foo=1; PHPSESSID=abc; bar=2", "PHPSESSID"));
        assert!(!cookie_contains_name("foo=1; bar=2", "PHPSESSID"));
    }

    /// The default tag config parses into the expected tag/attr pairs, with the
    /// trailing `form=` yielding an empty (injection) attr.
    #[test]
    fn parses_default_tags() {
        let t = parse_tags("a=href,area=href,frame=src,form=");
        assert_eq!(
            t,
            vec![
                ("a".to_string(), "href".to_string()),
                ("area".to_string(), "href".to_string()),
                ("frame".to_string(), "src".to_string()),
                ("form".to_string(), String::new()),
            ]
        );
    }
}
