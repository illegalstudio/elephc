//! Purpose:
//! Preserves PHP byte strings through native SimpleXML calls without UTF-8 replacement.
//!
//! Called from:
//! - Managed-native bridge tests in CI.
//!
//! Key details:
//! - Live values, selectors, names, and namespace results use the same raw-byte ABI.

use super::*;

/// The native handle ABI round-trips invalid UTF-8 attribute keys and element payloads.
#[test]
fn simplexml_native_abi_preserves_non_utf8_php_values_and_names() {
    let empty = cstring("");
    let source = cstring("<r/>");
    let root = unsafe { abi::elephc_xml_sx_load(source.as_ptr(), 4, 0, empty.as_ptr(), 0, empty.as_ptr()) };
    let key = cstring(b"k\xff");
    let value = cstring(b"\xff\xfe");
    assert_eq!(unsafe { abi::elephc_xml_sx_mutate(root, 3, key.as_ptr(), value.as_ptr(), empty.as_ptr(), 0, 0) }, 1);
    let selected = unsafe { abi::elephc_xml_sx_select(root, 3, key.as_ptr(), empty.as_ptr(), 0, 0) };
    assert_eq!(unsafe { bytes(abi::elephc_xml_sx_string(selected, 0)) }.as_slice(), b"\xff\xfe");
    assert_eq!(unsafe { bytes(abi::elephc_xml_sx_string(selected, 1)) }.as_slice(), b"k\xff");
    let name = cstring(b"b\xe9");
    let text = cstring(b"caf\xe9");
    let child = unsafe { abi::elephc_xml_sx_mutate(root, 20, name.as_ptr(), text.as_ptr(), empty.as_ptr(), 1, 0) };
    assert!(child > 0);
    assert_eq!(unsafe { bytes(abi::elephc_xml_sx_string(child, 0)) }.as_slice(), b"caf\xe9");
    assert_eq!(unsafe { bytes(abi::elephc_xml_sx_string(child, 1)) }.as_slice(), b"b\xe9");
    for handle in [selected, child, root] { abi::elephc_xml_sx_free(handle); }
}

/// Namespace selectors and enumeration keep literal non-UTF-8 URI bytes intact.
#[test]
fn simplexml_namespace_filters_keep_byte_exact_uris() {
    let root = parse(b"<r/>", 0, "", false).unwrap();
    let child = root.add_checked(b"p:child", Some(b"value"), Some(b"urn:\xe9"), false).unwrap();
    let selected = root.select(0, "", b"urn:\xe9", false, 0);
    assert_eq!(selected.first(), Some(child));
    assert_eq!(root.namespaces(true, true, true), vec![(b"p".to_vec(), b"urn:\xe9".to_vec())]);
}
