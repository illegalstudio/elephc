//! Purpose:
//! DOM element attribute operations on the shared libxml2 tree: legacy (DOM level 1) and
//! namespace-aware lookup, namespace-aware writes, removal and a full attribute listing.
//!
//! Called from:
//! - The DOM prelude's `DOMElement` members (package P5), through `elephc_xml_sx_dom_element`.
//!
//! Key details:
//! - Only libxml2's public API and the shim accessors of `ffi` are used. A removed attribute is
//!   freed by `xmlRemoveProp`: DOM attribute objects are snapshots in the prelude, so nothing
//!   else holds the native pointer.
//! - Every answer starts with `1` (present / done) or is empty (absent / refused), so a present
//!   empty value is told from a missing attribute. The listing is hex encoded.
//! - Namespace declarations are not attributes: they are listed only on request, as in php's
//!   `getAttributeNames()`.

use super::*;

extern "C" {
    #[link_name = "xmlHasNsProp"]
    fn has_ns_prop(node: Node, name: *const c_char, href: *const c_char) -> Node;
    #[link_name = "xmlSearchNs"]
    fn search_ns(doc: Node, node: Node, prefix: *const c_char) -> Node;
    #[link_name = "xmlSearchNsByHref"]
    fn search_ns_by_href(doc: Node, node: Node, href: *const c_char) -> Node;
    #[link_name = "xmlNewNs"]
    fn new_ns(node: Node, href: *const c_char, prefix: *const c_char) -> Node;
    #[link_name = "xmlSetNsProp"]
    fn set_ns_prop(node: Node, ns: Node, name: *const c_char, value: *const c_char) -> Node;
    #[link_name = "xmlSetProp"]
    fn set_prop(node: Node, name: *const c_char, value: *const c_char) -> Node;
    #[link_name = "xmlRemoveProp"]
    fn remove_prop(attr: Node) -> std::ffi::c_int;
}

const XMLNS_URI: &[u8] = b"http://www.w3.org/2000/xmlns/";

/// What a legacy attribute lookup found.
enum Found {
    Attribute(Node),
    Declaration(Node),
}

/// Splits `prefix:local` the way `xmlSplitQName3` does: `None` unless both halves are non-empty.
fn split_qname(name: &[u8]) -> Option<(&[u8], &[u8])> {
    let colon = name.iter().position(|byte| *byte == b':')?;
    (colon > 0 && colon + 1 < name.len()).then(|| (&name[..colon], &name[colon + 1..]))
}

/// A namespace declaration of `node` with the given prefix (`None`: the default namespace).
unsafe fn declaration(node: Node, prefix: Option<&[u8]>) -> Option<Node> {
    let mut ns = ffi::ns_defs(node);
    while !ns.is_null() {
        let declared = ffi::ns_value(ns, 1);
        let same = match prefix {
            None => declared.is_null(),
            Some(wanted) => !declared.is_null() && CStr::from_ptr(declared).to_bytes() == wanted,
        };
        if same { return Some(ns); }
        ns = ffi::ns_next(ns);
    }
    None
}

/// php's `dom_get_dom1_attribute`: a qualified name resolves its prefix in scope, `xmlns` and
/// `xmlns:p` find declarations, an unprefixed name finds a no-namespace attribute.
unsafe fn lookup(doc: Node, node: Node, name: &[u8]) -> Option<Found> {
    if ffi::node_type(node) != 1 { return None; }
    if let Some((prefix, local)) = split_qname(name) {
        if prefix == b"xmlns" { return declaration(node, Some(local)).map(Found::Declaration); }
        let ns = search_ns(doc, node, cstring(prefix).as_ptr());
        if ns.is_null() { return None; }
        let found = has_ns_prop(node, cstring(local).as_ptr(), ffi::ns_value(ns, 0));
        return (!found.is_null()).then_some(Found::Attribute(found));
    }
    if name == b"xmlns" { return declaration(node, None).map(Found::Declaration); }
    let found = has_ns_prop(node, cstring(name).as_ptr(), std::ptr::null());
    (!found.is_null()).then_some(Found::Attribute(found))
}

/// The attribute (or declaration, for the xmlns namespace) with the namespace URI and local name.
unsafe fn lookup_ns(node: Node, uri: &[u8], local: &[u8]) -> Option<Found> {
    if ffi::node_type(node) != 1 { return None; }
    if uri == XMLNS_URI {
        let prefix = if local == b"xmlns" { None } else { Some(local) };
        return declaration(node, prefix).map(Found::Declaration);
    }
    let uri = (!uri.is_empty()).then(|| cstring(uri));
    let found = has_ns_prop(node, cstring(local).as_ptr(), uri.as_ref().map_or(std::ptr::null(), |text| text.as_ptr()));
    (!found.is_null() && ffi::node_type(found) == 2).then_some(Found::Attribute(found))
}

/// The text of a found attribute or declaration.
unsafe fn value_of(found: &Found) -> Vec<u8> {
    match found {
        Found::Attribute(attr) => owned_bytes(ffi::text(*attr)),
        Found::Declaration(ns) => bytes(ffi::ns_value(*ns, 0)),
    }
}

/// Declares `prefix` for `uri` on `node` unless an equal binding is in scope; returns the binding.
unsafe fn ensure_ns(doc: Node, node: Node, prefix: &[u8], uri: &[u8]) -> Node {
    let uri_c = cstring(uri);
    let prefix_c = cstring(prefix);
    let prefix_ptr = if prefix.is_empty() { std::ptr::null() } else { prefix_c.as_ptr() };
    let in_scope = search_ns(doc, node, prefix_ptr);
    if !in_scope.is_null() && bytes(ffi::ns_value(in_scope, 0)) == uri { return in_scope; }
    if !prefix.is_empty() {
        let by_href = search_ns_by_href(doc, node, uri_c.as_ptr());
        if !by_href.is_null() && bytes(ffi::ns_value(by_href, 1)) == prefix { return by_href; }
    }
    new_ns(node, uri_c.as_ptr(), prefix_ptr)
}

/// Runs one element attribute operation on the node the wrapper selects.
///
/// Operations: 0 `hasAttribute(name)`; 1 `getAttributeNS(uri, local)`; 2 `hasAttributeNS(uri,
/// local)`; 3 `setAttributeNS(uri, qualified, value)`; 4 `removeAttribute(name)`; 5
/// `removeAttributeNS(uri, local)`; 6 the attribute listing (`value` = "1" adds the namespace
/// declarations): hex of records `kind \0 qualified \0 local \0 prefix \0 uri \0 value`, records
/// separated by `\x01`, kind `a` (attribute) or `n` (declaration).
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_dom_element(handle: i64, operation: i64, uri: *const c_char, name: *const c_char, value: *const c_char) -> *const c_char {
    super::abi::guard(|| super::abi::output(super::abi::view(handle).and_then(|view| {
        let node = view.first()?;
        if ffi::node_type(node) != 1 { return None; }
        let (uri, name, value) = (bytes(uri), bytes(name), bytes(value));
        match operation {
            0 => lookup(view.doc.raw, node, &name).map(|_| b"1".to_vec()),
            1 => lookup_ns(node, &uri, &name).map(|found| [b"1".to_vec(), value_of(&found)].concat()),
            2 => lookup_ns(node, &uri, &name).map(|_| b"1".to_vec()),
            3 => {
                let (prefix, local) = match split_qname(&name) { Some((p, l)) => (p.to_vec(), l.to_vec()), None => (Vec::new(), name.clone()) };
                if uri == XMLNS_URI || name == b"xmlns" || prefix == b"xmlns" {
                    let declared = if name == b"xmlns" { Vec::new() } else { local };
                    if declaration(node, if declared.is_empty() { None } else { Some(&declared) }).is_none() {
                        let declared_c = cstring(&declared);
                        new_ns(node, cstring(&value).as_ptr(), if declared.is_empty() { std::ptr::null() } else { declared_c.as_ptr() });
                    }
                    return Some(b"1".to_vec());
                }
                let status = if uri.is_empty() {
                    set_prop(node, cstring(&name).as_ptr(), cstring(&value).as_ptr())
                } else {
                    let ns = ensure_ns(view.doc.raw, node, &prefix, &uri);
                    set_ns_prop(node, ns, cstring(&local).as_ptr(), cstring(&value).as_ptr())
                };
                (!status.is_null()).then(|| b"1".to_vec())
            }
            4 => match lookup(view.doc.raw, node, &name)? {
                Found::Attribute(attr) => { remove_prop(attr); Some(b"1".to_vec()) }
                Found::Declaration(_) => None,
            },
            5 => match lookup_ns(node, &uri, &name)? {
                Found::Attribute(attr) => { remove_prop(attr); Some(b"1".to_vec()) }
                Found::Declaration(_) => None,
            },
            6 => {
                let mut records: Vec<Vec<u8>> = Vec::new();
                if value == b"1" {
                    let mut ns = ffi::ns_defs(node);
                    while !ns.is_null() {
                        let prefix = bytes(ffi::ns_value(ns, 1));
                        let qualified = if prefix.is_empty() { b"xmlns".to_vec() } else { [b"xmlns:".to_vec(), prefix.clone()].concat() };
                        let local = if prefix.is_empty() { b"xmlns".to_vec() } else { prefix.clone() };
                        records.push([b"n".as_slice(), &[0], &qualified, &[0], &local, &[0], b"xmlns", &[0], XMLNS_URI, &[0], &bytes(ffi::ns_value(ns, 0))].concat());
                        ns = ffi::ns_next(ns);
                    }
                }
                let mut attr = ffi::attrs(node);
                while !attr.is_null() {
                    let local = bytes(ffi::name(attr));
                    let prefix = bytes(ffi::ns(attr, 1));
                    let qualified = if prefix.is_empty() { local.clone() } else { [prefix.clone(), b":".to_vec(), local.clone()].concat() };
                    records.push([b"a".as_slice(), &[0], &qualified, &[0], &local, &[0], &prefix, &[0], &bytes(ffi::ns(attr, 0)), &[0], &owned_bytes(ffi::text(attr))].concat());
                    attr = ffi::next(attr);
                }
                Some([b"1".to_vec(), super::abi::encode_hex(&records.join(&1u8)).into_bytes()].concat())
            }
            _ => None,
        }
    }).unwrap_or_default()))
}
