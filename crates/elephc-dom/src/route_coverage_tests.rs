//! Purpose:
//! Pins the generated DOM route inventory and adversarial public C-ABI contract.
//! It intentionally tests the locked PHP 8.5.8 specification before bridge fixes land.
//!
//! Called from:
//! - `cargo test -p elephc-dom route_coverage_tests` through Rust's test harness.
//!
//! Key details:
//! - Every generated route has an explicit native, compiler-resident, or internal classification.
//! - ABI failures must not mutate DOM state and must retain pointer-free result frames until release.

use std::ffi::c_void;

use crate::abi::{
    HostVTable, RequestHeader, ResultHeader, Value, ABI_VERSION, OPCODE_ABI_PING,
    STATUS_ABI_ERROR, STATUS_MALFORMED_REQUEST, STATUS_OK, VALUE_ARRAY,
    VALUE_BRIDGE_HANDLE, VALUE_BYTES, VALUE_MAP,
};

/// The three bridge-internal DOM lifecycle routes generated in the manifest.
const INTERNAL_DOM_ROUTES: &[&str] = &[
    "internal:bridge.object.clone",
    "internal:bridge.wrapper.release",
    "internal:bridge.wrapper.retain",
];

/// DOM members implemented by compiler-managed enums, constructors, or iteration machinery.
///
/// This is deliberately an exact list rather than a catch-all branch: adding a compiler route
/// must update this inventory in the same change as the generated manifest.
const COMPILER_RESIDENT_DOM_ROUTES: &[&str] = &[
    "method:dom\\adjacentposition::cases",
    "method:dom\\adjacentposition::from",
    "method:dom\\adjacentposition::tryfrom",
    "method:dom\\namespaceinfo::__construct",
    "method:dom\\node::__construct",
    "method:dom\\tokenlist::__construct",
    "method:dom\\dtdnamednodemap::getiterator",
    "method:dom\\htmlcollection::getiterator",
    "method:dom\\namednodemap::getiterator",
    "method:dom\\nodelist::getiterator",
    "method:domnamednodemap::getiterator",
    "method:domnodelist::getiterator",
    "method:dom\\tokenlist::getiterator",
    "property-get:dom\\adjacentposition::$name",
    "property-get:dom\\adjacentposition::$value",
];

/// Stable extension family inferred from a generated operation key without a default family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExtensionFamily {
    /// The legacy and modern PHP DOM surface plus bridge lifecycle operations.
    Dom,
    /// The companion libxml surface.
    Libxml,
    /// The companion SimpleXML surface and object handlers.
    SimpleXml,
}

/// Explicit execution owner for one DOM opcode route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RouteOwner {
    /// The operation must enter the native DOM bridge dispatcher.
    Native,
    /// The operation is implemented by ordinary compiler/runtime machinery.
    CompilerResident,
    /// The operation is bridge-private lifecycle plumbing.
    Internal,
}

/// Identifies the manifest extension for one known generated operation key.
fn extension_family(key: &str) -> Option<ExtensionFamily> {
    match key {
        key if key.starts_with("function:libxml_")
            || key.starts_with("property-get:libxmlerror::")
            || key.starts_with("property-set:libxmlerror::") => Some(ExtensionFamily::Libxml),
        key if key.starts_with("function:simplexml_")
            || key.starts_with("method:simplexml")
            || key.starts_with("object-handler:simplexml::") => Some(ExtensionFamily::SimpleXml),
        key if key.starts_with("function:dom")
            || key.starts_with("method:dom")
            || key.starts_with("property-get:dom")
            || key.starts_with("property-set:dom")
            || INTERNAL_DOM_ROUTES.contains(&key) => Some(ExtensionFamily::Dom),
        _ => None,
    }
}

/// Classifies one DOM operation only when it has an explicit generated route shape.
///
/// There is intentionally no `otherwise native` fallback: an unfamiliar key fails the test
/// until its generated operation and its executor are classified together.
fn dom_route_owner(key: &str) -> Option<RouteOwner> {
    if extension_family(key) != Some(ExtensionFamily::Dom) {
        return None;
    }
    if INTERNAL_DOM_ROUTES.contains(&key) {
        return Some(RouteOwner::Internal);
    }
    if COMPILER_RESIDENT_DOM_ROUTES.contains(&key) {
        return Some(RouteOwner::CompilerResident);
    }
    match key.split_once(':').map(|(kind, _)| kind) {
        Some("function" | "method" | "property-get" | "property-set") => {
            Some(RouteOwner::Native)
        }
        _ => None,
    }
}

/// Owns one public bridge context for the duration of an adversarial test.
struct TestContext(u64);

impl TestContext {
    /// Allocates a callback-free bridge context with the current public host-vtable shape.
    fn new() -> Self {
        let host = HostVTable {
            abi_version: ABI_VERSION,
            struct_size: std::mem::size_of::<HostVTable>() as u32,
            user_data: std::ptr::null_mut::<c_void>(),
            call: None,
        };
        let mut context = 0;
        let status = unsafe { crate::elephc_dom_context_new(&host, &mut context) };
        assert_eq!(status, STATUS_OK, "test fixture context construction");
        assert_ne!(context, 0, "test fixture context id");
        Self(context)
    }

    /// Returns the opaque public context identifier.
    fn id(&self) -> u64 {
        self.0
    }
}

impl Drop for TestContext {
    /// Releases the fixture context even when an intentionally-red TDD assertion fails.
    fn drop(&mut self) {
        crate::elephc_dom_context_free(self.0);
    }
}

/// Captures native state only; retained result frames are released before snapshots compare.
#[derive(Debug, Eq, PartialEq)]
struct NativeState {
    native_handles: usize,
    documents: usize,
    nodes: usize,
    implementations: usize,
    token_lists: usize,
    namespaces: usize,
    detached_roots: usize,
}

/// Reads a bridge context's DOM-mutating state without changing ownership.
fn native_state(context_id: u64) -> NativeState {
    let context = crate::context::context(context_id).expect("live test context");
    let context = context.borrow();
    NativeState {
        native_handles: context.native_objects.len(),
        documents: context.document_handles.len(),
        nodes: context.node_handles.len(),
        implementations: context.implementation_handles.len(),
        token_lists: context.token_list_handles.len(),
        namespaces: context.namespace_node_handles.len(),
        detached_roots: context.detached_roots.len(),
    }
}

/// Encodes one public flat request without relying on its Rust in-memory alignment.
fn request_bytes(header: RequestHeader, values: &[Value], bytes: &[u8]) -> Vec<u8> {
    let mut request = Vec::with_capacity(
        std::mem::size_of::<RequestHeader>()
            + std::mem::size_of_val(values)
            + bytes.len(),
    );
    unsafe {
        request.extend_from_slice(std::slice::from_raw_parts(
            (&header as *const RequestHeader).cast::<u8>(),
            std::mem::size_of::<RequestHeader>(),
        ));
        request.extend_from_slice(std::slice::from_raw_parts(
            values.as_ptr().cast::<u8>(),
            std::mem::size_of_val(values),
        ));
    }
    request.extend_from_slice(bytes);
    request
}

/// Builds the canonical, non-mutating ABI-ping request header.
fn ping_header() -> RequestHeader {
    RequestHeader {
        abi_version: ABI_VERSION,
        header_size: std::mem::size_of::<RequestHeader>() as u32,
        opcode: OPCODE_ABI_PING,
        flags: 0,
        receiver: 0,
        value_count: 0,
        byte_count: 0,
    }
}

/// Resolves one generated operation key to its stable public opcode.
fn opcode(key: &str) -> u32 {
    crate::generated::opcodes::OPERATIONS
        .iter()
        .find_map(|(opcode, candidate)| (*candidate == key).then_some(*opcode))
        .unwrap_or_else(|| panic!("missing generated operation {key}"))
}

/// Calls the exported boundary with an already encoded adversarial request.
fn call(context: u64, request: &[u8]) -> (u32, ResultHeader) {
    let mut result = ResultHeader::abi_error();
    let status = unsafe {
        crate::elephc_dom_call(
            context,
            request.as_ptr(),
            request.len() as u64,
            &mut result,
        )
    };
    (status, result)
}

/// Verifies the scalar error-result ownership contract from specification section 5.6.
fn assert_owned_pointer_free_error(result: &ResultHeader, expected_status: u32, case_id: &str) {
    assert_eq!(result.abi_version, ABI_VERSION, "{case_id}: ABI version");
    assert_eq!(
        result.struct_size,
        std::mem::size_of::<ResultHeader>() as u32,
        "{case_id}: result header size"
    );
    assert_eq!(result.status, expected_status, "{case_id}: result status");
    assert_ne!(result.result_id, 0, "{case_id}: owned result id");
    assert_eq!(result.bytes_len, 0, "{case_id}: no byte payload");
    assert!(result.bytes_ptr.is_null(), "{case_id}: no byte pointer");
    assert_eq!(result.values_len, 0, "{case_id}: no value payload");
    assert!(result.values_ptr.is_null(), "{case_id}: no value pointer");
    assert_eq!(result.diagnostics_len, 0, "{case_id}: no diagnostics");
    assert!(
        result.diagnostics_ptr.is_null(),
        "{case_id}: no diagnostics pointer"
    );
}

/// Releases one result only after proving that the bridge assigned a live ownership ID.
fn release_result(context: u64, result: &ResultHeader) {
    assert_ne!(result.result_id, 0, "result must be explicitly releasable");
    crate::elephc_dom_result_release(context, result.result_id);
}

/// Builds one structurally shaped but never allocated opaque handle for a requested family.
fn forged_handle(kind: u8) -> u64 {
    (u64::from(kind) << 56) | (u64::from(1_u32) << 32) | 1
}

/// Returns a fresh legacy document wrapper handle while releasing only its result frame.
fn legacy_document(context: u64) -> u64 {
    let request = request_bytes(
        RequestHeader {
            opcode: opcode("method:domdocument::__construct"),
            ..ping_header()
        },
        &[],
        &[],
    );
    let (status, result) = call(context, &request);
    assert_eq!(status, STATUS_OK, "legacy document construction");
    let handle = result.payload0;
    release_result(context, &result);
    handle
}

/// Asserts that a rejected ABI request leaves no DOM object/handle mutation after release.
fn assert_rejected_without_native_mutation(
    context: u64,
    case_id: &str,
    request: &[u8],
    expected_status: u32,
) {
    let before = native_state(context);
    let (status, result) = call(context, request);
    assert_eq!(status, expected_status, "{case_id}: return status");
    assert_owned_pointer_free_error(&result, expected_status, case_id);
    release_result(context, &result);
    assert_eq!(native_state(context), before, "{case_id}: native mutation");
}

/// Verifies the exact 603-route manifest partitions into DOM, libxml, and SimpleXML.
#[test]
fn dom_route_inventory_is_complete_and_extension_partitioned() {
    let mut dom = 0;
    let mut libxml = 0;
    let mut simplexml = 0;
    for (offset, (opcode, key)) in crate::generated::opcodes::OPERATIONS.iter().enumerate() {
        assert_eq!(
            *opcode,
            4096 + offset as u32,
            "route {key} must keep its contiguous generated opcode"
        );
        match extension_family(key) {
            Some(ExtensionFamily::Dom) => dom += 1,
            Some(ExtensionFamily::Libxml) => libxml += 1,
            Some(ExtensionFamily::SimpleXml) => simplexml += 1,
            None => panic!("unclassified generated route {key}"),
        }
    }
    assert_eq!(dom, 546, "locked DOM route total");
    assert_eq!(libxml, 20, "locked libxml route total");
    assert_eq!(simplexml, 37, "locked SimpleXML route total");
    assert_eq!(dom + libxml + simplexml, 603, "complete bridge route total");
}

/// Verifies every one of the 546 DOM routes has an explicit executor owner.
#[test]
fn dom_route_inventory_has_no_implicit_executor_fallback() {
    let mut native = 0;
    let mut compiler = 0;
    let mut internal = 0;
    for (_, key) in crate::generated::opcodes::OPERATIONS {
        if extension_family(key) != Some(ExtensionFamily::Dom) {
            continue;
        }
        match dom_route_owner(key) {
            Some(RouteOwner::Native) => native += 1,
            Some(RouteOwner::CompilerResident) => compiler += 1,
            Some(RouteOwner::Internal) => internal += 1,
            None => panic!("DOM route has no explicit executor owner: {key}"),
        }
    }
    assert_eq!(native, 528, "native bridge routes");
    assert_eq!(compiler, 15, "compiler-resident DOM routes");
    assert_eq!(internal, 3, "bridge-internal DOM routes");
    assert_eq!(native + compiler + internal, 546, "all DOM routes classified");
}

/// Verifies generated DOM kind counts remain aligned with the frozen PHP surface manifest.
#[test]
fn dom_route_inventory_preserves_callable_and_property_shape() {
    let dom_routes = crate::generated::opcodes::OPERATIONS
        .iter()
        .filter_map(|(_, key)| {
            (extension_family(key) == Some(ExtensionFamily::Dom)).then_some(*key)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        dom_routes.iter().filter(|key| key.starts_with("function:")).count(),
        2
    );
    assert_eq!(
        dom_routes.iter().filter(|key| key.starts_with("method:")).count(),
        313
    );
    assert_eq!(
        dom_routes
            .iter()
            .filter(|key| key.starts_with("property-get:"))
            .count(),
        184
    );
    assert_eq!(
        dom_routes
            .iter()
            .filter(|key| key.starts_with("property-set:"))
            .count(),
        44
    );
}

/// Verifies all malformed boundary cases retain a scalar result and cannot mutate DOM state.
#[test]
fn abi_boundary_matrix_rejects_before_opcode_or_mutation() {
    let context = TestContext::new();
    let header_size = std::mem::size_of::<RequestHeader>() as u32;
    let mut prefix_only = request_bytes(ping_header(), &[], &[]);
    prefix_only.truncate(8);
    let cases = [
        (
            "abi-unknown-opcode",
            request_bytes(
                RequestHeader {
                    opcode: u32::MAX,
                    ..ping_header()
                },
                &[],
                &[],
            ),
            STATUS_ABI_ERROR,
        ),
        (
            "abi-incompatible-version",
            request_bytes(
                RequestHeader {
                    abi_version: ABI_VERSION + 1,
                    ..ping_header()
                },
                &[],
                &[],
            ),
            STATUS_ABI_ERROR,
        ),
        ("abi-shorter-than-prefix", vec![0; 7], STATUS_MALFORMED_REQUEST),
        (
            "abi-readable-prefix-truncated-header",
            prefix_only,
            STATUS_MALFORMED_REQUEST,
        ),
        (
            "abi-readable-prefix-invalid-header-size",
            request_bytes(
                RequestHeader {
                    header_size: 8,
                    ..ping_header()
                },
                &[],
                &[],
            ),
            STATUS_MALFORMED_REQUEST,
        ),
        (
            "abi-value-count-overflow",
            request_bytes(
                RequestHeader {
                    header_size,
                    value_count: u64::MAX,
                    ..ping_header()
                },
                &[],
                &[],
            ),
            STATUS_MALFORMED_REQUEST,
        ),
        (
            "abi-byte-count-overflow",
            request_bytes(
                RequestHeader {
                    byte_count: u64::MAX,
                    ..ping_header()
                },
                &[],
                &[],
            ),
            STATUS_MALFORMED_REQUEST,
        ),
        (
            "abi-unknown-value-tag",
            request_bytes(
                RequestHeader {
                    value_count: 1,
                    ..ping_header()
                },
                &[Value {
                    tag: u32::MAX,
                    flags: 0,
                    payload0: 0,
                    payload1: 0,
                }],
                &[],
            ),
            STATUS_MALFORMED_REQUEST,
        ),
        (
            "abi-array-range-overflow",
            request_bytes(
                RequestHeader {
                    value_count: 1,
                    ..ping_header()
                },
                &[Value {
                    tag: VALUE_ARRAY,
                    flags: 0,
                    payload0: u64::MAX,
                    payload1: 1,
                }],
                &[],
            ),
            STATUS_MALFORMED_REQUEST,
        ),
        (
            "abi-map-entry-count-overflow",
            request_bytes(
                RequestHeader {
                    value_count: 1,
                    ..ping_header()
                },
                &[Value {
                    tag: VALUE_MAP,
                    flags: 0,
                    payload0: 0,
                    payload1: u64::MAX,
                }],
                &[],
            ),
            STATUS_MALFORMED_REQUEST,
        ),
        (
            "abi-byte-range-overflow",
            request_bytes(
                RequestHeader {
                    value_count: 1,
                    byte_count: 1,
                    ..ping_header()
                },
                &[Value {
                    tag: VALUE_BYTES,
                    flags: 0,
                    payload0: 1,
                    payload1: u64::MAX,
                }],
                b"x",
            ),
            STATUS_MALFORMED_REQUEST,
        ),
        (
            "abi-cyclic-value-tree",
            request_bytes(
                RequestHeader {
                    value_count: 1,
                    ..ping_header()
                },
                &[Value {
                    tag: VALUE_ARRAY,
                    flags: 0,
                    payload0: 0,
                    payload1: 1,
                }],
                &[],
            ),
            STATUS_MALFORMED_REQUEST,
        ),
    ];
    for (case_id, request, expected_status) in cases {
        assert_rejected_without_native_mutation(context.id(), case_id, &request, expected_status);
    }
}

/// Verifies flat byte values preserve embedded NUL instead of using a C-string decode path.
#[test]
fn abi_embedded_nul_is_a_valid_bounded_byte_value() {
    let bytes = b"left\0right";
    let request = request_bytes(
        RequestHeader {
            value_count: 1,
            byte_count: bytes.len() as u64,
            ..ping_header()
        },
        &[Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: bytes.len() as u64,
        }],
        bytes,
    );
    let decoded = crate::request::decode(request.as_ptr(), request.len() as u64)
        .expect("embedded NUL must remain a valid byte string");
    assert_eq!(decoded.byte_string(0), Ok(bytes.as_slice()));
}

/// Verifies every opaque handle family rejects forged and cross-family receivers without mutation.
#[test]
fn abi_handle_family_matrix_rejects_forged_and_cross_family_receivers() {
    let context = TestContext::new();
    let families = [
        ("document", crate::objects::HANDLE_DOCUMENT),
        ("node", crate::objects::HANDLE_NODE),
        ("collection", crate::objects::HANDLE_COLLECTION),
        ("implementation", crate::objects::HANDLE_IMPLEMENTATION),
        ("token-list", crate::objects::HANDLE_TOKEN_LIST),
        ("xpath", crate::objects::HANDLE_XPATH),
        ("namespace-node", crate::objects::HANDLE_NAMESPACE_NODE),
        ("simplexml", crate::objects::HANDLE_SIMPLEXML),
    ];
    for (family, kind) in families {
        let request = request_bytes(
            RequestHeader {
                opcode: opcode("property-get:domnode::$nodeName"),
                receiver: forged_handle(kind),
                ..ping_header()
            },
            &[],
            &[],
        );
        assert_rejected_without_native_mutation(
            context.id(),
            &format!("handle-forged-{family}"),
            &request,
            STATUS_ABI_ERROR,
        );
    }
}

/// Verifies context-bound handles cannot be replayed by a different PHP execution context.
#[test]
fn abi_cross_context_handle_is_rejected_without_mutating_either_context() {
    let source = TestContext::new();
    let target = TestContext::new();
    let document = legacy_document(source.id());
    let request = request_bytes(
        RequestHeader {
            opcode: opcode("property-get:domdocument::$documentURI"),
            receiver: document,
            ..ping_header()
        },
        &[],
        &[],
    );
    let source_before = native_state(source.id());
    assert_rejected_without_native_mutation(
        target.id(),
        "handle-cross-context",
        &request,
        STATUS_ABI_ERROR,
    );
    assert_eq!(native_state(source.id()), source_before, "source context mutation");
}

/// Verifies stale handles fail after their wrapper has been released and cannot revive a slot.
#[test]
fn abi_stale_handle_is_rejected_without_native_mutation() {
    let context = TestContext::new();
    let document = legacy_document(context.id());
    let release = request_bytes(
        RequestHeader {
            opcode: opcode("internal:bridge.wrapper.release"),
            receiver: document,
            ..ping_header()
        },
        &[],
        &[],
    );
    let (status, released) = call(context.id(), &release);
    assert_eq!(status, STATUS_OK, "release source wrapper");
    release_result(context.id(), &released);
    let stale_use = request_bytes(
        RequestHeader {
            opcode: opcode("property-get:domdocument::$documentURI"),
            receiver: document,
            ..ping_header()
        },
        &[],
        &[],
    );
    assert_rejected_without_native_mutation(
        context.id(),
        "handle-stale",
        &stale_use,
        STATUS_ABI_ERROR,
    );
}

/// Verifies a node from another document reports PHP's Wrong Document result without mutation.
#[test]
fn abi_cross_document_node_is_a_owned_wrong_document_result() {
    let context = TestContext::new();
    let source = legacy_document(context.id());
    let target = legacy_document(context.id());
    let name = b"foreign";
    let create = request_bytes(
        RequestHeader {
            opcode: opcode("method:domdocument::createelement"),
            receiver: source,
            value_count: 1,
            byte_count: name.len() as u64,
            ..ping_header()
        },
        &[Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: name.len() as u64,
        }],
        name,
    );
    let (status, created) = call(context.id(), &create);
    assert_eq!(status, STATUS_OK, "foreign node construction");
    let foreign = created.payload0;
    release_result(context.id(), &created);
    let before = native_state(context.id());
    let append = request_bytes(
        RequestHeader {
            opcode: opcode("method:domnode::appendchild"),
            receiver: target,
            value_count: 1,
            ..ping_header()
        },
        &[Value {
            tag: VALUE_BRIDGE_HANDLE,
            flags: 0,
            payload0: foreign,
            payload1: 0,
        }],
        &[],
    );
    let (status, result) = call(context.id(), &append);
    assert_eq!(status, STATUS_OK, "bridge must publish PHP throw results");
    assert_eq!(result.status, crate::abi::STATUS_THROW, "Wrong Document status");
    assert_eq!(result.php_error_kind, crate::abi::PHP_ERROR_KIND_DOM_EXCEPTION);
    assert_eq!(result.dom_exception_code, 4, "Wrong Document DOM code");
    assert_ne!(result.result_id, 0, "cross-document owned result id");
    assert!(!result.bytes_ptr.is_null(), "cross-document diagnostic pointer");
    let diagnostic = unsafe {
        std::slice::from_raw_parts(result.bytes_ptr, result.bytes_len as usize)
    };
    assert_eq!(diagnostic, b"Wrong Document Error", "cross-document diagnostic");
    release_result(context.id(), &result);
    assert_eq!(native_state(context.id()), before, "cross-document mutation");
}

/// Verifies a legacy wrapper cannot be dispatched through a modern-only document operation.
#[test]
fn abi_cross_family_document_handle_is_rejected_without_native_mutation() {
    let context = TestContext::new();
    let legacy = legacy_document(context.id());
    let request = request_bytes(
        RequestHeader {
            opcode: opcode("method:dom\\xmldocument::savexml"),
            receiver: legacy,
            ..ping_header()
        },
        &[],
        &[],
    );
    assert_rejected_without_native_mutation(
        context.id(),
        "handle-cross-family",
        &request,
        STATUS_ABI_ERROR,
    );
}

/// Verifies double, unknown, and foreign result releases are production no-ops for live contexts.
#[test]
fn abi_invalid_result_releases_do_not_destroy_live_context_or_results() {
    let owner = TestContext::new();
    let foreign = TestContext::new();
    let ping = request_bytes(ping_header(), &[], &[]);
    let (status, first) = call(owner.id(), &ping);
    assert_eq!(status, STATUS_OK, "first live result");
    let first_id = first.result_id;
    release_result(owner.id(), &first);
    crate::elephc_dom_result_release(owner.id(), first_id);
    crate::elephc_dom_result_release(owner.id(), u64::MAX);
    crate::elephc_dom_result_release(foreign.id(), first_id);
    let (status, second) = call(owner.id(), &ping);
    assert_eq!(status, STATUS_OK, "owner remains callable after invalid releases");
    assert_ne!(second.result_id, 0, "new live result ownership");
    release_result(owner.id(), &second);
}

/// Documents test-only hooks still required to execute panic and allocation containment cases.
///
/// No production hook is added here: section 5.8 requires an explicit controlled injection
/// surface owned by the bridge implementation, and none is currently exposed to this module.
#[test]
#[ignore = "blocked by missing test-only panic/allocation injection and invalid-release counter accessors"]
fn abi_panic_allocation_and_invalid_release_instrumentation_contract() {
    let required_hooks = [
        "panic injection at each exported entry point",
        "recoverable native allocation-failure injection",
        "structured invalid-release event counter by context",
    ];
    assert!(required_hooks.is_empty(), "implement the listed test-only bridge hooks");
}
