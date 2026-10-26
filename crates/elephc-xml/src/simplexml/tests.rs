//! Purpose:
//! Native SimpleXML ownership, selection, and query regressions.
//!
//! Called from:
//! - Managed-native CI with `elephc_xml_native` enabled.
//!
//! Key details:
//! - Uses real managed libxml2 and checks document isolation after clone.

use super::*;

/// Child selectors observe later appends and indexed writes escape raw text.
#[test]
fn simplexml_live_selection_and_mutation() {
    let root = parse(b"<r><a>one</a><a>two</a></r>", 0, "", false).unwrap();
    let items = root.select(2, "a", "", false, 0);
    assert_eq!(items.items().len(), 2);
    assert_eq!(items.write(4, "", 0, "A&B"), 1);
    assert!(root.add("a", Some("three"), None, false).is_some());
    assert_eq!(items.items().len(), 3);
    let first = items.select(4, "", "", false, 0);
    assert_eq!(unsafe { owned_string(ffi::text(first.first().unwrap())) }, "A&B");
    assert_eq!(first.items().len(), 0);
}

/// Clone preserves repeated-node selection while isolating both document and attributes.
#[test]
fn simplexml_clone_owns_an_independent_document() {
    let root = parse(b"<r><a>one</a><a>two</a></r>", 0, "", false).unwrap();
    let items = root.select(2, "a", "", false, 0);
    let clone = items.deep_clone().unwrap();
    assert_eq!(clone.items().len(), 2);
    assert_eq!(clone.write(4, "", 0, "changed"), 1);
    assert_eq!(unsafe { owned_string(ffi::text(items.first().unwrap())) }, "one");
    assert_eq!(unsafe { owned_string(ffi::text(clone.first().unwrap())) }, "changed");
}

/// A wrapper survives parent release, node removal, and replacement of its parent's text.
#[test]
fn simplexml_detached_selection_keeps_storage_alive() {
    let root = parse(b"<r><a><b>kept</b></a></r>", 0, "", false).unwrap();
    let named = root.select(2, "a", "", false, 0);
    let selected = named.with_scope(Scope::Node(named.first().unwrap()));
    root.remove(2, "a", 0);
    drop(root);
    assert_eq!(selected.select(2, "b", "", false, 0).items().len(), 1);
    drop(named);
    assert_eq!(unsafe { string(ffi::name(selected.first().unwrap())) }, "a");
}

/// Concrete node and attribute aliases remain clonable after their removal from the live tree.
#[test]
fn simplexml_clone_preserves_detached_nodes_and_attributes() {
    let root = parse(b"<r id='old'><a>one</a></r>", 0, "", false).unwrap();
    let selected = root.select(2, "a", "", false, 0).select(4, "", "", false, 0);
    let attribute = root.select(3, "id", "", false, 0);
    let attribute = attribute.with_scope(Scope::Node(attribute.first().unwrap()));
    root.remove(2, "a", 0);
    root.remove(3, "id", 0);
    let copied = selected.deep_clone().unwrap();
    let copied_attribute = attribute.deep_clone().unwrap();
    assert_eq!(copied.write(4, "", 0, "two"), 1);
    assert_eq!(unsafe { owned_string(ffi::text(selected.first().unwrap())) }, "one");
    assert_eq!(unsafe { owned_string(ffi::text(copied.first().unwrap())) }, "two");
    assert_eq!(unsafe { owned_string(ffi::text(copied_attribute.first().unwrap())) }, "old");
}

/// XPath binds in-scope namespaces and text results map to their containing element.
#[test]
fn simplexml_xpath_namespace_and_text_nodes() {
    let root = parse(b"<r xmlns:p='urn:p'><p:a>one</p:a><a>two</a></r>", 0, "", false).unwrap();
    assert_eq!(root.xpath("//p:a").unwrap().len(), 1);
    root.xpath_ns.borrow_mut().insert("q".into(), "urn:p".into());
    assert_eq!(root.xpath("//q:a/text()").unwrap().len(), 1);
    assert_eq!(root.namespaces(true, true, true), vec![("p".into(), "urn:p".into())]);
    assert_eq!(root.xpath("count(//a)"), Err(query::XPathError::Scalar(3)));
}

/// Newly materialized wrappers own independent prefix maps while aliases and clones preserve theirs.
#[test]
fn simplexml_derived_xpath_registrations_are_isolated() {
    let root = parse(b"<r xmlns:p='urn:p'><p:v>one</p:v><c/></r>", 0, "", false).unwrap();
    root.xpath_ns.borrow_mut().insert("q".into(), "urn:p".into());
    let alias = root.clone();
    let copied = root.deep_clone().unwrap();
    let results = root.with_scope(Scope::XPath(root.xpath("//c").unwrap()));
    let xpath = results.item(0).unwrap();
    let iterated = root.item(0).unwrap();
    let added = root.with_scope(Scope::Node(root.add("d", None, None, false).unwrap()));
    for derived in [xpath, iterated, added] {
        assert_eq!(derived.xpath("//q:v"), Err(query::XPathError::Invalid));
        assert!(diagnostics::take().contains("Undefined namespace prefix"));
        derived.xpath_ns.borrow_mut().insert("q".into(), "urn:other".into());
        assert_eq!(root.xpath("//q:v").unwrap().len(), 1);
    }
    alias.xpath_ns.borrow_mut().insert("q".into(), "urn:other".into());
    assert!(root.xpath("//q:v").unwrap().is_empty());
    assert_eq!(copied.xpath("//q:v").unwrap().len(), 1);
}

/// Parser diagnostics retain source/caret lines, respect options, and clear between operations.
#[test]
fn simplexml_parse_diagnostics_preserve_php_lines() {
    assert!(parse(b"<r>", 0, "", false).is_none());
    let messages = diagnostics::take();
    let lines = messages.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("Premature end of data in tag r line 1"));
    assert_eq!(lines[1], "<r>");
    assert_eq!(lines[2], "   ^");
    assert!(diagnostics::take().is_empty());
    assert!(parse(b"", 0, "", false).is_none());
    assert!(diagnostics::take().contains("Document is empty"));
    assert!(parse(b"<r>", 32, "", false).is_none());
    assert!(diagnostics::take().is_empty());
    assert!(parse(b"<r/>", 0, "", false).is_some());
    assert!(diagnostics::take().is_empty());
}

/// DOM parser callbacks have one contextual line and preserve warning versus notice severity.
#[test]
fn simplexml_dom_parser_diagnostics_use_structured_lines() {
    assert!(parse_with_diagnostics(b"<r>", 0, "", false, "", 2).is_none());
    assert_eq!(diagnostics::take(), "Warning: Premature end of data in tag r line 1 in Entity, line: 1\n");
    assert!(parse_with_diagnostics(b"<?xml version='1.1'?><r/>", 0, "", false, "", 2).is_some());
    assert_eq!(diagnostics::take(), "Notice: Unsupported version '1.1' in Entity, line: 1\n");
}

/// A selected attribute reports its namespace; an empty filtered attribute list reports none.
#[test]
fn simplexml_attribute_used_namespace_matches_selected_node() {
    let root = parse(b"<r xmlns:p='urn:p' p:id='8'/>", 0, "", false).unwrap();
    let attribute = root.select(1, "", "p", true, 0).select(2, "id", "", false, 0);
    assert_eq!(attribute.namespaces(false, false, false), vec![("p".into(), "urn:p".into())]);
    assert_eq!(attribute.namespaces(false, true, false), vec![("p".into(), "urn:p".into())]);
    assert!(root.select(1, "", "", false, 0).namespaces(false, false, false).is_empty());
}

/// Comparisons retain the receiver base even after a live selection moves to another node.
#[test]
fn simplexml_comparison_identity_preserves_retained_base_nodes() {
    let root = parse(b"<r><a>1</a><a>2</a><b>1</b></r>", 0, "", false).unwrap();
    let a = root.select(2, "a", "", false, 0);
    let b = root.select(2, "b", "", false, 0);
    assert_eq!(a.identity, root.identity);
    assert_eq!(b.identity, root.identity);
    let first = a.select(4, "", "", false, 0);
    let saved = a.select(2, "missing", "", false, 0);
    assert_eq!(saved.identity, first.identity);
    root.remove(2, "a", 0);
    assert_eq!(saved.identity, first.identity);
    assert!(a.first().is_none());
    let copied = first.deep_clone().unwrap();
    assert!(!copied.identity.is_null());
    assert_ne!(copied.identity, first.identity);
}

/// XPath attribute wrappers count and iterate only their selected expanded name.
#[test]
fn simplexml_xpath_attribute_wrappers_preserve_namespace_uri() {
    let root = parse(b"<r xmlns:p='urn:p' id='7' p:id='8'/>", 0, "", false).unwrap();
    let results = root.with_scope(Scope::XPath(root.xpath("//@*").unwrap()));
    for (index, expected) in ["7", "8"].into_iter().enumerate() {
        let attribute = results.item(index).unwrap();
        assert!(matches!(attribute.scope, Scope::Attributes(_, Some(_))));
        assert_eq!(attribute.items().len(), 1);
        assert_eq!(unsafe { owned_string(ffi::text(attribute.first().unwrap())) }, expected);
    }
    let offset = root.select(3, "id", "", false, 0);
    assert_eq!(offset.with_scope(Scope::Node(offset.first().unwrap())).items().len(), 0);
}

/// Comment and PI XPath nodes retain their content, while scalar types remain distinct.
#[test]
fn simplexml_xpath_miscellaneous_nodes_and_scalar_types() {
    let root = parse(b"<r><!--note--><?work data?></r>", 0, "", false).unwrap();
    let nodes = root.xpath("comment() | processing-instruction()").unwrap();
    assert_eq!(nodes.len(), 2);
    assert_eq!(unsafe { owned_string(ffi::text(nodes[0])) }, "note");
    assert_eq!(unsafe { owned_string(ffi::text(nodes[1])) }, "data");
    assert_eq!(root.xpath("true()"), Err(query::XPathError::Scalar(2)));
    assert_eq!(root.xpath("string(.)"), Err(query::XPathError::Scalar(4)));
}

/// The native empty predicate follows the first text child's exact PHP falsiness rules.
#[test]
fn simplexml_container_empty_is_distinct_from_boolean() {
    let root = parse(b"<r z='0' n='00'><blank/><zero>0</zero><number>00</number><nested><a/></nested><cdata><![CDATA[0]]></cdata></r>", 0, "", false).unwrap();
    for (name, expected) in [("blank", 1), ("zero", 1), ("number", 0), ("nested", 0), ("cdata", 0)] {
        let node = root.select(2, name, "", false, 0).first().unwrap();
        assert_eq!(unsafe { ffi::empty(node) }, expected, "{name}");
    }
    for (name, expected) in [("z", 1), ("n", 0)] {
        let node = root.select(3, name, "", false, 0).first().unwrap();
        assert_eq!(unsafe { ffi::empty(node) }, expected, "{name}");
    }
}

/// Root and indexed-node iteration retain explicit namespace filters independently of public children().
#[test]
fn simplexml_iteration_retains_namespace_filter() {
    let root = parse(b"<r xmlns:p='urn:p'><p:a><p:b>value</p:b><b>plain</b></p:a><p:a/><a/></r>",
        0, "urn:p", false).unwrap();
    assert_eq!(root.items().len(), 2);
    let node = root.select(2, "a", "", false, 0).select(4, "", "", false, 0);
    let children = node.items();
    assert_eq!(children.len(), 1);
    assert_eq!(unsafe { owned_string(ffi::text(children[0])) }, "value");
    assert_eq!(node.select(0, "", "", false, 0).items().len(), 1);
}

/// Missing named ancestors are materialized once for a chained assignment.
#[test]
fn simplexml_materializes_missing_ancestors() {
    let root = parse(b"<r/>", 0, "", false).unwrap();
    let missing = root.select(2, "outer", "", false, 0);
    assert_eq!(missing.write(2, "inner", 0, "value"), 1);
    assert_eq!(missing.nodes().len(), 1);
    assert_eq!(unsafe { owned_string(ffi::text(missing.select(2, "inner", "", false, 0).first().unwrap())) }, "value");
}

/// Indexed aliases keep their node after sibling removal and tolerate repeated unsets.
#[test]
fn simplexml_indexed_alias_survives_removal() {
    let root = parse(b"<r><a>one</a><a>two</a></r>", 0, "", false).unwrap();
    let items = root.select(2, "a", "", false, 0);
    let first = items.select(4, "", "", false, 0);
    items.remove(4, "", -1);
    assert_eq!(items.nodes().len(), 2);
    items.remove(4, "", 0);
    assert_eq!(unsafe { owned_string(ffi::text(first.first().unwrap())) }, "one");
    assert_eq!(unsafe { owned_string(ffi::text(items.first().unwrap())) }, "two");
    first.remove(4, "", 0);
    first.remove(4, "", 0);
    assert_eq!(first.doc.detached.borrow().len(), 1);
}

/// Rejected attribute additions preserve unique XML names and namespace prefixes.
#[test]
fn simplexml_attribute_addition_rejects_duplicates_and_missing_prefix() {
    let root = parse(b"<r id='old'/>", 0, "", false).unwrap();
    assert!(root.add("id", Some("new"), None, true).is_none());
    assert!(root.add("other", Some("v"), Some("urn:p"), true).is_none());
    assert!(root.add("p:other", Some("v"), Some("urn:p"), true).is_some());
    assert_eq!(root.select(1, "", "", false, 0).nodes().len(), 1);
    assert_eq!(root.select(1, "", "urn:p", false, 0).nodes().len(), 1);
}

/// Native mutation errors distinguish missing parents, attribute lists, and duplicate names.
#[test]
fn simplexml_checked_add_resolves_parent_and_reports_failures() {
    let root = parse(b"<r xmlns:p='urn:p' id='1'/>", 0, "", false).unwrap();
    let attribute = root.select(3, "id", "", false, 0);
    let attribute = attribute.with_scope(Scope::Node(attribute.first().unwrap()));
    assert!(attribute.add_checked("extra", Some("2"), None, true).is_ok());
    let attrs = root.select(1, "", "", false, 0);
    assert!(attrs.add_checked("listed", Some("3"), None, true).is_ok());
    assert_eq!(attrs.add_checked("child", None, None, false), Err(mutation::AddError::ElementToAttributes));
    assert_eq!(root.add_checked("id", Some("2"), None, true), Err(mutation::AddError::DuplicateAttribute));
    assert_eq!(root.add_checked("bare", Some("2"), Some("urn:p"), true), Err(mutation::AddError::AttributePrefix));
    assert!(root.add_checked("q:child", Some("v"), Some("urn:p"), false).is_ok());
    let child = root.select(0, "", "urn:p", false, 0).select(2, "child", "", false, 0);
    assert_eq!(unsafe { string(ffi::ns(child.first().unwrap(), 1)) }, "p");
}

/// Write-fetch creates a stable empty node, while ordinary absent selections stay lazy.
#[test]
fn simplexml_write_fetch_and_absent_read_views() {
    let root = parse(b"<r/>", 0, "", false).unwrap();
    let absent = root.select(2, "item", "", false, 0);
    assert_eq!(absent.write(4, "", 0, "ignored"), 0);
    assert_eq!(absent.write(5, "", 0, "ignored"), 0);
    assert!(root.items().is_empty());
    let created = absent.ensure_first().unwrap();
    let node = absent.with_scope(Scope::Node(created));
    assert_eq!(node.write(4, "", 1, "ignored"), 0);
    assert_eq!(unsafe { owned_string(ffi::text(created)) }, "");
    assert_eq!(node.write(5, "", 0, "stored"), 1);
    assert_eq!(unsafe { owned_string(ffi::text(created)) }, "stored");
    assert!(root.add_checked("1abc", Some("v"), None, false).is_ok());
    assert!(root.add_checked("1abc", Some("a"), None, true).is_ok());
}

/// Stream parsing keeps source URLs and diagnostics inside one parser context.
#[test]
fn simplexml_stream_input_keeps_parser_url() {
    let url = cstring("data://text/plain,source");
    let ns = cstring("");
    let bytes = b"<r>";
    unsafe {
        assert_eq!(abi::elephc_xml_sx_load_stream(url.as_ptr(), bytes.as_ptr().cast(), bytes.len() as i64, 0, ns.as_ptr(), 0), 0);
        let diagnostics = string(abi::elephc_xml_sx_diagnostics());
        assert!(diagnostics.contains("data://text/plain,source"), "{diagnostics}");
        assert!(diagnostics.contains("Premature end"), "{diagnostics}");
        let bytes = b"<r/>";
        let handle = abi::elephc_xml_sx_load_stream(url.as_ptr(), bytes.as_ptr().cast(), bytes.len() as i64, 0, ns.as_ptr(), 0);
        assert_ne!(handle, 0);
        assert!(string(abi::elephc_xml_sx_diagnostics()).is_empty());
        abi::elephc_xml_sx_free(handle);
        assert_eq!(abi::elephc_xml_sx_load_stream(url.as_ptr(), bytes.as_ptr().cast(), -1, 0, ns.as_ptr(), 0), 0);
        assert!(string(abi::elephc_xml_sx_diagnostics()).contains("I/O warning"));
    }
}
