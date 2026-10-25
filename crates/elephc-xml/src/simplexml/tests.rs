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
