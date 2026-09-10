//! Purpose:
//! Regression tests for SimpleXML document claims, fresh views, and iterator ownership.
//!
//! Called from:
//! - `cargo test -p elephc-dom simplexml_foundation` through Rust's test harness.
//!
//! Key details:
//! - Every fixture is a real libxml2 graph freed through its final `Rc<DocumentGraph>`.
//! - Tests distinguish fresh PHP-view handles from intentionally reused iterator data.

use std::rc::Rc;

use crate::abi::{
    DomClassMetadataEntry, Value, DOM_CLASS_NO_PARENT, PHP_ERROR_KIND_EXCEPTION,
    DIAGNOSTIC_FLAG_CALLSITE_LOCATION, PHP_ERROR_KIND_ERROR,
    PHP_ERROR_KIND_TYPE_ERROR,
    PHP_ERROR_KIND_VALUE_ERROR, STATUS_THROW, VALUE_ARRAY, VALUE_BOOL,
    VALUE_BRIDGE_HANDLE, VALUE_BYTES, VALUE_INT, VALUE_MAP, VALUE_NULL,
};
use crate::context::{Context, Host};
use crate::handles::HandleError;
use crate::{RequestHeader, ResultHeader, ABI_VERSION, STATUS_OK};
use crate::objects::{
    DocumentFamily, DocumentGraph, DocumentObject, DomApiFamily, DomClaimError,
    NativeObject, NodeObject, SimpleXmlIteratorState, SimpleXmlIteratorType,
    SimpleXmlObject, HANDLE_DOCUMENT, HANDLE_NODE, HANDLE_SIMPLEXML,
};

/// Parses a minimal XML tree and returns its shared unclaimed graph and element pointers.
fn unclaimed_xml_graph() -> (Rc<DocumentGraph>, usize, usize) {
    let parsed = crate::native::document_parse_xml(
        b"<root><child/></root>",
        0,
        None,
        None,
    )
    .expect("SimpleXML fixture parses");
    let document = parsed.document.expect("parsed fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let child = crate::native::node_first_child(root).expect("fixture has a child");
    (DocumentGraph::new_unclaimed_xml(document), root, child)
}

/// Builds one context without host callbacks for native ownership tests.
fn context() -> Context {
    Context::new(Host {
        user_data: 0,
        call: None,
    })
}

/// Builds one direct SimpleXML view retaining the supplied shared graph.
fn direct_view(
    pointer: usize,
    graph: Rc<DocumentGraph>,
    wrapper_kind: u64,
) -> SimpleXmlObject {
    SimpleXmlObject::new(
        pointer,
        graph,
        wrapper_kind,
        SimpleXmlIteratorState::direct(None, false),
    )
}

/// Invokes one empty wrapper lifecycle operation through the public bridge ABI.
fn invoke_wrapper_operation(
    context: u64,
    operation: &str,
    receiver: u64,
) -> (u32, ResultHeader) {
    let opcode = crate::generated::opcodes::OPERATIONS
        .iter()
        .find_map(|(opcode, key)| (*key == operation).then_some(*opcode))
        .expect("wrapper lifecycle opcode exists");
    let request = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: std::mem::size_of::<RequestHeader>() as u32,
        opcode,
        flags: 0,
        receiver,
        value_count: 0,
        byte_count: 0,
    };
    let mut result = ResultHeader::abi_error();
    let status = unsafe {
        crate::elephc_dom_call(
            context,
            (&request as *const RequestHeader).cast::<u8>(),
            std::mem::size_of::<RequestHeader>() as u64,
            &mut result,
        )
    };
    (status, result)
}

/// Encodes and validates one test request through the production ABI decoder.
fn decoded_request(
    operation: &str,
    receiver: u64,
    values: &[Value],
    bytes: &[u8],
) -> crate::request::Request {
    let opcode = crate::generated::opcodes::OPERATIONS
        .iter()
        .find_map(|(opcode, key)| (*key == operation).then_some(*opcode))
        .expect("test operation opcode exists");
    let header = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: std::mem::size_of::<RequestHeader>() as u32,
        opcode,
        flags: 0,
        receiver,
        value_count: values.len() as u64,
        byte_count: bytes.len() as u64,
    };
    let mut encoded = Vec::with_capacity(
        std::mem::size_of::<RequestHeader>()
            + std::mem::size_of_val(values)
            + bytes.len(),
    );
    unsafe {
        encoded.extend_from_slice(std::slice::from_raw_parts(
            (&header as *const RequestHeader).cast::<u8>(),
            std::mem::size_of::<RequestHeader>(),
        ));
        encoded.extend_from_slice(std::slice::from_raw_parts(
            values.as_ptr().cast::<u8>(),
            std::mem::size_of_val(values),
        ));
    }
    encoded.extend_from_slice(bytes);
    crate::request::decode(encoded.as_ptr(), encoded.len() as u64)
        .expect("test request decodes")
}

/// Builds one byte-string ABI value referencing the complete supplied buffer.
fn bytes_value(bytes: &[u8]) -> Value {
    bytes_value_range(0, bytes.len())
}

/// Builds one byte-string ABI value referencing a selected request-byte range.
fn bytes_value_range(offset: usize, length: usize) -> Value {
    Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: offset as u64,
        payload1: length as u64,
    }
}

/// Builds one opaque native-wrapper argument value.
fn bridge_value(handle: u64) -> Value {
    Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: handle,
        payload1: 0,
    }
}

/// Builds one null ABI argument value.
fn null_value() -> Value {
    Value {
        tag: VALUE_NULL,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }
}

/// Builds one signed PHP integer ABI argument value.
fn integer_value(value: i64) -> Value {
    Value {
        tag: VALUE_INT,
        flags: 0,
        payload0: value as u64,
        payload1: 0,
    }
}

/// Builds one strict boolean ABI argument value.
fn boolean_value(value: bool) -> Value {
    Value {
        tag: VALUE_BOOL,
        flags: 0,
        payload0: u64::from(value),
        payload1: 0,
    }
}

/// Builds one context containing canonical legacy document and element wrappers.
fn legacy_tree_context() -> (Context, Rc<DocumentGraph>, u64, u64) {
    let parsed = crate::native::document_parse_xml(b"<root/>", 0, None, None)
        .expect("legacy fixture parses");
    let pointer = parsed.document.expect("legacy fixture has a document");
    let root = crate::native::document_element(pointer).expect("fixture has a root");
    let document = DocumentObject::new(pointer, DocumentFamily::Legacy);
    let graph = document.graph();
    let mut context = context();
    let document_handle = context
        .native_objects
        .insert(HANDLE_DOCUMENT, NativeObject::Document(document));
    context.document_handles.insert(pointer, document_handle);
    let node_handle = context.native_objects.insert(
        HANDLE_NODE,
        NativeObject::Node(NodeObject::new(root, Rc::clone(&graph), 101)),
    );
    context.node_handles.insert(root, node_handle);
    (context, graph, document_handle, node_handle)
}

/// Verifies legacy and SimpleXML XML start unclaimed while modern documents are pinned.
#[test]
fn simplexml_foundation_document_claim_initial_state_matches_php_src() {
    let legacy_pointer = crate::native::document_new(b"1.0", b"")
        .expect("legacy document allocation succeeds");
    let legacy = DocumentObject::new(legacy_pointer, DocumentFamily::Legacy);
    assert_eq!(legacy.graph().dom_api(), None);
    assert_eq!(legacy.graph().family(), DocumentFamily::Legacy);

    let modern_html_pointer = crate::native::document_new_html(None)
        .expect("modern HTML allocation succeeds");
    let modern_html =
        DocumentObject::new(modern_html_pointer, DocumentFamily::ModernHtml);
    assert_eq!(modern_html.graph().dom_api(), Some(DomApiFamily::Modern));
    assert_eq!(
        modern_html.graph().claim_dom_api(DomApiFamily::Modern),
        Ok(DocumentFamily::ModernHtml)
    );
    assert_eq!(
        modern_html.graph().claim_dom_api(DomApiFamily::Legacy),
        Err(DomClaimError::ConflictingFamily)
    );

    let (simplexml, _, _) = unclaimed_xml_graph();
    assert_eq!(simplexml.dom_api(), None);
}

/// Verifies one claim is visible through every SimpleXML view and rejects the other API.
#[test]
fn simplexml_foundation_legacy_claim_is_document_wide() {
    let (graph, root, child) = unclaimed_xml_graph();
    let root_view = direct_view(root, Rc::clone(&graph), 501);
    let child_view = direct_view(child, Rc::clone(&graph), 501);
    let root_graph = root_view.document();
    let child_graph = child_view.document();
    assert!(Rc::ptr_eq(&root_graph, &child_graph));
    assert_eq!(
        root_graph.claim_dom_api(DomApiFamily::Legacy),
        Ok(DocumentFamily::Legacy)
    );
    assert_eq!(child_graph.dom_api(), Some(DomApiFamily::Legacy));
    assert_eq!(
        child_graph.claim_dom_api(DomApiFamily::Modern),
        Err(DomClaimError::ConflictingFamily)
    );
}

/// Verifies the first modern claim converts XML once and pins every shared view.
#[test]
fn simplexml_foundation_modern_claim_is_idempotent_and_document_wide() {
    let (graph, root, child) = unclaimed_xml_graph();
    let root_view = direct_view(root, Rc::clone(&graph), 601);
    let child_view = direct_view(child, Rc::clone(&graph), 601);
    assert_eq!(
        root_view.document().claim_dom_api(DomApiFamily::Modern),
        Ok(DocumentFamily::ModernXml)
    );
    assert_eq!(
        child_view.document().claim_dom_api(DomApiFamily::Modern),
        Ok(DocumentFamily::ModernXml)
    );
    assert_eq!(graph.dom_api(), Some(DomApiFamily::Modern));
    assert_eq!(
        graph.claim_dom_api(DomApiFamily::Legacy),
        Err(DomClaimError::ConflictingFamily)
    );
}

/// Verifies equal node pointers still receive fresh SimpleXML handles and wrapper state.
#[test]
fn simplexml_foundation_views_are_fresh_and_preserve_subclass_state() {
    let (graph, root, _) = unclaimed_xml_graph();
    let mut first_view = SimpleXmlObject::new(
        root,
        Rc::clone(&graph),
        701,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::Element,
            Some(b"child".to_vec()),
            Some(b"urn:one".to_vec()),
            false,
        ),
    );
    first_view.register_xpath_namespace(b"p".to_vec(), b"urn:xpath".to_vec());
    let second_view = direct_view(root, Rc::clone(&graph), 702);
    let mut context = context();
    let first = context.insert_simplexml_external(first_view);
    let second = context.insert_simplexml_external(second_view);
    assert_ne!(first, second);

    let first_object = context
        .native_objects
        .get(first, HANDLE_SIMPLEXML)
        .expect("first fresh handle remains live")
        .simplexml()
        .expect("first handle stores a SimpleXML view");
    assert_eq!(first_object.pointer(), root);
    assert_eq!(first_object.wrapper_kind(), 701);
    assert_eq!(first_object.iterator().kind(), SimpleXmlIteratorType::Element);
    assert_eq!(first_object.iterator().name(), Some(b"child".as_slice()));
    assert_eq!(
        first_object.iterator().namespace_or_prefix(),
        Some(b"urn:one".as_slice())
    );
    assert!(!first_object.iterator().is_prefix());
    assert_eq!(
        first_object.xpath_namespaces(),
        &[(b"p".to_vec(), b"urn:xpath".to_vec())]
    );

    let second_object = context
        .native_objects
        .get(second, HANDLE_SIMPLEXML)
        .expect("second fresh handle remains live")
        .simplexml()
        .expect("second handle stores a SimpleXML view");
    assert_eq!(second_object.wrapper_kind(), 702);
    assert!(second_object.xpath_namespaces().is_empty());
    assert!(Rc::ptr_eq(&first_object.document(), &second_object.document()));

    context
        .release_simplexml_external(first)
        .expect("first wrapper releases cleanly");
    assert!(matches!(
        context.native_objects.get(first, HANDLE_SIMPLEXML),
        Err(HandleError::Stale)
    ));
    assert!(context.native_objects.get(second, HANDLE_SIMPLEXML).is_ok());
    context
        .release_simplexml_external(second)
        .expect("second wrapper releases cleanly");
}

/// Verifies every php-src iterator mode retains its independent name and namespace filter.
#[test]
fn simplexml_foundation_iterator_modes_preserve_exact_view_filters() {
    let direct = SimpleXmlIteratorState::direct(Some(b"p".to_vec()), true);
    assert_eq!(direct.kind(), SimpleXmlIteratorType::None);
    assert_eq!(direct.namespace_or_prefix(), Some(b"p".as_slice()));
    assert!(direct.is_prefix());

    let child = SimpleXmlIteratorState::new(
        SimpleXmlIteratorType::Child,
        None,
        Some(b"urn:child".to_vec()),
        false,
    );
    assert_eq!(child.kind(), SimpleXmlIteratorType::Child);
    assert_eq!(child.name(), None);
    assert_eq!(
        child.namespace_or_prefix(),
        Some(b"urn:child".as_slice())
    );

    let attributes = SimpleXmlIteratorState::new(
        SimpleXmlIteratorType::AttrList,
        Some(b"id".to_vec()),
        Some(b"a".to_vec()),
        true,
    );
    assert_eq!(attributes.kind(), SimpleXmlIteratorType::AttrList);
    assert_eq!(attributes.name(), Some(b"id".as_slice()));
    assert_eq!(attributes.namespace_or_prefix(), Some(b"a".as_slice()));
    assert!(attributes.is_prefix());
}

/// Verifies iterator data keeps one identity across exposure and independent lifetimes.
#[test]
fn simplexml_foundation_iterator_current_balances_internal_and_external_owners() {
    let (graph, root, child) = unclaimed_xml_graph();
    let mut context = context();
    let parent = context.insert_simplexml_external(direct_view(
        root,
        Rc::clone(&graph),
        801,
    ));
    let current = context
        .install_fresh_simplexml_iterator_current(
            parent,
            direct_view(child, Rc::clone(&graph), 801),
        )
        .expect("iterator current installs");
    assert_eq!(
        context.simplexml_iterator_current(parent),
        Ok(Some(current))
    );

    context
        .expose_simplexml_handle(current)
        .expect("first current exposure records a PHP owner");
    context
        .expose_simplexml_handle(current)
        .expect("repeat current exposure preserves the same identity");
    assert_eq!(
        context.simplexml_iterator_current(parent),
        Ok(Some(current))
    );

    context
        .release_simplexml_external(current)
        .expect("PHP current wrapper can die while the iterator retains it");
    assert!(context
        .native_objects
        .get(current, HANDLE_SIMPLEXML)
        .is_ok());
    context
        .expose_simplexml_handle(current)
        .expect("the retained iterator identity can be materialized again");

    context
        .release_simplexml_external(parent)
        .expect("parent can die while the exposed current remains alive");
    assert!(matches!(
        context.native_objects.get(parent, HANDLE_SIMPLEXML),
        Err(HandleError::Stale)
    ));
    assert!(context
        .native_objects
        .get(current, HANDLE_SIMPLEXML)
        .is_ok());
    context
        .release_simplexml_external(current)
        .expect("final current owner retires the handle");
    assert!(matches!(
        context.native_objects.get(current, HANDLE_SIMPLEXML),
        Err(HandleError::Stale)
    ));
}

/// Verifies advancing an iterator retires unexposed current data exactly once.
#[test]
fn simplexml_foundation_iterator_advance_releases_internal_only_current() {
    let (graph, root, child) = unclaimed_xml_graph();
    let mut context = context();
    let parent = context.insert_simplexml_external(direct_view(
        root,
        Rc::clone(&graph),
        901,
    ));
    let current = context
        .install_fresh_simplexml_iterator_current(
            parent,
            direct_view(child, graph, 901),
        )
        .expect("iterator current installs");
    context
        .clear_simplexml_iterator_current(parent)
        .expect("iterator advance clears current");
    assert_eq!(context.simplexml_iterator_current(parent), Ok(None));
    assert!(matches!(
        context.native_objects.get(current, HANDLE_SIMPLEXML),
        Err(HandleError::Stale)
    ));
    context
        .release_simplexml_external(parent)
        .expect("parent wrapper releases cleanly");
}

/// Verifies void iterator moves eagerly return one private strong-wrapper handle per live node.
#[test]
fn simplexml_foundation_iterator_moves_publish_eager_identity_and_balance_handles() {
    let parsed = crate::native::document_parse_xml(
        b"<root><first/><second/></root>",
        0,
        None,
        None,
    )
    .expect("iterator identity fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let parent = context.insert_simplexml_external(direct_view(root, graph, 1201));

    let rewind = decoded_request(
        "method:simplexmlelement::rewind",
        parent,
        &[],
        &[],
    );
    let first = crate::dispatch::dispatch(&mut context, &rewind)
        .expect("rewind eagerly materializes the first iterator wrapper");
    assert_eq!(first.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(first.frame.payload1, 1201);
    assert_eq!(
        context.simplexml_iterator_current(parent),
        Ok(Some(first.frame.payload0))
    );

    for operation in [
        "method:simplexmlelement::current",
        "method:simplexmlelement::getchildren",
    ] {
        let request = decoded_request(operation, parent, &[], &[]);
        let result = crate::dispatch::dispatch(&mut context, &request)
            .expect("current identity accessor succeeds");
        assert_eq!(result.value_tag, VALUE_BRIDGE_HANDLE);
        assert_eq!(result.frame.payload0, first.frame.payload0);
    }

    let next = decoded_request("method:simplexmlelement::next", parent, &[], &[]);
    let second = crate::dispatch::dispatch(&mut context, &next)
        .expect("next eagerly materializes the second iterator wrapper");
    assert_eq!(second.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(second.frame.payload1, 1201);
    assert_ne!(second.frame.payload0, first.frame.payload0);
    assert_eq!(
        context.simplexml_iterator_current(parent),
        Ok(Some(second.frame.payload0))
    );
    assert!(context
        .native_objects
        .get(first.frame.payload0, HANDLE_SIMPLEXML)
        .is_ok());
    context
        .release_simplexml_external(first.frame.payload0)
        .expect("replaced first wrapper retires after its private PHP owner releases");
    assert!(matches!(
        context
            .native_objects
            .get(first.frame.payload0, HANDLE_SIMPLEXML),
        Err(HandleError::Stale)
    ));

    let end = crate::dispatch::dispatch(&mut context, &next)
        .expect("advancing past the final child succeeds");
    assert_eq!(end.value_tag, VALUE_NULL);
    assert_eq!(context.simplexml_iterator_current(parent), Ok(None));
    context
        .release_simplexml_external(second.frame.payload0)
        .expect("final eager wrapper retires after reaching iterator end");
    assert!(matches!(
        context
            .native_objects
            .get(second.frame.payload0, HANDLE_SIMPLEXML),
        Err(HandleError::Stale)
    ));
    context
        .release_simplexml_external(parent)
        .expect("parent wrapper releases after its iterator reaches end");
}

/// Verifies native object cloning copies XML but never inherits live iterator data.
#[test]
fn simplexml_foundation_clone_resets_iterator_current_identity() {
    let parsed = crate::native::document_parse_xml(
        b"<root><first/><second/></root>",
        0,
        None,
        None,
    )
    .expect("clone fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let source = context.insert_simplexml_external(direct_view(root, graph, 1301));
    let rewind = decoded_request(
        "method:simplexmlelement::rewind",
        source,
        &[],
        &[],
    );
    let current = crate::dispatch::dispatch(&mut context, &rewind)
        .expect("source iterator initializes");
    assert_eq!(current.value_tag, VALUE_BRIDGE_HANDLE);

    let clone_request = decoded_request(
        "internal:bridge.object.clone",
        source,
        &[],
        &[],
    );
    let clone = crate::dispatch::dispatch(&mut context, &clone_request)
        .expect("SimpleXML object clone succeeds");
    assert_eq!(clone.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(clone.frame.payload1, 1301);
    assert_ne!(clone.frame.payload0, source);
    assert_eq!(context.simplexml_iterator_current(source), Ok(Some(current.frame.payload0)));
    assert_eq!(context.simplexml_iterator_current(clone.frame.payload0), Ok(None));
    assert_ne!(
        context
            .native_objects
            .get(source, HANDLE_SIMPLEXML)
            .expect("source remains live")
            .simplexml()
            .expect("source retains SimpleXML state")
            .pointer(),
        context
            .native_objects
            .get(clone.frame.payload0, HANDLE_SIMPLEXML)
            .expect("clone remains live")
            .simplexml()
            .expect("clone retains SimpleXML state")
            .pointer(),
    );

    context
        .release_simplexml_external(clone.frame.payload0)
        .expect("clone wrapper releases independently");
    context
        .clear_simplexml_iterator_current(source)
        .expect("source internal iterator owner releases");
    context
        .release_simplexml_external(current.frame.payload0)
        .expect("source current wrapper releases independently");
    context
        .release_simplexml_external(source)
        .expect("source wrapper releases independently");
}

/// Verifies request reset invalidates external and internal SimpleXML handles together.
#[test]
fn simplexml_foundation_context_reset_clears_iterator_ownership_graph() {
    let (graph, root, child) = unclaimed_xml_graph();
    let mut context = context();
    let parent = context.insert_simplexml_external(direct_view(
        root,
        Rc::clone(&graph),
        1001,
    ));
    let current = context
        .install_fresh_simplexml_iterator_current(
            parent,
            direct_view(child, graph, 1001),
        )
        .expect("iterator current installs");
    context.reset();
    assert!(matches!(
        context.native_objects.get(parent, HANDLE_SIMPLEXML),
        Err(HandleError::Stale)
    ));
    assert!(matches!(
        context.native_objects.get(current, HANDLE_SIMPLEXML),
        Err(HandleError::Stale)
    ));
    assert_eq!(context.native_objects.len(), 0);
}

/// Verifies generated wrapper retain/finalize calls accept and retire SimpleXML handles.
#[test]
fn simplexml_foundation_public_wrapper_finalizer_routes_to_balanced_release() {
    let (graph, root, _) = unclaimed_xml_graph();
    let mut native_context = context();
    let handle = native_context.insert_simplexml_external(direct_view(root, graph, 1101));
    let context_id = crate::context::register_context(native_context);

    let (retain_status, retained) = invoke_wrapper_operation(
        context_id,
        "internal:bridge.wrapper.retain",
        handle,
    );
    assert_eq!(retain_status, STATUS_OK);
    assert_eq!(retained.status, STATUS_OK);
    assert_eq!(retained.payload0, handle);
    crate::elephc_dom_result_release(context_id, retained.result_id);

    let (release_status, released) = invoke_wrapper_operation(
        context_id,
        "internal:bridge.wrapper.release",
        handle,
    );
    assert_eq!(release_status, STATUS_OK);
    assert_eq!(released.status, STATUS_OK);
    crate::elephc_dom_result_release(context_id, released.result_id);
    let registered = crate::context::context(context_id).expect("context stays registered");
    assert!(matches!(
        registered.borrow().native_objects.get(handle, HANDLE_SIMPLEXML),
        Err(HandleError::Stale)
    ));
    crate::context::remove_context(context_id);
}

/// Verifies string and local-file loaders publish fresh unclaimed SimpleXML graphs.
#[test]
fn simplexml_loaders_parse_string_and_local_file_into_fresh_views() {
    let context_id = crate::context::register_context(context());
    let source = b"<root><child/></root>";
    let string_request = decoded_request(
        "function:simplexml_load_string",
        0,
        &[bytes_value(source)],
        source,
    );
    let string_result = crate::dispatch::dispatch_reentrant(
        context_id,
        "function:simplexml_load_string",
        &string_request,
    )
    .expect("string loader dispatch succeeds")
    .expect("string loader is reentrant-routed");
    assert_eq!(string_result.frame.status, STATUS_OK);
    assert_eq!(string_result.value_tag, VALUE_BRIDGE_HANDLE);
    let string_handle = string_result.frame.payload0;

    let path = std::env::temp_dir().join(format!(
        "elephc-simplexml-loader-{}.xml",
        std::process::id()
    ));
    std::fs::write(&path, source).expect("local XML fixture is written");
    let path_bytes = path.to_string_lossy().into_owned().into_bytes();
    let file_request = decoded_request(
        "function:simplexml_load_file",
        0,
        &[bytes_value(&path_bytes)],
        &path_bytes,
    );
    let file_result = crate::dispatch::dispatch_reentrant(
        context_id,
        "function:simplexml_load_file",
        &file_request,
    )
    .expect("file loader dispatch succeeds")
    .expect("file loader is reentrant-routed");
    std::fs::remove_file(&path).expect("local XML fixture is removed");
    assert_eq!(file_result.frame.status, STATUS_OK);
    assert_eq!(file_result.value_tag, VALUE_BRIDGE_HANDLE);
    let file_handle = file_result.frame.payload0;
    assert_ne!(string_handle, file_handle);

    let registered =
        crate::context::context(context_id).expect("context stays registered");
    let registered = registered.borrow();
    let string = registered
        .native_objects
        .get(string_handle, HANDLE_SIMPLEXML)
        .expect("string view remains live")
        .simplexml()
        .expect("string handle stores SimpleXML");
    let file = registered
        .native_objects
        .get(file_handle, HANDLE_SIMPLEXML)
        .expect("file view remains live")
        .simplexml()
        .expect("file handle stores SimpleXML");
    assert_eq!(
        crate::native::node_name(string.pointer()).as_deref(),
        Some(b"root".as_slice())
    );
    assert_eq!(
        crate::native::node_name(file.pointer()).as_deref(),
        Some(b"root".as_slice())
    );
    assert_eq!(string.document().dom_api(), None);
    assert_eq!(file.document().dom_api(), None);
    assert!(!Rc::ptr_eq(&string.document(), &file.document()));
    drop(registered);
    crate::context::remove_context(context_id);
}

/// Verifies loader class-name validation and subclass discriminators match php-src.
#[test]
fn simplexml_loader_class_name_accepts_only_simplexml_subclasses() {
    let names = [
        b"SimpleXMLElement".as_slice(),
        b"CustomXml".as_slice(),
        b"stdClass".as_slice(),
    ];
    let definitions = [
        (100_u64, DOM_CLASS_NO_PARENT),
        (101_u64, 100_u64),
        (102_u64, DOM_CLASS_NO_PARENT),
    ];
    let entries = names
        .iter()
        .zip(definitions)
        .map(|(name, (class_id, parent_class_id))| DomClassMetadataEntry {
            name_ptr: name.as_ptr(),
            name_len: name.len() as u64,
            class_id,
            parent_class_id,
            is_abstract: 0,
            reserved: 0,
        })
        .collect::<Vec<_>>();
    let mut native_context = context();
    native_context
        .class_metadata
        .install(&entries)
        .expect("class metadata is valid");
    let context_id = crate::context::register_context(native_context);

    let source = b"<root/>";
    let class_name = b"CustomXml";
    let mut bytes = source.to_vec();
    bytes.extend_from_slice(class_name);
    let request = decoded_request(
        "function:simplexml_load_string",
        0,
        &[
            bytes_value_range(0, source.len()),
            bytes_value_range(source.len(), class_name.len()),
        ],
        &bytes,
    );
    let result = crate::dispatch::dispatch_reentrant(
        context_id,
        "function:simplexml_load_string",
        &request,
    )
    .expect("subclass loader dispatch succeeds")
    .expect("subclass loader is reentrant-routed");
    assert_eq!(result.frame.status, STATUS_OK);
    assert_eq!(result.frame.payload1, (1_u64 << 63) | 101);

    let invalid_name = b"stdClass";
    let mut invalid_bytes = source.to_vec();
    invalid_bytes.extend_from_slice(invalid_name);
    let invalid_request = decoded_request(
        "function:simplexml_load_string",
        0,
        &[
            bytes_value_range(0, source.len()),
            bytes_value_range(source.len(), invalid_name.len()),
        ],
        &invalid_bytes,
    );
    let invalid = crate::dispatch::dispatch_reentrant(
        context_id,
        "function:simplexml_load_string",
        &invalid_request,
    )
    .expect("invalid subclass dispatch completes")
    .expect("invalid subclass is reentrant-routed");
    assert_eq!(invalid.frame.status, STATUS_THROW);
    assert_eq!(invalid.frame.php_error_kind, PHP_ERROR_KIND_TYPE_ERROR);
    assert_eq!(
        &*invalid.frame.bytes,
        b"simplexml_load_string(): Argument #2 ($class_name) must be a class name derived from SimpleXMLElement or null, stdClass given"
    );
    crate::context::remove_context(context_id);
}

/// Verifies loader and constructor option narrowing use php-src's exact error classes.
#[test]
fn simplexml_loader_argument_limits_match_php_src_diagnostics() {
    let context_id = crate::context::register_context(context());
    let source = b"<root/>";
    let oversized = decoded_request(
        "function:simplexml_load_string",
        0,
        &[
            bytes_value(source),
            null_value(),
            integer_value(i64::MAX),
        ],
        source,
    );
    let oversized = crate::dispatch::dispatch_reentrant(
        context_id,
        "function:simplexml_load_string",
        &oversized,
    )
    .expect("oversized loader dispatch completes")
    .expect("oversized loader is reentrant-routed");
    assert_eq!(oversized.frame.status, STATUS_THROW);
    assert_eq!(
        oversized.frame.php_error_kind,
        PHP_ERROR_KIND_VALUE_ERROR
    );
    assert_eq!(
        &*oversized.frame.bytes,
        b"simplexml_load_string(): Argument #3 ($options) is too large"
    );

    let constructor = decoded_request(
        "method:simplexmlelement::__construct",
        0,
        &[bytes_value(source), integer_value(i64::MAX)],
        source,
    );
    let constructor = crate::dispatch::dispatch_reentrant(
        context_id,
        "method:simplexmlelement::__construct",
        &constructor,
    )
    .expect("invalid constructor options dispatch completes")
    .expect("constructor is reentrant-routed");
    assert_eq!(constructor.frame.status, STATUS_THROW);
    assert_eq!(constructor.frame.php_error_kind, PHP_ERROR_KIND_EXCEPTION);
    assert_eq!(
        &*constructor.frame.bytes,
        b"SimpleXMLElement::__construct(): Argument #2 ($options) is invalid"
    );

    let nul_path = b"bad\0path.xml";
    let file = decoded_request(
        "function:simplexml_load_file",
        0,
        &[bytes_value(nul_path)],
        nul_path,
    );
    let file = crate::dispatch::dispatch_reentrant(
        context_id,
        "function:simplexml_load_file",
        &file,
    )
    .expect("NUL path dispatch completes")
    .expect("file loader is reentrant-routed");
    assert_eq!(file.frame.status, STATUS_THROW);
    assert_eq!(file.frame.php_error_kind, PHP_ERROR_KIND_VALUE_ERROR);
    assert_eq!(
        &*file.frame.bytes,
        b"simplexml_load_file(): Argument #1 ($filename) must not contain any null bytes"
    );
    crate::context::remove_context(context_id);
}

/// Verifies recovered rootless documents retain ownership and method-specific diagnostics.
#[test]
fn simplexml_recovered_rootless_documents_match_php_method_contracts() {
    let source = b"X";
    for operation in [
        "function:simplexml_load_string",
        "method:simplexmlelement::__construct",
    ] {
        let context_id = crate::context::register_context(context());
        let values = if operation == "function:simplexml_load_string" {
            vec![
                bytes_value(source),
                null_value(),
                integer_value(1),
            ]
        } else {
            vec![bytes_value(source), integer_value(1)]
        };
        let request = decoded_request(operation, 0, &values, source);
        let loaded = crate::dispatch::dispatch_reentrant(
            context_id,
            operation,
            &request,
        )
        .expect("recovered rootless parse dispatch succeeds")
        .expect("SimpleXML parsing uses the reentrant route");
        assert_eq!(loaded.frame.status, STATUS_OK);
        assert_eq!(loaded.value_tag, VALUE_BRIDGE_HANDLE);
        assert!(!loaded.frame.diagnostics.is_empty());

        let registered =
            crate::context::context(context_id).expect("context stays registered");
        let document = {
            let context = registered.borrow();
            let object = context
                .native_objects
                .get(loaded.frame.payload0, HANDLE_SIMPLEXML)
                .expect("rootless wrapper remains live")
                .simplexml()
                .expect("loader result stores SimpleXML");
            assert_eq!(object.node_pointer(), None);
            let document = object.document();
            assert_eq!(crate::native::document_element(document.pointer()), None);
            document
        };
        assert_eq!(Rc::strong_count(&document), 2);

        let namespaces = decoded_request(
            "method:simplexmlelement::getdocnamespaces",
            loaded.frame.payload0,
            &[],
            &[],
        );
        let namespaces = crate::dispatch::dispatch(
            &mut registered.borrow_mut(),
            &namespaces,
        )
        .expect("rootless getDocNamespaces dispatch succeeds");
        assert_eq!(namespaces.frame.status, STATUS_OK);
        assert_eq!(namespaces.value_tag, VALUE_BOOL);
        assert_eq!(namespaces.frame.payload0, 0);

        let local_namespaces = decoded_request(
            "method:simplexmlelement::getdocnamespaces",
            loaded.frame.payload0,
            &[boolean_value(false), boolean_value(false)],
            &[],
        );
        let local_namespaces = crate::dispatch::dispatch(
            &mut registered.borrow_mut(),
            &local_namespaces,
        )
        .expect("rootless local getDocNamespaces returns a PHP Error");
        assert_eq!(local_namespaces.frame.status, STATUS_THROW);
        assert_eq!(local_namespaces.frame.php_error_kind, PHP_ERROR_KIND_ERROR);
        assert_eq!(
            &*local_namespaces.frame.bytes,
            b"SimpleXMLElement is not properly initialized",
        );

        let expression = b"BBBB";
        let xpath = decoded_request(
            "method:simplexmlelement::xpath",
            loaded.frame.payload0,
            &[bytes_value(expression)],
            expression,
        );
        let xpath = crate::dispatch::dispatch(
            &mut registered.borrow_mut(),
            &xpath,
        )
        .expect("rootless XPath dispatch returns a PHP Error");
        assert_eq!(xpath.frame.status, STATUS_THROW);
        assert_eq!(xpath.frame.php_error_kind, PHP_ERROR_KIND_ERROR);
        assert_eq!(
            &*xpath.frame.bytes,
            b"SimpleXMLElement is not properly initialized",
        );
        assert!(xpath.frame.diagnostics.is_empty());

        registered
            .borrow_mut()
            .release_simplexml_external(loaded.frame.payload0)
            .expect("rootless wrapper releases its document owner");
        assert!(matches!(
            registered
                .borrow()
                .native_objects
                .get(loaded.frame.payload0, HANDLE_SIMPLEXML),
            Err(HandleError::Stale)
        ));
        assert_eq!(Rc::strong_count(&document), 1);
        assert_eq!(crate::native::document_element(document.pointer()), None);
        drop(document);
        crate::context::remove_context(context_id);
    }
}

/// Verifies explicit constructor re-entry atomically replaces a rooted document by a rootless one.
#[test]
fn simplexml_recovered_constructor_reentry_balances_document_ownership() {
    let (original, root, _) = unclaimed_xml_graph();
    let mut native_context = context();
    let receiver = native_context.insert_simplexml_external(direct_view(
        root,
        Rc::clone(&original),
        1201,
    ));
    assert_eq!(Rc::strong_count(&original), 2);
    let context_id = crate::context::register_context(native_context);
    let source = b"X";
    let request = decoded_request(
        "method:simplexmlelement::__construct",
        receiver,
        &[bytes_value(source), integer_value(1)],
        source,
    );
    let result = crate::dispatch::dispatch_reentrant(
        context_id,
        "method:simplexmlelement::__construct",
        &request,
    )
    .expect("constructor re-entry dispatch succeeds")
    .expect("constructor re-entry uses the reentrant route");
    assert_eq!(result.frame.status, STATUS_OK);
    assert_eq!(result.value_tag, VALUE_NULL);
    assert!(!result.frame.diagnostics.is_empty());
    assert_eq!(Rc::strong_count(&original), 1);

    let registered =
        crate::context::context(context_id).expect("context stays registered");
    {
        let context = registered.borrow();
        let object = context
            .native_objects
            .get(receiver, HANDLE_SIMPLEXML)
            .expect("constructor receiver remains live")
            .simplexml()
            .expect("constructor receiver remains SimpleXML");
        assert_eq!(object.node_pointer(), None);
        assert_eq!(object.wrapper_kind(), 1201);
        assert_eq!(
            crate::native::document_element(object.document().pointer()),
            None,
        );
    }
    registered
        .borrow_mut()
        .release_simplexml_external(receiver)
        .expect("re-entered rootless wrapper releases cleanly");
    assert!(matches!(
        registered
            .borrow()
            .native_objects
            .get(receiver, HANDLE_SIMPLEXML),
        Err(HandleError::Stale)
    ));
    crate::context::remove_context(context_id);
}

/// Verifies loader namespace selectors survive parsing on the returned direct view.
#[test]
fn simplexml_loader_preserves_namespace_or_prefix_selector() {
    let context_id = crate::context::register_context(context());
    let source = b"<p:root xmlns:p=\"urn:test\"/>";
    let prefix = b"p";
    let mut bytes = source.to_vec();
    bytes.extend_from_slice(prefix);
    let request = decoded_request(
        "function:simplexml_load_string",
        0,
        &[
            bytes_value_range(0, source.len()),
            null_value(),
            integer_value(0),
            bytes_value_range(source.len(), prefix.len()),
            boolean_value(true),
        ],
        &bytes,
    );
    let result = crate::dispatch::dispatch_reentrant(
        context_id,
        "function:simplexml_load_string",
        &request,
    )
    .expect("namespace loader dispatch succeeds")
    .expect("namespace loader is reentrant-routed");
    let registered =
        crate::context::context(context_id).expect("context stays registered");
    let registered = registered.borrow();
    let object = registered
        .native_objects
        .get(result.frame.payload0, HANDLE_SIMPLEXML)
        .expect("namespace-filtered view remains live")
        .simplexml()
        .expect("loader result stores SimpleXML");
    assert_eq!(object.iterator().namespace_or_prefix(), Some(prefix.as_slice()));
    assert!(object.iterator().is_prefix());
    drop(registered);
    crate::context::remove_context(context_id);
}

/// Verifies a failed explicit constructor re-entry leaves the receiver graph untouched.
#[test]
fn simplexml_constructor_parse_failure_is_atomic_and_throws_php_exception() {
    let (graph, root, _) = unclaimed_xml_graph();
    let mut native_context = context();
    let receiver = native_context.insert_simplexml_external(direct_view(
        root,
        Rc::clone(&graph),
        1201,
    ));
    let context_id = crate::context::register_context(native_context);
    let malformed = b"<root>";
    let request = decoded_request(
        "method:simplexmlelement::__construct",
        receiver,
        &[bytes_value(malformed)],
        malformed,
    );
    let result = crate::dispatch::dispatch_reentrant(
        context_id,
        "method:simplexmlelement::__construct",
        &request,
    )
    .expect("constructor dispatch completes")
    .expect("constructor is reentrant-routed");
    assert_eq!(result.frame.status, STATUS_THROW);
    assert_eq!(result.frame.php_error_kind, PHP_ERROR_KIND_EXCEPTION);
    let message_len = result.frame.payload1 as usize;
    assert_eq!(
        &result.frame.bytes[..message_len],
        b"String could not be parsed as XML"
    );
    assert_eq!(result.frame.diagnostics.len(), 1);

    let registered =
        crate::context::context(context_id).expect("context stays registered");
    let registered = registered.borrow();
    let receiver_object = registered
        .native_objects
        .get(receiver, HANDLE_SIMPLEXML)
        .expect("failed constructor preserves receiver handle")
        .simplexml()
        .expect("receiver remains a SimpleXML view");
    assert_eq!(receiver_object.pointer(), root);
    assert!(Rc::ptr_eq(&receiver_object.document(), &graph));
    assert_eq!(receiver_object.wrapper_kind(), 1201);
    drop(registered);
    crate::context::remove_context(context_id);
}

/// Verifies DOM-to-SimpleXML imports are fresh and preserve exact warning classes.
#[test]
fn simplexml_import_dom_mints_fresh_views_and_distinguishes_invalid_nodes() {
    let (mut context, graph, document_handle, node_handle) = legacy_tree_context();
    let request = decoded_request(
        "function:simplexml_import_dom",
        0,
        &[bridge_value(node_handle)],
        &[],
    );
    let first = crate::dispatch::dispatch(&mut context, &request)
        .expect("first DOM import succeeds");
    let second = crate::dispatch::dispatch(&mut context, &request)
        .expect("second DOM import succeeds");
    assert_eq!(first.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(second.value_tag, VALUE_BRIDGE_HANDLE);
    assert_ne!(first.frame.payload0, second.frame.payload0);
    assert_eq!(graph.dom_api(), None);

    let node_pointer = context
        .native_objects
        .get(node_handle, HANDLE_NODE)
        .expect("canonical node remains live")
        .node()
        .expect("canonical handle stores a node")
        .pointer();
    let detached_handle = context.native_objects.insert(
        HANDLE_NODE,
        NativeObject::Node(NodeObject::without_owner_document(
            node_pointer,
            Rc::clone(&graph),
            101,
        )),
    );
    let detached_request = decoded_request(
        "function:simplexml_import_dom",
        0,
        &[bridge_value(detached_handle)],
        &[],
    );
    let detached = crate::dispatch::dispatch(&mut context, &detached_request)
        .expect("documentless import returns a PHP warning");
    assert_eq!(detached.value_tag, crate::abi::VALUE_NULL);
    assert_eq!(
        &*detached.frame.bytes,
        b"Warning: simplexml_import_dom(): Imported Node must have associated Document\n"
    );

    let empty_pointer = crate::native::document_new(b"1.0", b"")
        .expect("empty document allocation succeeds");
    let empty_document = DocumentObject::new(empty_pointer, DocumentFamily::Legacy);
    let empty_handle = context.native_objects.insert(
        HANDLE_DOCUMENT,
        NativeObject::Document(empty_document),
    );
    context.document_handles.insert(empty_pointer, empty_handle);
    let empty_request = decoded_request(
        "function:simplexml_import_dom",
        0,
        &[bridge_value(empty_handle)],
        &[],
    );
    let empty = crate::dispatch::dispatch(&mut context, &empty_request)
        .expect("empty-document import returns a PHP warning");
    assert_eq!(empty.value_tag, crate::abi::VALUE_NULL);
    assert_eq!(
        &*empty.frame.bytes,
        b"Warning: simplexml_import_dom(): Invalid Nodetype to import\n"
    );
    assert!(context
        .native_objects
        .get(document_handle, HANDLE_DOCUMENT)
        .is_ok());
}

/// Verifies modern import claims a legacy graph while returning its exact existing wrapper.
#[test]
fn modern_import_of_unclaimed_legacy_node_preserves_identity_and_locks_graph() {
    let (mut context, graph, _, node_handle) = legacy_tree_context();
    assert_eq!(graph.dom_api(), None);
    let modern_request = decoded_request(
        "function:dom\\import_simplexml",
        0,
        &[bridge_value(node_handle)],
        &[],
    );
    let modern = crate::dispatch::dispatch(&mut context, &modern_request)
        .expect("modern import succeeds");
    assert_eq!(modern.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(modern.frame.payload0, node_handle);
    assert_eq!(modern.frame.payload1, 101);
    assert_eq!(graph.dom_api(), Some(DomApiFamily::Modern));

    let repeated = crate::dispatch::dispatch(&mut context, &modern_request)
        .expect("repeated modern import succeeds");
    assert_eq!(repeated.frame.payload0, node_handle);
    assert_eq!(repeated.frame.payload1, 101);

    let legacy_request = decoded_request(
        "function:dom_import_simplexml",
        0,
        &[bridge_value(node_handle)],
        &[],
    );
    let legacy = crate::dispatch::dispatch(&mut context, &legacy_request)
        .expect("conflicting legacy import is a PHP error result");
    assert_eq!(legacy.frame.status, STATUS_THROW);
    assert_eq!(legacy.frame.php_error_kind, PHP_ERROR_KIND_TYPE_ERROR);
    assert_eq!(
        &*legacy.frame.bytes,
        b"dom_import_simplexml(): Argument #1 ($node) must not be already imported as a Dom\\Node"
    );
}

/// Verifies DOM imports export the first matching iterator element or attribute.
#[test]
fn dom_import_simplexml_resolves_non_destructive_iterator_views() {
    let parsed = crate::native::document_parse_xml(
        b"<root id=\"a\"><skip/><wanted/></root>",
        0,
        None,
        None,
    )
    .expect("iterator export fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let element_view = context.insert_simplexml_external(SimpleXmlObject::new(
        root,
        Rc::clone(&graph),
        0,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::Element,
            Some(b"wanted".to_vec()),
            None,
            false,
        ),
    ));
    let element_request = decoded_request(
        "function:dom_import_simplexml",
        0,
        &[bridge_value(element_view)],
        &[],
    );
    let element = crate::dispatch::dispatch(&mut context, &element_request)
        .expect("named-element view imports");
    assert_eq!(element.frame.payload1, 101);
    let element_pointer = context
        .native_objects
        .get(element.frame.payload0, HANDLE_NODE)
        .expect("element result remains live")
        .node()
        .expect("element result stores a node")
        .pointer();
    assert_eq!(
        crate::native::node_name(element_pointer).as_deref(),
        Some(b"wanted".as_slice())
    );

    let attribute_view = context.insert_simplexml_external(SimpleXmlObject::new(
        root,
        Rc::clone(&graph),
        0,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::AttrList,
            Some(b"id".to_vec()),
            None,
            false,
        ),
    ));
    let attribute_request = decoded_request(
        "function:dom_import_simplexml",
        0,
        &[bridge_value(attribute_view)],
        &[],
    );
    let attribute = crate::dispatch::dispatch(&mut context, &attribute_request)
        .expect("named-attribute view imports");
    assert_eq!(attribute.frame.payload1, 102);
    let attribute_pointer = context
        .native_objects
        .get(attribute.frame.payload0, HANDLE_NODE)
        .expect("attribute result remains live")
        .node()
        .expect("attribute result stores a node")
        .pointer();
    assert_eq!(
        crate::native::node_name(attribute_pointer).as_deref(),
        Some(b"id".as_slice())
    );
}

/// Verifies namespace methods export an exact alternating map ABI range.
#[test]
fn simplexml_namespace_results_use_map_records() {
    let parsed = crate::native::document_parse_xml(
        b"<root xmlns=\"urn:d\" xmlns:p=\"urn:p\"/>",
        0,
        None,
        None,
    )
    .expect("namespace fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let receiver = context.insert_simplexml_external(direct_view(root, graph, 0));
    let request = decoded_request(
        "method:simplexmlelement::getdocnamespaces",
        receiver,
        &[],
        &[],
    );
    let result = crate::dispatch::dispatch(&mut context, &request)
        .expect("namespace dispatch succeeds");
    assert_eq!(result.value_tag, VALUE_MAP);
    assert_eq!(result.frame.payload0, 0);
    assert_eq!(result.frame.payload1, 2);
    assert_eq!(result.frame.values.len(), 4);
    assert!(result
        .frame
        .values
        .iter()
        .all(|value| value.tag == VALUE_BYTES));
}

/// Verifies omitted and explicit-empty namespace selectors both create unfiltered views.
#[test]
fn simplexml_empty_namespace_selectors_match_omission() {
    let (graph, root, _) = unclaimed_xml_graph();
    let mut context = context();
    let receiver = context.insert_simplexml_external(direct_view(root, graph, 0));
    for operation in [
        "method:simplexmlelement::children",
        "method:simplexmlelement::attributes",
    ] {
        let omitted = decoded_request(operation, receiver, &[], &[]);
        let omitted = crate::dispatch::dispatch(&mut context, &omitted)
            .expect("omitted selector dispatch succeeds");
        let empty = decoded_request(
            operation,
            receiver,
            &[bytes_value(b"")],
            b"",
        );
        let empty = crate::dispatch::dispatch(&mut context, &empty)
            .expect("empty selector dispatch succeeds");
        for result in [omitted, empty] {
            assert_eq!(result.value_tag, VALUE_BRIDGE_HANDLE);
            let view = context
                .native_objects
                .get(result.frame.payload0, HANDLE_SIMPLEXML)
                .expect("fresh view remains registered")
                .simplexml()
                .expect("fresh view is SimpleXML");
            assert_eq!(view.iterator().namespace_or_prefix(), None);
        }
    }
}

/// Verifies null passed to SimpleXML boolean parameters emits ordered PHP deprecations.
#[test]
fn simplexml_nullable_bool_deprecations_match_php() {
    let (graph, root, _) = unclaimed_xml_graph();
    let mut context = context();
    let receiver = context.insert_simplexml_external(direct_view(root, graph, 0));
    let cases: [(&str, Vec<Value>, &[&[u8]]); 4] = [
        (
            "method:simplexmlelement::children",
            vec![null_value(), null_value()],
            &[b"Deprecated: SimpleXMLElement::children(): Passing null to parameter #2 ($isPrefix) of type bool is deprecated\n"],
        ),
        (
            "method:simplexmlelement::attributes",
            vec![null_value(), null_value()],
            &[b"Deprecated: SimpleXMLElement::attributes(): Passing null to parameter #2 ($isPrefix) of type bool is deprecated\n"],
        ),
        (
            "method:simplexmlelement::getdocnamespaces",
            vec![null_value(), null_value()],
            &[
                b"Deprecated: SimpleXMLElement::getDocNamespaces(): Passing null to parameter #1 ($recursive) of type bool is deprecated\n",
                b"Deprecated: SimpleXMLElement::getDocNamespaces(): Passing null to parameter #2 ($fromRoot) of type bool is deprecated\n",
            ],
        ),
        (
            "method:simplexmlelement::getnamespaces",
            vec![null_value()],
            &[b"Deprecated: SimpleXMLElement::getNamespaces(): Passing null to parameter #1 ($recursive) of type bool is deprecated\n"],
        ),
    ];
    for (operation, values, expected) in cases {
        let request = decoded_request(operation, receiver, &values, &[]);
        let result = crate::dispatch::dispatch(&mut context, &request)
            .expect("nullable bool dispatch succeeds");
        assert_eq!(result.frame.diagnostics.len(), expected.len());
        for (diagnostic, expected) in
            result.frame.diagnostics.iter().zip(expected.iter())
        {
            let start = diagnostic.message_offset as usize;
            let end = start + diagnostic.message_len as usize;
            assert_eq!(&result.frame.bytes[start..end], *expected);
        }
    }
}

/// Verifies `addChild()` retains php-src's local-name and QName-prefix view metadata.
#[test]
fn simplexml_add_child_preserves_qname_iterator_state() {
    let parsed = crate::native::document_parse_xml(
        b"<root xmlns:p=\"urn:p\"/>",
        0,
        None,
        None,
    )
    .expect("addChild fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let receiver = context.insert_simplexml_external(direct_view(root, graph, 1201));
    let request_bytes = b"p:cPurn:p";
    let request = decoded_request(
        "method:simplexmlelement::addchild",
        receiver,
        &[
            bytes_value_range(0, 3),
            bytes_value_range(3, 1),
            bytes_value_range(4, 5),
        ],
        request_bytes,
    );
    let result = crate::dispatch::dispatch(&mut context, &request)
        .expect("addChild dispatch succeeds");
    assert_eq!(result.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(result.frame.payload1, 1201);
    let child = context
        .native_objects
        .get(result.frame.payload0, HANDLE_SIMPLEXML)
        .expect("added child wrapper remains live")
        .simplexml()
        .expect("added child result is SimpleXML");
    assert_eq!(child.iterator().kind(), SimpleXmlIteratorType::None);
    assert_eq!(child.iterator().name(), Some(b"c".as_slice()));
    assert_eq!(
        child.iterator().namespace_or_prefix(),
        Some(b"p".as_slice())
    );
    assert!(!child.iterator().is_prefix());
    assert_eq!(
        crate::native::node_prefix(child.pointer()).as_deref(),
        Some(b"p".as_slice())
    );
}

/// Verifies `__debugInfo()` flattens attributes, duplicates, and fresh subclass wrappers.
#[test]
fn simplexml_debug_info_exports_recursive_php_shape() {
    let parsed = crate::native::document_parse_xml(
        b"<r id=\"7\"><a><b>B</b></a><a>A2</a><c>C</c></r>",
        0,
        None,
        None,
    )
    .expect("debug-info fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let receiver =
        context.insert_simplexml_external(direct_view(root, graph, 1201));
    let request = decoded_request(
        "method:simplexmlelement::__debuginfo",
        receiver,
        &[],
        &[],
    );
    let first = crate::dispatch::dispatch(&mut context, &request)
        .expect("first debug-info dispatch succeeds");
    let second = crate::dispatch::dispatch(&mut context, &request)
        .expect("second debug-info dispatch succeeds");

    for result in [&first, &second] {
        assert_eq!(result.value_tag, VALUE_MAP);
        assert_eq!(result.frame.payload0, 0);
        assert_eq!(result.frame.payload1, 3);
        assert!(result.frame.values.len() >= 10);
        let bytes = |value: &Value| {
            let start = value.payload0 as usize;
            let end = start + value.payload1 as usize;
            &result.frame.bytes[start..end]
        };
        assert_eq!(bytes(&result.frame.values[0]), b"@attributes");
        assert_eq!(result.frame.values[1].tag, VALUE_MAP);
        let attributes_start = result.frame.values[1].payload0 as usize;
        assert_eq!(result.frame.values[1].payload1, 1);
        assert_eq!(bytes(&result.frame.values[attributes_start]), b"id");
        assert_eq!(bytes(&result.frame.values[attributes_start + 1]), b"7");
        assert_eq!(bytes(&result.frame.values[2]), b"a");
        assert_eq!(result.frame.values[3].tag, VALUE_ARRAY);
        let duplicates_start = result.frame.values[3].payload0 as usize;
        assert_eq!(result.frame.values[3].payload1, 2);
        assert_eq!(
            result.frame.values[duplicates_start].tag,
            VALUE_BRIDGE_HANDLE
        );
        assert_eq!(result.frame.values[duplicates_start].payload1, 1201);
        assert_eq!(
            bytes(&result.frame.values[duplicates_start + 1]),
            b"A2"
        );
        assert_eq!(bytes(&result.frame.values[4]), b"c");
        assert_eq!(bytes(&result.frame.values[5]), b"C");
    }

    let first_duplicates = first.frame.values[3].payload0 as usize;
    let second_duplicates = second.frame.values[3].payload0 as usize;
    assert_ne!(
        first.frame.values[first_duplicates].payload0,
        second.frame.values[second_duplicates].payload0
    );

    let receiver_graph = context
        .native_objects
        .get(receiver, HANDLE_SIMPLEXML)
        .expect("receiver remains live")
        .simplexml()
        .expect("receiver remains SimpleXML")
        .document();
    let missing = context.insert_simplexml_external(SimpleXmlObject::new(
        root,
        receiver_graph,
        1201,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::Element,
            Some(b"missing".to_vec()),
            None,
            false,
        ),
    ));
    let empty_request = decoded_request(
        "method:simplexmlelement::__debuginfo",
        missing,
        &[],
        &[],
    );
    let empty = crate::dispatch::dispatch(&mut context, &empty_request)
        .expect("missing view debug-info dispatch succeeds");
    assert_eq!(empty.value_tag, VALUE_MAP);
    assert_eq!(empty.frame.payload1, 0);
    assert!(empty.frame.values.is_empty());
}

/// Verifies named element views project the selected node's children and attributes.
#[test]
fn simplexml_debug_info_projects_content_and_file_element_views() {
    let parsed = crate::native::document_parse_xml(
        b"<pres><content><file glob=\"slide_*.xml\"/></content></pres>",
        0,
        None,
        None,
    )
    .expect("element-view debug fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let content = crate::native::node_first_child(root).expect("fixture has content");
    let file = crate::native::node_first_child(content).expect("fixture has file");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let content_view = context.insert_simplexml_external(SimpleXmlObject::new(
        root,
        Rc::clone(&graph),
        1201,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::Element,
            Some(b"content".to_vec()),
            None,
            false,
        ),
    ));
    let file_view = context.insert_simplexml_external(SimpleXmlObject::new(
        content,
        graph,
        1201,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::Element,
            Some(b"file".to_vec()),
            None,
            false,
        ),
    ));

    let content_request = decoded_request(
        "method:simplexmlelement::__debuginfo",
        content_view,
        &[],
        &[],
    );
    let content_debug = crate::dispatch::dispatch(&mut context, &content_request)
        .expect("content-view debug-info dispatch succeeds");
    let content_bytes = |value: &Value| {
        let start = value.payload0 as usize;
        let end = start + value.payload1 as usize;
        &content_debug.frame.bytes[start..end]
    };
    assert_eq!(content_debug.value_tag, VALUE_MAP);
    assert_eq!(content_debug.frame.payload1, 1);
    assert_eq!(content_bytes(&content_debug.frame.values[0]), b"file");
    assert_eq!(content_debug.frame.values[1].tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(content_debug.frame.values[1].payload1, 1201);
    let nested_file = context
        .native_objects
        .get(content_debug.frame.values[1].payload0, HANDLE_SIMPLEXML)
        .expect("content projection retains its nested wrapper")
        .simplexml()
        .expect("nested projection is SimpleXML");
    assert_eq!(nested_file.pointer(), file);

    let file_request = decoded_request(
        "method:simplexmlelement::__debuginfo",
        file_view,
        &[],
        &[],
    );
    let file_debug = crate::dispatch::dispatch(&mut context, &file_request)
        .expect("file-view debug-info dispatch succeeds");
    let file_bytes = |value: &Value| {
        let start = value.payload0 as usize;
        let end = start + value.payload1 as usize;
        &file_debug.frame.bytes[start..end]
    };
    assert_eq!(file_debug.value_tag, VALUE_MAP);
    assert_eq!(file_debug.frame.payload1, 1);
    assert_eq!(file_bytes(&file_debug.frame.values[0]), b"@attributes");
    assert_eq!(file_debug.frame.values[1].tag, VALUE_MAP);
    let attributes = file_debug.frame.values[1].payload0 as usize;
    assert_eq!(file_debug.frame.values[1].payload1, 1);
    assert_eq!(file_bytes(&file_debug.frame.values[attributes]), b"glob");
    assert_eq!(
        file_bytes(&file_debug.frame.values[attributes + 1]),
        b"slide_*.xml"
    );
}

/// Verifies a repeated named-element view keeps php-src's series projection mode.
#[test]
fn simplexml_debug_info_projects_repeated_element_view_as_series() {
    let parsed = crate::native::document_parse_xml(
        b"<r><file a=\"1\"><child/></file><file a=\"2\">two</file></r>",
        0,
        None,
        None,
    )
    .expect("repeated element-view debug fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let first_file = crate::native::node_first_child(root).expect("fixture has files");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let receiver = context.insert_simplexml_external(SimpleXmlObject::new(
        root,
        graph,
        1201,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::Element,
            Some(b"file".to_vec()),
            None,
            false,
        ),
    ));
    let request = decoded_request(
        "method:simplexmlelement::__debuginfo",
        receiver,
        &[],
        &[],
    );
    let debug = crate::dispatch::dispatch(&mut context, &request)
        .expect("repeated element-view debug-info dispatch succeeds");
    let bytes = |value: &Value| {
        let start = value.payload0 as usize;
        let end = start + value.payload1 as usize;
        &debug.frame.bytes[start..end]
    };

    assert_eq!(debug.value_tag, VALUE_MAP);
    assert_eq!(debug.frame.payload1, 3);
    assert_eq!(bytes(&debug.frame.values[0]), b"@attributes");
    assert_eq!(debug.frame.values[1].tag, VALUE_MAP);
    let attributes = debug.frame.values[1].payload0 as usize;
    assert_eq!(bytes(&debug.frame.values[attributes]), b"a");
    assert_eq!(bytes(&debug.frame.values[attributes + 1]), b"1");
    assert_eq!(debug.frame.values[2].tag, VALUE_INT);
    assert_eq!(debug.frame.values[2].payload0, 0);
    assert_eq!(debug.frame.values[3].tag, VALUE_BRIDGE_HANDLE);
    let first_wrapper = context
        .native_objects
        .get(debug.frame.values[3].payload0, HANDLE_SIMPLEXML)
        .expect("series projection retains its first wrapper")
        .simplexml()
        .expect("series projection wrapper is SimpleXML");
    assert_eq!(first_wrapper.pointer(), first_file);
    assert_eq!(debug.frame.values[4].tag, VALUE_INT);
    assert_eq!(debug.frame.values[4].payload0, 1);
    assert_eq!(bytes(&debug.frame.values[5]), b"two");
}

/// Verifies debug projection retains php-src's empty comment and PI wrappers.
#[test]
fn simplexml_debug_info_keeps_named_empty_non_element_nodes() {
    let parsed = crate::native::document_parse_xml(
        b"<r><!-- c --><branch><?test data?></branch></r>",
        0,
        None,
        None,
    )
    .expect("named non-element fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let receiver = context.insert_simplexml_external(direct_view(root, graph, 0));
    let root_request = decoded_request(
        "method:simplexmlelement::__debuginfo",
        receiver,
        &[],
        &[],
    );
    let root_debug = crate::dispatch::dispatch(&mut context, &root_request)
        .expect("root debug-info dispatch succeeds");
    let root_bytes = |value: &Value| {
        let start = value.payload0 as usize;
        let end = start + value.payload1 as usize;
        &root_debug.frame.bytes[start..end]
    };

    assert_eq!(root_debug.frame.payload1, 2);
    assert_eq!(root_bytes(&root_debug.frame.values[0]), b"comment");
    assert_eq!(root_debug.frame.values[1].tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(root_bytes(&root_debug.frame.values[2]), b"branch");
    assert_eq!(root_debug.frame.values[3].tag, VALUE_BRIDGE_HANDLE);

    let comment_handle = root_debug.frame.values[1].payload0;
    let branch_handle = root_debug.frame.values[3].payload0;
    let comment_request = decoded_request(
        "method:simplexmlelement::__debuginfo",
        comment_handle,
        &[],
        &[],
    );
    let comment_debug = crate::dispatch::dispatch(&mut context, &comment_request)
        .expect("comment debug-info dispatch succeeds");
    assert_eq!(comment_debug.frame.payload1, 0);

    let branch_request = decoded_request(
        "method:simplexmlelement::__debuginfo",
        branch_handle,
        &[],
        &[],
    );
    let branch_debug = crate::dispatch::dispatch(&mut context, &branch_request)
        .expect("branch debug-info dispatch succeeds");
    let pi_key = branch_debug.frame.values[0];
    let pi_start = pi_key.payload0 as usize;
    let pi_end = pi_start + pi_key.payload1 as usize;
    assert_eq!(branch_debug.frame.payload1, 1);
    assert_eq!(&branch_debug.frame.bytes[pi_start..pi_end], b"test");
    assert_eq!(branch_debug.frame.values[1].tag, VALUE_BRIDGE_HANDLE);

    let pi_request = decoded_request(
        "method:simplexmlelement::__debuginfo",
        branch_debug.frame.values[1].payload0,
        &[],
        &[],
    );
    let pi_debug = crate::dispatch::dispatch(&mut context, &pi_request)
        .expect("processing-instruction debug-info dispatch succeeds");
    assert_eq!(pi_debug.frame.payload1, 0);
}

/// Verifies SimpleXML string/debug values concatenate only direct text siblings.
#[test]
fn simplexml_inline_text_excludes_nested_element_content() {
    let parsed = crate::native::document_parse_xml(
        b"<r><e>outer<b>nested</b>tail</e></r>",
        0,
        None,
        None,
    )
    .expect("inline-text fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let child = crate::native::node_first_child(root).expect("fixture has a child");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let root_handle = context.insert_simplexml_external(direct_view(
        root,
        Rc::clone(&graph),
        0,
    ));
    let child_handle = context.insert_simplexml_external(direct_view(
        child,
        graph,
        0,
    ));

    let debug_request = decoded_request(
        "method:simplexmlelement::__debuginfo",
        root_handle,
        &[],
        &[],
    );
    let debug = crate::dispatch::dispatch(&mut context, &debug_request)
        .expect("inline debug-info dispatch succeeds");
    assert_eq!(debug.value_tag, VALUE_MAP);
    assert_eq!(debug.frame.payload1, 1);
    let debug_value = debug.frame.values[1];
    let start = debug_value.payload0 as usize;
    let end = start + debug_value.payload1 as usize;
    assert_eq!(&debug.frame.bytes[start..end], b"outertail");

    let string_request = decoded_request(
        "method:simplexmlelement::__tostring",
        child_handle,
        &[],
        &[],
    );
    let string = crate::dispatch::dispatch(&mut context, &string_request)
        .expect("inline toString dispatch succeeds");
    assert_eq!(string.value_tag, VALUE_BYTES);
    assert_eq!(&*string.frame.bytes, b"outertail");
}

/// Verifies SimpleXML subnode serialization uses php-src's namespace-preserving dump.
#[test]
fn simplexml_as_xml_uses_php_node_dump() {
    let parsed = crate::native::document_parse_xml(
        b"<root xmlns:p=\"urn:p\"><p:item>one</p:item></root>",
        0,
        None,
        None,
    )
    .expect("asXML fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let child = crate::native::node_first_child(root).expect("fixture has a child");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let receiver = context.insert_simplexml_external(direct_view(
        child,
        Rc::clone(&graph),
        0,
    ));

    let string_request = decoded_request(
        "method:simplexmlelement::asxml",
        receiver,
        &[],
        &[],
    );
    let string_result = crate::dispatch::dispatch(&mut context, &string_request)
        .expect("string asXML dispatch succeeds");
    assert_eq!(string_result.value_tag, VALUE_BYTES);
    assert_eq!(&*string_result.frame.bytes, b"<p:item>one</p:item>");

    graph
        .claim_dom_api(DomApiFamily::Modern)
        .expect("modern DOM claim succeeds");
    let modern_result = crate::dispatch::dispatch(&mut context, &string_request)
        .expect("modern-claimed string asXML dispatch succeeds");
    assert_eq!(modern_result.value_tag, VALUE_BYTES);
    assert_eq!(
        &*modern_result.frame.bytes,
        b"<p:item xmlns:p=\"urn:p\">one</p:item>"
    );
}

/// Verifies XPath registration failures and php-src's exact supported node filter.
#[test]
fn simplexml_xpath_registration_and_node_filter_match_php() {
    let parsed = crate::native::document_parse_xml(
        b"<root xmlns:p=\"urn:p\"><p:item/><p:item/></root>",
        0,
        None,
        None,
    )
    .expect("XPath fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let receiver = context.insert_simplexml_external(direct_view(root, graph, 1201));

    let empty_prefix = decoded_request(
        "method:simplexmlelement::registerxpathnamespace",
        receiver,
        &[bytes_value_range(0, 0), bytes_value_range(0, 5)],
        b"urn:p",
    );
    let empty_prefix = crate::dispatch::dispatch(&mut context, &empty_prefix)
        .expect("empty-prefix registration returns false");
    assert_eq!(empty_prefix.value_tag, VALUE_BOOL);
    assert_eq!(empty_prefix.frame.payload0, 0);

    let registration = decoded_request(
        "method:simplexmlelement::registerxpathnamespace",
        receiver,
        &[bytes_value_range(0, 1), bytes_value_range(1, 5)],
        b"purn:p",
    );
    let registration = crate::dispatch::dispatch(&mut context, &registration)
        .expect("valid namespace registration succeeds");
    assert_eq!(registration.frame.payload0, 1);

    for expression in [b"/".as_slice(), b"//namespace::*".as_slice()] {
        let request = decoded_request(
            "method:simplexmlelement::xpath",
            receiver,
            &[bytes_value(expression)],
            expression,
        );
        let result = crate::dispatch::dispatch(&mut context, &request)
            .expect("unsupported XPath node kinds are filtered");
        assert_eq!(result.value_tag, VALUE_ARRAY);
        assert_eq!(result.frame.payload1, 0);
        assert!(result.frame.values.is_empty());
    }

    let expression = b"//p:item";
    let request = decoded_request(
        "method:simplexmlelement::xpath",
        receiver,
        &[bytes_value(expression)],
        expression,
    );
    let first = crate::dispatch::dispatch(&mut context, &request)
        .expect("first node-set evaluation succeeds");
    let second = crate::dispatch::dispatch(&mut context, &request)
        .expect("second node-set evaluation succeeds");
    assert_eq!(first.frame.payload1, 2);
    assert_eq!(second.frame.payload1, 2);
    assert!(first
        .frame
        .values
        .iter()
        .all(|value| value.tag == VALUE_BRIDGE_HANDLE && value.payload1 == 1201));
    assert_ne!(first.frame.values[0].payload0, second.frame.values[0].payload0);
}

/// Verifies scalar and invalid XPath warnings retain their method text for call-site decoration.
#[test]
fn simplexml_xpath_warning_details_request_location_only() {
    let parsed = crate::native::document_parse_xml(b"<root/>", 0, None, None)
        .expect("XPath warning fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let receiver = context.insert_simplexml_external(direct_view(root, graph, 0));

    for (expression, expected) in [
        (
            b"***".as_slice(),
            b"Warning: SimpleXMLElement::xpath(): XPath expression must return a node set, number returned".as_slice(),
        ),
        (
            b"**".as_slice(),
            b"Warning: SimpleXMLElement::xpath(): Invalid expression".as_slice(),
        ),
    ] {
        let request = decoded_request(
            "method:simplexmlelement::xpath",
            receiver,
            &[bytes_value(expression)],
            expression,
        );
        let result = crate::dispatch::dispatch(&mut context, &request)
            .expect("XPath warning dispatch succeeds");
        assert_eq!(result.value_tag, VALUE_BOOL);
        assert_eq!(result.frame.payload0, 0);
        assert_eq!(result.frame.diagnostics.len(), 1);
        let diagnostic = result.frame.diagnostics[0];
        assert_eq!(diagnostic.reserved, DIAGNOSTIC_FLAG_CALLSITE_LOCATION);
        let start = diagnostic.message_offset as usize;
        let end = start + diagnostic.message_len as usize;
        assert_eq!(&result.frame.bytes[start..end], expected);
    }
}

/// Verifies mutator warnings retain full method details for call-site decoration.
#[test]
fn simplexml_add_mutator_warnings_request_location_only() {
    let parsed = crate::native::document_parse_xml(b"<root id=\"1\"/>", 0, None, None)
        .expect("mutator warning fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let receiver = context.insert_simplexml_external(direct_view(root, graph, 0));

    let duplicate_bytes = b"id2";
    let duplicate = decoded_request(
        "method:simplexmlelement::addattribute",
        receiver,
        &[bytes_value_range(0, 2), bytes_value_range(2, 1)],
        duplicate_bytes,
    );
    let duplicate = crate::dispatch::dispatch(&mut context, &duplicate)
        .expect("duplicate attribute warning dispatch succeeds");
    assert_location_only_warning(
        &duplicate,
        b"Warning: SimpleXMLElement::addAttribute(): Attribute already exists",
    );

    let attributes = decoded_request(
        "method:simplexmlelement::attributes",
        receiver,
        &[],
        &[],
    );
    let attributes = crate::dispatch::dispatch(&mut context, &attributes)
        .expect("attribute-list view dispatch succeeds");
    let child_bytes = b"child";
    let child = decoded_request(
        "method:simplexmlelement::addchild",
        attributes.frame.payload0,
        &[bytes_value(child_bytes)],
        child_bytes,
    );
    let child = crate::dispatch::dispatch(&mut context, &child)
        .expect("attribute-list child warning dispatch succeeds");
    assert_location_only_warning(
        &child,
        b"Warning: SimpleXMLElement::addChild(): Cannot add element to attributes",
    );
}

/// Asserts one bridge result carries exactly one location-only warning detail.
fn assert_location_only_warning(result: &crate::dispatch::DispatchResult, expected: &[u8]) {
    assert_eq!(result.value_tag, VALUE_NULL);
    assert_eq!(result.frame.diagnostics.len(), 1);
    let diagnostic = result.frame.diagnostics[0];
    assert_eq!(diagnostic.reserved, DIAGNOSTIC_FLAG_CALLSITE_LOCATION);
    let start = diagnostic.message_offset as usize;
    let end = start + diagnostic.message_len as usize;
    assert_eq!(&result.frame.bytes[start..end], expected);
}
