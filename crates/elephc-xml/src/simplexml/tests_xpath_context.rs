//! Purpose:
//! Covers XPath namespace context inheritance, cloning and empty selections.
//!
//! Called from:
//! - Managed-native SimpleXML bridge tests in CI.
//!
//! Key details:
//! - Attribute, comment and PI views resolve prefixes from their element ancestors.
//! - PHP clones discard explicit registrations; empty selections can register them.

use super::*;

/// Non-element contexts inherit the nearest declaration instead of a registered alias.
#[test]
fn simplexml_xpath_non_element_contexts_walk_ancestor_namespaces() {
    let root = parse(b"<r xmlns:p='urn:outer' a='1'><p:c>outer</p:c><!--r--><?r data?><q xmlns:p='urn:inner' b='2'><p:c>inner</p:c><!--q--><?q data?></q></r>", 0, "", false).unwrap();
    for (expression, expected) in [
        ("/r/@a", "outer"), ("/r/comment()", "outer"), ("/r/processing-instruction()", "outer"),
        ("/r/q/@b", "inner"), ("/r/q/comment()", "inner"), ("/r/q/processing-instruction()", "inner"),
    ] {
        let nodes = root.xpath(expression).unwrap();
        assert_eq!(nodes.len(), 1, "{expression}");
        let context = root.with_scope(Scope::Node(nodes[0]));
        context.xpath_ns.borrow_mut().insert(b"p".to_vec(), b"urn:unused".to_vec());
        let found = context.xpath("../p:c").unwrap();
        assert_eq!(found.len(), 1, "{expression}");
        assert_eq!(unsafe { owned_bytes(ffi::text(found[0])) }, expected.as_bytes());
    }
}

/// Root-document and detached-subtree clones discard registrations without losing declarations.
#[test]
fn simplexml_xpath_clones_start_with_independent_empty_registrations() {
    let root = parse(b"<r xmlns:p='urn:p'><p:v>one</p:v><a><p:v>two</p:v></a></r>", 0, "", false).unwrap();
    for source in [root.clone(), root.select(2, "a", "", false, 0).select(4, "", "", false, 0)] {
        source.xpath_ns.borrow_mut().insert(b"q".to_vec(), b"urn:p".to_vec());
        let cloned = source.deep_clone().unwrap();
        assert!(cloned.xpath_ns.borrow().is_empty());
        assert_eq!(cloned.xpath(".//q:v"), Err(query::XPathError::Invalid));
        assert!(!cloned.xpath(".//p:v").unwrap().is_empty());
        cloned.xpath_ns.borrow_mut().insert(b"q".to_vec(), b"urn:other".to_vec());
        assert!(!source.xpath(".//q:v").unwrap().is_empty());
    }
}

/// Missing children and attribute lists validate registrations, retaining them for later queries.
#[test]
fn simplexml_xpath_empty_selection_registration_uses_the_document_context() {
    let source = cstring("<r xmlns:p='urn:p'><p:c>yes</p:c></r>");
    let empty = cstring("");
    let prefix = cstring("q");
    let uri = cstring("urn:p");
    let name = cstring("later");
    let root = unsafe { abi::elephc_xml_sx_load(source.as_ptr(), source.as_bytes().len() as i64, 0, empty.as_ptr(), 0, empty.as_ptr()) };
    assert_ne!(root, 0);
    unsafe {
        for kind in [2, 1] {
            let selected = abi::elephc_xml_sx_select(root, kind, name.as_ptr(), empty.as_ptr(), 0, 0);
            assert_ne!(selected, 0);
            assert_eq!(abi::elephc_xml_sx_int(selected, 1), 0);
            assert_eq!(abi::elephc_xml_sx_register(selected, prefix.as_ptr(), uri.as_ptr()), 1);
            assert_eq!(abi::elephc_xml_sx_register(selected, empty.as_ptr(), uri.as_ptr()), 0);
            assert_eq!(abi::elephc_xml_sx_register(selected, prefix.as_ptr(), empty.as_ptr()), 1);
            assert_eq!(abi::elephc_xml_sx_register(selected, prefix.as_ptr(), uri.as_ptr()), 1);
            if kind == 2 {
                assert_eq!(abi::elephc_xml_sx_mutate(root, 2, name.as_ptr(), empty.as_ptr(), empty.as_ptr(), 0, 0), 1);
                let expression = cstring("../q:c");
                let result = abi::elephc_xml_sx_xpath(selected, expression.as_ptr());
                assert!(result > 0);
                assert_eq!(abi::elephc_xml_sx_int(result, 0), 1);
                abi::elephc_xml_sx_free(result);
            }
            abi::elephc_xml_sx_free(selected);
        }
    }
    abi::elephc_xml_sx_free(root);
}
