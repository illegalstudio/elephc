//! Purpose:
//! Direct bridge tests for the locked SimpleXML object-handler semantics.
//! Covers filtered views, scalar access, BP_VAR modes, mutation, and detached liveness.
//!
//! Called from:
//! - `cargo test -p elephc-dom simplexml_object_handler` through Rust's test harness.
//!
//! Key details:
//! - Fixtures use the exact native libxml2 graph and generation-checked handles.
//! - Expected edge cases are anchored to the frozen PHP 8.5.8 oracle.

use crate::abi::{
    RequestHeader, Value, ABI_VERSION, DIAGNOSTIC_FLAG_CALLSITE_CONTEXT,
    PHP_ERROR_KIND_TYPE_ERROR, REQUEST_FLAG_ARGUMENT_COUNT, STATUS_THROW, VALUE_ARRAY,
    VALUE_BOOL, VALUE_BRIDGE_HANDLE, VALUE_BYTES, VALUE_FLOAT, VALUE_INT, VALUE_NULL,
    VALUE_MAP, VALUE_SIMPLEXML_APPEND,
};
use crate::context::{Context, Host};
use crate::objects::{
    DocumentGraph, SimpleXmlIteratorState, SimpleXmlIteratorType, SimpleXmlObject,
};

use super::{
    cast, compare_dispatch, count_dispatch, get_iterator_dispatch, has_dimension_dispatch,
    has_property_dispatch, read_dimension_dispatch, read_property_dispatch,
    unset_dimension_dispatch, unset_property_dispatch, write_dimension_dispatch,
    write_property_dispatch,
};

/// Verifies the iterator handler returns the exact receiver without creating a fresh view.
#[test]
fn simplexml_object_handler_get_iterator_preserves_receiver_identity() {
    let (mut context, root) = fixture(b"<r><a/></r>", 0x8000_0042);
    for _ in 0..2 {
        let result = get_iterator_dispatch(
            &mut context,
            &request(
                "object-handler:simplexml::get_iterator",
                root,
                0,
                &[],
                &[],
            ),
        )
        .expect("iterator handler succeeds");
        assert_eq!(result.value_tag, VALUE_BRIDGE_HANDLE);
        assert_eq!(result.frame.payload0, root);
        assert_eq!(result.frame.payload1, 0x8000_0042);
        assert_eq!(context.simplexml_iterator_current(root), Ok(None));
    }
}

/// Builds one callback-free bridge context suitable for native handler tests.
fn context() -> Context {
    Context::new(Host {
        user_data: 0,
        call: None,
    })
}

/// Parses XML and inserts one externally owned direct SimpleXML root wrapper.
fn fixture(xml: &[u8], wrapper_kind: u64) -> (Context, u64) {
    let parsed = crate::native::document_parse_xml(xml, 0, None, None)
        .expect("handler fixture parses");
    let document = parsed.document.expect("fixture owns a document");
    let root = crate::native::document_element(document).expect("fixture has a root");
    let graph = DocumentGraph::new_unclaimed_xml(document);
    let mut context = context();
    let handle = context.insert_simplexml_external(SimpleXmlObject::new(
        root,
        graph,
        wrapper_kind,
        SimpleXmlIteratorState::direct(None, false),
    ));
    (context, handle)
}

/// Encodes one byte-string value into the shared request byte section.
fn push_bytes(bytes: &mut Vec<u8>, value: &[u8]) -> Value {
    let offset = bytes.len();
    bytes.extend_from_slice(value);
    Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: offset as u64,
        payload1: value.len() as u64,
    }
}

/// Builds one signed integer ABI value.
fn integer(value: i64) -> Value {
    Value {
        tag: VALUE_INT,
        flags: 0,
        payload0: value as u64,
        payload1: 0,
    }
}

/// Builds the ABI-only marker emitted for a PHP empty SimpleXML append dimension.
fn append_offset() -> Value {
    Value {
        tag: VALUE_SIMPLEXML_APPEND,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }
}

/// Builds one strict boolean ABI value.
fn boolean(value: bool) -> Value {
    Value {
        tag: VALUE_BOOL,
        flags: 0,
        payload0: u64::from(value),
        payload1: 0,
    }
}

/// Builds one PHP null ABI value.
fn null() -> Value {
    Value {
        tag: VALUE_NULL,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }
}

/// Builds one bridge-handle ABI value.
fn bridge_handle(value: u64) -> Value {
    Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: value,
        payload1: 0,
    }
}

/// Decodes a production request with an explicit root-argument count.
fn request(
    operation: &str,
    receiver: u64,
    root_count: usize,
    values: &[Value],
    bytes: &[u8],
) -> crate::request::Request {
    let opcode = crate::generated::opcodes::OPERATIONS
        .iter()
        .find_map(|(opcode, key)| (*key == operation).then_some(*opcode))
        .expect("handler opcode exists");
    let header = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: std::mem::size_of::<RequestHeader>() as u32,
        opcode,
        flags: REQUEST_FLAG_ARGUMENT_COUNT | root_count as u32,
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
        .expect("handler request decodes")
}

/// Extracts the handle from one typed bridge result.
fn result_handle(result: crate::dispatch::DispatchResult) -> (u64, u64) {
    assert_eq!(result.value_tag, VALUE_BRIDGE_HANDLE);
    (result.frame.payload0, result.frame.payload1)
}

/// Reads one named child through ordinary BP_VAR_R semantics.
fn read_property(
    context: &mut Context,
    receiver: u64,
    name: &[u8],
) -> crate::dispatch::DispatchResult {
    let mut bytes = Vec::new();
    let values = [push_bytes(&mut bytes, name)];
    read_property_dispatch(
        context,
        &request(
            "object-handler:simplexml::read_property",
            receiver,
            1,
            &values,
            &bytes,
        ),
    )
    .expect("property read succeeds")
}

/// Reads one named child with BP_VAR_W semantics, optionally preserving an empty terminal append view.
fn read_property_for_write(
    context: &mut Context,
    receiver: u64,
    name: &[u8],
    append_target: bool,
) -> crate::dispatch::DispatchResult {
    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, name),
        integer(1),
        boolean(true),
        boolean(append_target),
    ];
    read_property_dispatch(
        context,
        &request(
            "object-handler:simplexml::read_property",
            receiver,
            values.len(),
            &values,
            &bytes,
        ),
    )
    .expect("property write read succeeds")
}

/// Reads one numeric dimension with an optional BP_VAR mode.
fn read_index(
    context: &mut Context,
    receiver: u64,
    index: i64,
    mode: Option<i64>,
) -> crate::dispatch::DispatchResult {
    let mut values = vec![integer(index)];
    if let Some(mode) = mode {
        values.push(integer(mode));
    }
    read_dimension_dispatch(
        context,
        &request(
            "object-handler:simplexml::read_dimension",
            receiver,
            values.len(),
            &values,
            &[],
        ),
    )
    .expect("dimension read succeeds")
}

/// Verifies an append dimension materializes one missing element for a chained write.
#[test]
fn simplexml_object_handler_append_dimension_read_materializes_child() {
    let (mut context, root) = fixture(b"<root/>", 0x8000_0042);
    let (posts_view, _) = result_handle(read_property(&mut context, root, b"posts"));
    let values = [append_offset(), integer(1)];
    let (child, wrapper_kind) = result_handle(
        read_dimension_dispatch(
            &mut context,
            &request(
                "object-handler:simplexml::read_dimension",
                posts_view,
                values.len(),
                &values,
                &[],
            ),
        )
        .expect("null dimension read succeeds"),
    );
    assert_eq!(wrapper_kind, 0x8000_0042);
    let child_snapshot = super::snapshot(&context, child).expect("child wrapper remains valid");
    assert_eq!(
        crate::native::node_local_name(child_snapshot.pointer).as_deref(),
        Some(b"posts".as_slice()),
    );
}

/// Verifies an append-target property stays empty until the dimension appends exactly one child.
#[test]
fn simplexml_object_handler_append_target_defers_missing_terminal_materialization() {
    let (mut context, root) = fixture(b"<root/>", 0x8000_0042);
    let (bla, _) = result_handle(read_property_for_write(&mut context, root, b"bla", false));
    let (posts, _) = result_handle(read_property_for_write(&mut context, bla, b"posts", true));
    assert_eq!(count(&mut context, posts), 0, "terminal append view must remain empty");

    let values = [append_offset(), integer(1)];
    let (child, _) = result_handle(
        read_dimension_dispatch(
            &mut context,
            &request(
                "object-handler:simplexml::read_dimension",
                posts,
                values.len(),
                &values,
                &[],
            ),
        )
        .expect("append dimension read succeeds"),
    );
    assert_eq!(count(&mut context, posts), 1, "append must materialize one posts child");

    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, b"name"),
        push_bytes(&mut bytes, b"FooBar"),
    ];
    write_property_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_property",
            child,
            values.len(),
            &values,
            &bytes,
        ),
    )
    .expect("appended child property write succeeds");
    let (name, _) = result_handle(read_property(&mut context, child, b"name"));
    assert_eq!(cast_string(&mut context, name), b"FooBar");
}

/// Verifies an append-target property adds one sibling when named children already exist.
#[test]
fn simplexml_object_handler_append_target_adds_sibling_to_existing_named_children() {
    let (mut context, root) = fixture(
        b"<root><bla><posts><name>old</name></posts></bla></root>",
        0x8000_0042,
    );
    let (bla, _) = result_handle(read_property_for_write(&mut context, root, b"bla", false));
    let (posts, _) = result_handle(read_property_for_write(&mut context, bla, b"posts", true));
    assert_eq!(count(&mut context, posts), 1, "existing posts must remain selected");

    let values = [append_offset(), integer(1)];
    let (child, _) = result_handle(
        read_dimension_dispatch(
            &mut context,
            &request(
                "object-handler:simplexml::read_dimension",
                posts,
                values.len(),
                &values,
                &[],
            ),
        )
        .expect("append dimension read succeeds"),
    );
    assert_eq!(count(&mut context, posts), 2, "append must add exactly one sibling");
    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, b"name"),
        push_bytes(&mut bytes, b"new"),
    ];
    write_property_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_property",
            child,
            values.len(),
            &values,
            &bytes,
        ),
    )
    .expect("second appended child property write succeeds");
    let (name, _) = result_handle(read_property(&mut context, child, b"name"));
    assert_eq!(cast_string(&mut context, name), b"new");
}

/// Verifies an explicit PHP null offset is not reinterpreted as an append request.
#[test]
fn simplexml_object_handler_null_dimension_is_not_append() {
    let (mut context, root) = fixture(b"<root><p>old</p></root>", 0x8000_0042);
    let (view, _) = result_handle(read_property(&mut context, root, b"p"));
    let values = [null(), integer(0)];
    let read = read_dimension_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::read_dimension",
            view,
            values.len(),
            &values,
            &[],
        ),
    )
    .expect("null dimension read succeeds");
    assert_eq!(read.value_tag, VALUE_NULL);

    let mut bytes = Vec::new();
    let values = [null(), push_bytes(&mut bytes, b"not-an-append")];
    let write = write_dimension_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_dimension",
            view,
            values.len(),
            &values,
            &bytes,
        ),
    )
    .expect("null dimension write returns a PHP exception");
    assert_eq!(write.frame.status, STATUS_THROW);
    assert_eq!(
        write.frame.bytes.as_ref(),
        b"Cannot create attribute with an empty name"
    );
}

/// Casts one handler wrapper to its direct string content.
fn cast_string(context: &mut Context, receiver: u64) -> Vec<u8> {
    let values = [integer(3)];
    cast(
        context,
        &request(
            "object-handler:simplexml::cast",
            receiver,
            1,
            &values,
            &[],
        ),
    )
    .expect("string cast succeeds")
    .frame
    .bytes
    .into_vec()
}

/// Casts one complete handler view with php-src's SimpleXML boolean rules.
fn cast_bool_value(context: &mut Context, receiver: u64) -> bool {
    let values = [integer(0)];
    let result = cast(
        context,
        &request(
            "object-handler:simplexml::cast",
            receiver,
            1,
            &values,
            &[],
        ),
    )
    .expect("boolean cast succeeds");
    result.frame.payload0 != 0
}

/// Casts one handler wrapper through php-src's dynamic `_IS_NUMBER` path.
fn cast_number(context: &mut Context, receiver: u64) -> crate::dispatch::DispatchResult {
    let values = [integer(4)];
    cast(
        context,
        &request(
            "object-handler:simplexml::cast",
            receiver,
            1,
            &values,
            &[],
        ),
    )
    .expect("number cast succeeds")
}

/// Casts one handler wrapper to php-src's array-cast property projection.
fn cast_array(context: &mut Context, receiver: u64) -> crate::dispatch::DispatchResult {
    let values = [integer(5)];
    cast(
        context,
        &request(
            "object-handler:simplexml::cast",
            receiver,
            1,
            &values,
            &[],
        ),
    )
    .expect("array cast succeeds")
}

/// Verifies array casts reuse the recursive PHP property projection for roots and selections.
#[test]
fn simplexml_object_handler_array_cast_matches_php_033_and_034() {
    let (mut empty_context, empty) = fixture(b"<foo/>", 0x8000_0042);
    let empty = cast_array(&mut empty_context, empty);
    assert_eq!(empty.value_tag, VALUE_MAP);
    assert_eq!(empty.frame.payload1, 0);
    assert!(empty.frame.values.is_empty());

    let (mut people_context, people) = fixture(
        b"<people><person name='Joe'/><person name='John'><children><person name='Joe'/></children></person><person name='Jane'/></people>",
        0x8000_0042,
    );
    let people = cast_array(&mut people_context, people);
    assert_eq!(people.value_tag, VALUE_MAP);
    assert_eq!(people.frame.payload1, 1);
    let key = &people.frame.values[0];
    let key_start = key.payload0 as usize;
    let key_end = key_start + key.payload1 as usize;
    assert_eq!(&people.frame.bytes[key_start..key_end], b"person");
    let repeated = &people.frame.values[1];
    assert_eq!(repeated.tag, VALUE_ARRAY);
    assert_eq!(repeated.payload1, 3);
    let repeated_start = repeated.payload0 as usize;
    for wrapper in &people.frame.values[repeated_start..repeated_start + 3] {
        assert_eq!(wrapper.tag, VALUE_BRIDGE_HANDLE);
        assert_eq!(wrapper.payload1, 0x8000_0042);
    }

    let (mut selection_context, root) = fixture(
        b"<foo><bar><p>Blah 1</p><p>Blah 2</p><p>Blah 3</p><tt>Blah 4</tt></bar></foo>",
        0,
    );
    let (bar, _) = result_handle(read_property(&mut selection_context, root, b"bar"));
    let (paragraphs, _) = result_handle(read_property(&mut selection_context, bar, b"p"));
    let paragraphs = cast_array(&mut selection_context, paragraphs);
    assert_eq!(paragraphs.value_tag, VALUE_MAP);
    assert_eq!(paragraphs.frame.payload1, 3);
    for (index, expected) in [b"Blah 1", b"Blah 2", b"Blah 3"].iter().enumerate() {
        let key = &paragraphs.frame.values[index * 2];
        let value = &paragraphs.frame.values[index * 2 + 1];
        assert_eq!(key.tag, VALUE_INT);
        assert_eq!(key.payload0, index as u64);
        assert_eq!(value.tag, VALUE_BYTES);
        let start = value.payload0 as usize;
        let end = start + value.payload1 as usize;
        assert_eq!(&paragraphs.frame.bytes[start..end], *expected);
    }
}

/// Verifies arithmetic casts preserve PHP's dynamic integer/float distinction.
#[test]
fn simplexml_object_handler_number_cast_preserves_numeric_kind() {
    let (mut integer_context, integer) = fixture(b"<r>30</r>", 0x8000_0042);
    let integer = cast_number(&mut integer_context, integer);
    assert_eq!(integer.value_tag, VALUE_INT);
    assert_eq!(integer.frame.payload0 as i64, 30);

    let (mut float_context, float) = fixture(b"<r>12.5</r>", 0x8000_0042);
    let float = cast_number(&mut float_context, float);
    assert_eq!(float.value_tag, VALUE_FLOAT);
    assert_eq!(f64::from_bits(float.frame.payload0), 12.5);

    let (mut exponent_context, exponent) = fixture(b"<r>1e3</r>", 0x8000_0042);
    let exponent = cast_number(&mut exponent_context, exponent);
    assert_eq!(exponent.value_tag, VALUE_FLOAT);
    assert_eq!(f64::from_bits(exponent.frame.payload0), 1000.0);

    let (mut overflow_context, overflow) =
        fixture(b"<r>9223372036854775808</r>", 0x8000_0042);
    let overflow = cast_number(&mut overflow_context, overflow);
    assert_eq!(overflow.value_tag, VALUE_FLOAT);
    assert_eq!(f64::from_bits(overflow.frame.payload0), 9.223372036854776e18);

    let (mut empty_context, empty) = fixture(b"<r/>", 0x8000_0042);
    let empty = cast_number(&mut empty_context, empty);
    assert_eq!(empty.value_tag, VALUE_INT);
    assert_eq!(empty.frame.payload0, 0);
}

/// Verifies boolean casts preserve php-src's direct-node and filtered-view semantics.
#[test]
fn simplexml_object_handler_bool_cast_matches_php_858() {
    for (xml, expected) in [
        (b"<r/>".as_slice(), false),
        (b"<r>0</r>".as_slice(), true),
        (b"<r>text</r>".as_slice(), true),
        (b"<r empty=''/>".as_slice(), true),
    ] {
        let (mut context, root) = fixture(xml, 0);
        assert_eq!(cast_bool_value(&mut context, root), expected, "{xml:?}");
    }

    let (mut context, root) = fixture(
        b"<r empty='' zero='0' text='x' xmlns:p='urn:p'><empty/><zero>0</zero><text>x</text><p:item/></r>",
        0,
    );
    for (name, direct_expected) in [
        (b"empty".as_slice(), false),
        (b"zero".as_slice(), true),
        (b"text".as_slice(), true),
    ] {
        let (view, _) = result_handle(read_property(&mut context, root, name));
        assert!(cast_bool_value(&mut context, view), "property view {name:?}");
        let (direct, _) = result_handle(read_index(&mut context, view, 0, None));
        assert_eq!(
            cast_bool_value(&mut context, direct),
            direct_expected,
            "direct view {name:?}",
        );

        let mut bytes = Vec::new();
        let values = [push_bytes(&mut bytes, name)];
        let (attribute, _) = result_handle(
            read_dimension_dispatch(
                &mut context,
                &request(
                    "object-handler:simplexml::read_dimension",
                    root,
                    1,
                    &values,
                    &bytes,
                ),
            )
            .expect("attribute read succeeds"),
        );
        assert!(cast_bool_value(&mut context, attribute), "attribute {name:?}");
    }
    let (missing, _) = result_handle(read_property(&mut context, root, b"missing"));
    assert!(!cast_bool_value(&mut context, missing));

    let snapshot = super::snapshot(&context, root).expect("root snapshot");
    let attributes = super::fresh_view_handle(
        &mut context,
        &snapshot,
        snapshot.pointer,
        SimpleXmlIteratorState::new(SimpleXmlIteratorType::AttrList, None, None, false),
    );
    assert!(cast_bool_value(&mut context, attributes));
    let missing_attributes = super::fresh_view_handle(
        &mut context,
        &snapshot,
        snapshot.pointer,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::AttrList,
            None,
            Some(b"urn:missing".to_vec()),
            false,
        ),
    );
    assert!(!cast_bool_value(&mut context, missing_attributes));
    let prefixed_children = super::fresh_view_handle(
        &mut context,
        &snapshot,
        snapshot.pointer,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::Child,
            None,
            Some(b"urn:p".to_vec()),
            false,
        ),
    );
    assert!(cast_bool_value(&mut context, prefixed_children));
    let prefixed_children_by_prefix = super::fresh_view_handle(
        &mut context,
        &snapshot,
        snapshot.pointer,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::Child,
            None,
            Some(b"p".to_vec()),
            true,
        ),
    );
    assert!(cast_bool_value(&mut context, prefixed_children_by_prefix));
    let prefixed_item = super::fresh_view_handle(
        &mut context,
        &snapshot,
        snapshot.pointer,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::Element,
            Some(b"item".to_vec()),
            Some(b"urn:p".to_vec()),
            false,
        ),
    );
    assert!(cast_bool_value(&mut context, prefixed_item));
    let missing_children = super::fresh_view_handle(
        &mut context,
        &snapshot,
        snapshot.pointer,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::Child,
            None,
            Some(b"urn:missing".to_vec()),
            false,
        ),
    );
    assert!(!cast_bool_value(&mut context, missing_children));

    let (mut context, root) = fixture(b"<r a='' />", 0);
    let snapshot = super::snapshot(&context, root).expect("attribute-only root snapshot");
    let children = super::fresh_view_handle(
        &mut context,
        &snapshot,
        snapshot.pointer,
        SimpleXmlIteratorState::new(SimpleXmlIteratorType::Child, None, None, false),
    );
    assert!(
        cast_bool_value(&mut context, children),
        "php-src treats an empty children() view as truthy when the base owns an attribute",
    );
}

/// Counts the live selection represented by one handler wrapper.
fn count(context: &mut Context, receiver: u64) -> i64 {
    count_dispatch(
        context,
        &request(
            "object-handler:simplexml::count",
            receiver,
            0,
            &[],
            &[],
        ),
    )
    .expect("count succeeds")
    .frame
    .payload0 as i64
}

/// Writes one named dimension attribute through the production object-handler route.
fn write_attribute(
    context: &mut Context,
    receiver: u64,
    name: &[u8],
    value: &[u8],
) -> crate::dispatch::DispatchResult {
    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, name),
        push_bytes(&mut bytes, value),
    ];
    write_dimension_dispatch(
        context,
        &request(
            "object-handler:simplexml::write_dimension",
            receiver,
            2,
            &values,
            &bytes,
        ),
    )
    .expect("attribute write succeeds")
}

/// Extracts the first attached warning message from one result.
fn warning(result: &crate::dispatch::DispatchResult) -> Vec<u8> {
    let diagnostic = result
        .frame
        .diagnostics
        .first()
        .expect("result carries one warning");
    let start = diagnostic.message_offset as usize;
    let end = start + diagnostic.message_len as usize;
    result.frame.bytes[start..end].to_vec()
}

/// Verifies fresh filtered views preserve subclass kind, cast, count, and compare rules.
#[test]
fn simplexml_object_handler_filtered_views_cast_count_and_compare() {
    let (mut context, root) =
        fixture(b"<r><a>one</a><a>two</a><b>bee</b></r>", 0x8000_0042);
    let (a1, kind1) = result_handle(read_property(&mut context, root, b"a"));
    let (a2, kind2) = result_handle(read_property(&mut context, root, b"a"));
    let (b, _) = result_handle(read_property(&mut context, root, b"b"));
    assert_ne!(a1, a2);
    assert_eq!(kind1, 0x8000_0042);
    assert_eq!(kind2, kind1);
    assert_eq!(cast_string(&mut context, a1), b"one");
    assert_eq!(count(&mut context, a1), 2);

    for other in [a2, b] {
        let values = [bridge_handle(other)];
        let result = compare_dispatch(
            &mut context,
            &request(
                "object-handler:simplexml::compare",
                a1,
                1,
                &values,
                &[],
            ),
        )
        .expect("compare succeeds");
        assert_eq!(result.frame.payload0 as i64, 0);
    }
    let (first, _) = result_handle(read_index(&mut context, a1, 0, None));
    let (second, _) = result_handle(read_index(&mut context, a1, 1, None));
    let values = [bridge_handle(second)];
    let result = compare_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::compare",
            first,
            1,
            &values,
            &[],
        ),
    )
    .expect("direct-node compare succeeds");
    assert_eq!(result.frame.payload0 as i64, 1);
}

/// Verifies root and filtered numeric offsets match PHP's negative and warning behavior.
#[test]
fn simplexml_object_handler_numeric_offsets_match_php_858() {
    let (mut context, root) = fixture(b"<r><a>one</a><a>two</a></r>", 77);
    let result = read_index(&mut context, root, 2, None);
    let same_root = result.frame.payload0;
    let kind = result.frame.payload1;
    assert_eq!(kind, 77);
    assert_eq!(cast_string(&mut context, same_root), b"");
    assert_eq!(
        warning(&result),
        b"Cannot add element r number 2 when only 0 such elements exist"
    );
    assert_eq!(
        result.frame.diagnostics[0].reserved,
        DIAGNOSTIC_FLAG_CALLSITE_CONTEXT
    );

    let (a, _) = result_handle(read_property(&mut context, root, b"a"));
    let (negative, _) = result_handle(read_index(&mut context, a, -1, None));
    assert_eq!(cast_string(&mut context, negative), b"one");
    assert_eq!(
        read_index(&mut context, a, 2, None).value_tag,
        VALUE_NULL
    );
}

/// Verifies a numeric write gap mutates once and exports call-site warning detail.
#[test]
fn simplexml_object_handler_numeric_write_gap_matches_php_858() {
    let (mut context, root) = fixture(b"<r><a>one</a><a>two</a></r>", 77);
    let (a, _) = result_handle(read_property(&mut context, root, b"a"));
    let mut bytes = Vec::new();
    let values = [integer(3), push_bytes(&mut bytes, b"three")];
    let result = write_dimension_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_dimension",
            a,
            2,
            &values,
            &bytes,
        ),
    )
    .expect("numeric gap write succeeds with a warning");

    assert_eq!(
        warning(&result),
        b"Cannot add element a number 3 when only 2 such elements exist"
    );
    assert_eq!(
        result.frame.diagnostics[0].reserved,
        DIAGNOSTIC_FLAG_CALLSITE_CONTEXT
    );
    assert_eq!(count(&mut context, a), 3);
    let (third, _) = result_handle(read_index(&mut context, a, 2, None));
    assert_eq!(cast_string(&mut context, third), b"three");
}

/// Verifies AttrList pointers remain parent-rooted and apply attribute filters safely.
#[test]
fn simplexml_object_handler_attrlist_uses_parent_pointer_and_filters() {
    let (mut context, root) = fixture(b"<r a='A' b='B'/>", 91);
    let snapshot = super::snapshot(&context, root).expect("root snapshot");
    let attrlist = super::fresh_view_handle(
        &mut context,
        &snapshot,
        snapshot.pointer,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::AttrList,
            None,
            None,
            false,
        ),
    );
    assert_eq!(count(&mut context, attrlist), 2);
    let (second, kind) = result_handle(read_index(&mut context, attrlist, 1, None));
    assert_eq!(kind, 91);
    assert_eq!(cast_string(&mut context, second), b"B");
}

/// Verifies isset and empty distinguish missing, zero, whitespace, and attribute values.
#[test]
fn simplexml_object_handler_has_modes_match_php_858() {
    let (mut context, root) = fixture(
        b"<r a='A' z='0'><v>1</v><zero>0</zero><empty/><space> </space></r>",
        0,
    );
    for (name, isset, non_empty) in [
        (b"v".as_slice(), true, true),
        (b"zero", true, false),
        (b"empty", true, false),
        (b"space", true, true),
        (b"missing", false, false),
    ] {
        for (check_empty, expected) in [(false, isset), (true, non_empty)] {
            let mut bytes = Vec::new();
            let values = [
                push_bytes(&mut bytes, name),
                boolean(check_empty),
            ];
            let result = has_property_dispatch(
                &mut context,
                &request(
                    "object-handler:simplexml::has_property",
                    root,
                    2,
                    &values,
                    &bytes,
                ),
            )
            .expect("property existence succeeds");
            assert_eq!(result.frame.payload0 != 0, expected, "{name:?}");
        }
    }
    for (name, isset, non_empty) in [
        (b"a".as_slice(), true, true),
        (b"z", true, false),
        (b"missing", false, false),
    ] {
        for (check_empty, expected) in [(false, isset), (true, non_empty)] {
            let mut bytes = Vec::new();
            let values = [
                push_bytes(&mut bytes, name),
                boolean(check_empty),
            ];
            let result = has_dimension_dispatch(
                &mut context,
                &request(
                    "object-handler:simplexml::has_dimension",
                    root,
                    2,
                    &values,
                    &bytes,
                ),
            )
            .expect("dimension existence succeeds");
            assert_eq!(result.frame.payload0 != 0, expected, "{name:?}");
        }
    }
}

/// Verifies BP_VAR_IS suppresses missing views and property-address mode autovivifies chains.
#[test]
fn simplexml_object_handler_bp_var_and_property_address() {
    let (mut context, root) = fixture(b"<r/>", 101);
    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, b"missing"),
        integer(3),
    ];
    let missing = read_property_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::read_property",
            root,
            2,
            &values,
            &bytes,
        ),
    )
    .expect("BP_VAR_IS read succeeds");
    assert_eq!(missing.value_tag, VALUE_NULL);

    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, b"bla"),
        integer(1),
        boolean(true),
    ];
    let (bla, _) = result_handle(
        read_property_dispatch(
            &mut context,
            &request(
                "object-handler:simplexml::read_property",
                root,
                3,
                &values,
                &bytes,
            ),
        )
        .expect("property address creates bla"),
    );
    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, b"posts"),
        integer(1),
        boolean(true),
    ];
    let (posts, _) = result_handle(
        read_property_dispatch(
            &mut context,
            &request(
                "object-handler:simplexml::read_property",
                bla,
                3,
                &values,
                &bytes,
            ),
        )
        .expect("property address creates posts"),
    );
    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, b"name"),
        push_bytes(&mut bytes, b"FooBar"),
    ];
    write_property_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_property",
            posts,
            2,
            &values,
            &bytes,
        ),
    )
    .expect("nested property write succeeds");
    let (name, _) = result_handle(read_property(&mut context, posts, b"name"));
    assert_eq!(cast_string(&mut context, name), b"FooBar");
}

/// Verifies writes and unsets mutate live views while detached wrappers remain readable.
#[test]
fn simplexml_object_handler_mutation_preserves_detached_liveness() {
    let (mut context, root) =
        fixture(b"<r a='A'><x><old>kept</old></x><y>drop</y></r>", 0);
    let (x_view, _) = result_handle(read_property(&mut context, root, b"x"));
    let (x, _) = result_handle(read_index(&mut context, x_view, 0, None));
    let (old_view, _) = result_handle(read_property(&mut context, x, b"old"));
    let (old, _) = result_handle(read_index(&mut context, old_view, 0, None));

    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, b"x"),
        push_bytes(&mut bytes, b"changed"),
    ];
    write_property_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_property",
            root,
            2,
            &values,
            &bytes,
        ),
    )
    .expect("property write succeeds");
    assert_eq!(cast_string(&mut context, x_view), b"changed");
    assert_eq!(cast_string(&mut context, old), b"kept");

    let mut bytes = Vec::new();
    let values = [push_bytes(&mut bytes, b"y")];
    unset_property_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::unset_property",
            root,
            1,
            &values,
            &bytes,
        ),
    )
    .expect("property unset succeeds");
    let missing_y = read_property(&mut context, root, b"y").frame.payload0;
    assert_eq!(count(&mut context, missing_y), 0);

    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, b"a"),
        push_bytes(&mut bytes, b"B"),
    ];
    write_dimension_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_dimension",
            root,
            2,
            &values,
            &bytes,
        ),
    )
    .expect("attribute write succeeds");
    let mut bytes = Vec::new();
    let values = [push_bytes(&mut bytes, b"a")];
    let (attribute, _) = result_handle(
        read_dimension_dispatch(
            &mut context,
            &request(
                "object-handler:simplexml::read_dimension",
                root,
                1,
                &values,
                &bytes,
            ),
        )
        .expect("attribute read succeeds"),
    );
    assert_eq!(cast_string(&mut context, attribute), b"B");
    unset_dimension_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::unset_dimension",
            root,
            1,
            &values,
            &bytes,
        ),
    )
    .expect("attribute unset succeeds");
}

/// Verifies PHPT 028 creates one missing named element before its attribute and
/// reuses that live element for subsequent writes instead of appending duplicates.
#[test]
fn simplexml_object_handler_missing_element_attribute_write_autovivifies_once() {
    let (mut context, root) = fixture(b"<people/>", 0x8000_0042);
    let (person_view, wrapper_kind) = result_handle(read_property(&mut context, root, b"person"));
    assert_eq!(wrapper_kind, 0x8000_0042);
    assert_eq!(count(&mut context, person_view), 0);

    let first = write_attribute(&mut context, person_view, b"name", b"John");
    assert_eq!(first.value_tag, VALUE_NULL);
    assert_eq!(count(&mut context, person_view), 1);
    let (person, created_kind) = result_handle(read_index(&mut context, person_view, 0, None));
    assert_eq!(created_kind, wrapper_kind);
    let mut bytes = Vec::new();
    let values = [push_bytes(&mut bytes, b"name")];
    let (attribute, _) = result_handle(
        read_dimension_dispatch(
            &mut context,
            &request(
                "object-handler:simplexml::read_dimension",
                person_view,
                1,
                &values,
                &bytes,
            ),
        )
        .expect("autovivified attribute read succeeds"),
    );
    assert_eq!(cast_string(&mut context, attribute), b"John");

    let second = write_attribute(&mut context, person_view, b"name", b"Jane");
    assert_eq!(second.value_tag, VALUE_NULL);
    assert_eq!(count(&mut context, person_view), 1);
    let (same_person, _) = result_handle(read_index(&mut context, person_view, 0, None));
    assert_eq!(
        super::snapshot(&context, same_person)
            .expect("reselected person snapshot")
            .pointer,
        super::snapshot(&context, person)
            .expect("original person snapshot")
            .pointer,
    );
    assert_eq!(
        crate::native::node_content(
            crate::native::element_get_attribute_node(
                super::snapshot(&context, person)
                    .expect("person snapshot")
                    .pointer,
                b"name",
            )
            .expect("person owns exact name attribute"),
        )
        .as_deref(),
        Some(b"Jane".as_slice()),
    );
}

/// Verifies php-src's `mynode->ns` rule: a missing selected child inherits both
/// default and prefixed namespace bindings from its parent before attribute creation.
#[test]
fn simplexml_object_handler_autovivified_element_inherits_parent_namespace() {
    for (xml, namespace_uri, prefix) in [
        (
            b"<people xmlns='urn:default'/>".as_slice(),
            b"urn:default".as_slice(),
            None,
        ),
        (
            b"<p:people xmlns:p='urn:prefixed'/>".as_slice(),
            b"urn:prefixed".as_slice(),
            Some(b"p".as_slice()),
        ),
    ] {
        let (mut context, root) = fixture(xml, 0);
        let (person_view, _) = result_handle(read_property(&mut context, root, b"person"));
        let result = write_attribute(&mut context, person_view, b"name", b"John");
        assert_eq!(result.value_tag, VALUE_NULL);

        let created = crate::native::node_first_child(
            super::snapshot(&context, root)
                .expect("root snapshot")
                .pointer,
        )
        .expect("missing person element is created");
        assert_eq!(
            crate::native::node_local_name(created).as_deref(),
            Some(b"person".as_slice()),
        );
        assert_eq!(
            crate::native::node_namespace_uri(created).as_deref(),
            Some(namespace_uri),
        );
        assert_eq!(crate::native::node_prefix(created).as_deref(), prefix);
        assert_eq!(crate::native::element_attribute_count(created), 1);
        assert_eq!(
            crate::native::node_content(
                crate::native::element_get_attribute_node(created, b"name")
                    .expect("created element owns name attribute"),
            )
            .as_deref(),
            Some(b"John".as_slice()),
        );
    }
}

/// Verifies null-offset appends and write-name trimming match php-src.
#[test]
fn simplexml_object_handler_append_and_trimmed_dimension_writes_match_php_858() {
    let (mut context, root) = fixture(b"<r><a>one</a></r>", 0);
    let (a_view, _) = result_handle(read_property(&mut context, root, b"a"));
    let mut bytes = Vec::new();
    let values = [append_offset(), push_bytes(&mut bytes, b"two")];
    write_dimension_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_dimension",
            a_view,
            2,
            &values,
            &bytes,
        ),
    )
    .expect("element-view append succeeds");
    assert_eq!(count(&mut context, a_view), 2);
    let (second, _) = result_handle(read_index(&mut context, a_view, 1, None));
    assert_eq!(cast_string(&mut context, second), b"two");

    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, b" \tlabel\r\n"),
        push_bytes(&mut bytes, b"value"),
    ];
    write_dimension_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_dimension",
            root,
            2,
            &values,
            &bytes,
        ),
    )
    .expect("trimmed attribute name write succeeds");
    let mut bytes = Vec::new();
    let values = [push_bytes(&mut bytes, b"label")];
    let (attribute, _) = result_handle(
        read_dimension_dispatch(
            &mut context,
            &request(
                "object-handler:simplexml::read_dimension",
                root,
                1,
                &values,
                &bytes,
            ),
        )
        .expect("trimmed attribute is readable"),
    );
    assert_eq!(cast_string(&mut context, attribute), b"value");

    let mut bytes = Vec::new();
    let values = [append_offset(), push_bytes(&mut bytes, b"blocked")];
    let root_append = write_dimension_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_dimension",
            root,
            2,
            &values,
            &bytes,
        ),
    )
    .expect("root append returns a PHP exception");
    assert_eq!(root_append.frame.status, STATUS_THROW);
    assert_eq!(
        root_append.frame.bytes.as_ref(),
        b"Cannot append to an attribute list"
    );
}

/// Verifies duplicate and complex writes expose php-src warnings and TypeErrors.
#[test]
fn simplexml_object_handler_write_diagnostics_match_php_858() {
    let (mut context, root) = fixture(b"<r><x>one</x><x>two</x></r>", 0);
    let mut bytes = Vec::new();
    let values = [
        push_bytes(&mut bytes, b"x"),
        push_bytes(&mut bytes, b"changed"),
    ];
    let duplicate = write_property_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_property",
            root,
            2,
            &values,
            &bytes,
        ),
    )
    .expect("duplicate write returns warning");
    assert_eq!(
        warning(&duplicate),
        b"Warning: Unknown: Cannot assign to an array of nodes (duplicate subnodes or attr detected)\n"
    );
    let x = read_property(&mut context, root, b"x").frame.payload0;
    assert_eq!(cast_string(&mut context, x), b"one");

    let mut bytes = Vec::new();
    let array_value = Value {
        tag: VALUE_ARRAY,
        flags: 0,
        payload0: 2,
        payload1: 1,
    };
    let values = [
        push_bytes(&mut bytes, b"k"),
        array_value,
        integer(1),
    ];
    let complex = write_property_dispatch(
        &mut context,
        &request(
            "object-handler:simplexml::write_property",
            root,
            2,
            &values,
            &bytes,
        ),
    )
    .expect("complex write returns TypeError");
    assert_eq!(complex.frame.status, STATUS_THROW);
    assert_eq!(complex.frame.php_error_kind, PHP_ERROR_KIND_TYPE_ERROR);
    assert_eq!(
        complex.frame.bytes.as_ref(),
        b"It's not possible to assign a complex type to properties, array given"
    );
}
