//! Purpose:
//! Routes stable internal-extension opcodes to focused document, node, and libxml dispatchers.
//! Owns canonical native-handle lookup and flat bridge-result construction.
//!
//! Called from:
//! - `crate::exports::elephc_dom_call()` after complete flat-message validation.
//!
//! Key details:
//! - Receiver handles are generation- and kind-checked before native pointer use.
//! - Native pointer results preserve PHP wrapper identity through per-context caches.

mod character_data;
mod class_collection;
mod collection;
mod document;
mod document_config;
mod document_fragment;
mod document_html;
mod document_io;
mod document_validation;
mod document_xinclude;
mod document_type;
mod element;
mod entity;
mod element_adjacent;
mod element_markup;
mod implementation;
mod libxml;
mod lifecycle;
mod namespace_node;
mod node;
mod node_c14n;
mod node_constructors;
mod node_mutation;
mod node_rename;
mod node_values;
mod register_node_class;
mod reentrant;
mod routes;
mod selector;
mod simplexml;
mod token_list;
mod xpath;

use std::rc::Rc;

use crate::abi::{
    Diagnostic, Value, DIAGNOSTIC_FLAG_CALLSITE_CONTEXT,
    DIAGNOSTIC_FLAG_CALLSITE_LOCATION, VALUE_ARRAY, VALUE_BOOL, VALUE_BRIDGE_HANDLE,
    VALUE_BYTES, VALUE_CALLABLE, VALUE_FLOAT, VALUE_INT, VALUE_NULL, VALUE_OBJECT,
};

use crate::context::{Context, ResultFrame};
use crate::handles::handle_kind;
use crate::objects::{
    CollectionKind, CollectionObject, DocumentFamily, DocumentGraph,
    DocumentObject, LibxmlErrorObject, NativeObject, NamespaceNodeAllocation,
    NamespaceNodeObject, NodeObject,
    HANDLE_COLLECTION, HANDLE_DOCUMENT, HANDLE_IMPLEMENTATION, HANDLE_NAMESPACE_NODE,
    HANDLE_NODE, HANDLE_SIMPLEXML, HANDLE_TOKEN_LIST, HANDLE_XPATH,
    VALUE_OBJECT_LIBXML_ERROR,
    VALUE_OBJECT_NAMESPACE_INFO,
};
use crate::request::Request;

/// One successfully materialized bridge result and its public value tag.
pub(crate) struct DispatchResult {
    pub(crate) frame: ResultFrame,
    pub(crate) value_tag: u32,
}

impl DispatchResult {
    /// Builds a null operation result.
    fn null() -> Self {
        Self {
            frame: ResultFrame::null(),
            value_tag: VALUE_NULL,
        }
    }

    /// Builds a boolean operation result.
    fn boolean(value: bool) -> Self {
        Self {
            frame: ResultFrame::boolean(value),
            value_tag: VALUE_BOOL,
        }
    }

    /// Builds an integer scalar operation result.
    fn integer(value: i64) -> Self {
        Self {
            frame: ResultFrame {
                payload0: value as u64,
                ..ResultFrame::null()
            },
            value_tag: VALUE_INT,
        }
    }

    /// Builds a floating-point scalar operation result.
    fn float(value: f64) -> Self {
        Self {
            frame: ResultFrame {
                payload0: value.to_bits(),
                ..ResultFrame::null()
            },
            value_tag: VALUE_FLOAT,
        }
    }

    /// Builds an owned byte-string operation result.
    fn bytes(value: Vec<u8>) -> Self {
        Self {
            frame: ResultFrame::bytes(value),
            value_tag: VALUE_BYTES,
        }
    }

    /// Builds one indexed array of owned byte strings.
    fn byte_strings(items: Vec<Vec<u8>>) -> Self {
        let mut values = Vec::with_capacity(items.len());
        let mut bytes = Vec::new();
        for item in items {
            let offset = bytes.len();
            let length = item.len();
            bytes.extend_from_slice(&item);
            values.push(Value {
                tag: VALUE_BYTES,
                flags: 0,
                payload0: offset as u64,
                payload1: length as u64,
            });
        }
        Self {
            frame: ResultFrame::array(values.len(), values, bytes),
            value_tag: VALUE_ARRAY,
        }
    }

    /// Builds an opaque bridge-object operation result.
    fn bridge_handle(handle: u64) -> Self {
        Self {
            frame: ResultFrame::bridge_handle(handle),
            value_tag: VALUE_BRIDGE_HANDLE,
        }
    }

    /// Builds an opaque bridge handle with its stable concrete-wrapper discriminator.
    fn typed_bridge_handle(handle: u64, wrapper_kind: u64) -> Self {
        Self {
            frame: ResultFrame {
                payload1: wrapper_kind,
                ..ResultFrame::bridge_handle(handle)
            },
            value_tag: VALUE_BRIDGE_HANDLE,
        }
    }

    /// Builds a typed wrapper result carrying php-src-order eager XPath members.
    fn typed_bridge_handle_with_values(
        handle: u64,
        wrapper_kind: u64,
        values: Vec<Value>,
    ) -> Self {
        Self {
            frame: ResultFrame {
                payload1: wrapper_kind,
                values: values.into_boxed_slice(),
                ..ResultFrame::bridge_handle(handle)
            },
            value_tag: VALUE_BRIDGE_HANDLE,
        }
    }

    /// Builds one retained PHP callable descriptor result.
    fn callable(handle: u64) -> Self {
        Self {
            frame: ResultFrame::host_handle(handle),
            value_tag: VALUE_CALLABLE,
        }
    }

    /// Builds a catchable PHP `DOMException` result with exact code and message.
    fn dom_exception(code: i32, message: &[u8]) -> Self {
        Self {
            frame: ResultFrame::dom_exception(code, message),
            value_tag: VALUE_NULL,
        }
    }

    /// Builds a catchable PHP `ValueError` result with its exact message.
    fn value_error(message: &[u8]) -> Self {
        Self {
            frame: ResultFrame::value_error(message),
            value_tag: VALUE_NULL,
        }
    }

    /// Builds a catchable PHP base `Error` result with its exact message.
    fn error(message: &[u8]) -> Self {
        Self {
            frame: ResultFrame::error(message),
            value_tag: VALUE_NULL,
        }
    }

    /// Builds a catchable PHP base `Exception` result with its exact message.
    fn exception(message: &[u8]) -> Self {
        Self {
            frame: ResultFrame::exception(message),
            value_tag: VALUE_NULL,
        }
    }

    /// Builds a catchable PHP `TypeError` result with its exact message.
    fn type_error(message: &[u8]) -> Self {
        Self {
            frame: ResultFrame::type_error(message),
            value_tag: VALUE_NULL,
        }
    }

    /// Builds a signal that rethrows the exact Throwable captured around a PHP host callback.
    fn pending_host_throwable() -> Self {
        Self {
            frame: ResultFrame::pending_host_throwable(),
            value_tag: VALUE_NULL,
        }
    }

    /// Defers one callable release until the exported ABI has dropped its mutable context borrow.
    fn with_pending_host_action(
        mut self,
        action: crate::context::PendingHostAction,
    ) -> Self {
        self.frame.pending_host_actions.push(action);
        self
    }

    /// Attaches one complete PHP warning message to an otherwise ordinary result.
    fn with_warning(mut self, message: &[u8]) -> Self {
        let mut bytes = self.frame.bytes.into_vec();
        let mut diagnostics = self.frame.diagnostics.into_vec();
        let message_offset = bytes.len();
        bytes.extend_from_slice(message);
        let file_offset = bytes.len();
        self.frame.bytes = bytes.into_boxed_slice();
        diagnostics.push(Diagnostic {
            level: 2,
            domain: 0,
            code: 0,
            reserved: 0,
            line: 0,
            column: 0,
            message_offset: message_offset as u64,
            message_len: message.len() as u64,
            file_offset: file_offset as u64,
            file_len: 0,
        });
        self.frame.diagnostics = diagnostics.into_boxed_slice();
        self
    }

    /// Attaches warning detail that the compiler decorates with PHP call-site context.
    fn with_callsite_warning(mut self, detail: &[u8]) -> Self {
        let mut bytes = self.frame.bytes.into_vec();
        let mut diagnostics = self.frame.diagnostics.into_vec();
        let message_offset = bytes.len();
        bytes.extend_from_slice(detail);
        let file_offset = bytes.len();
        self.frame.bytes = bytes.into_boxed_slice();
        diagnostics.push(Diagnostic {
            level: 2,
            domain: 0,
            code: 0,
            reserved: DIAGNOSTIC_FLAG_CALLSITE_CONTEXT,
            line: 0,
            column: 0,
            message_offset: message_offset as u64,
            message_len: detail.len() as u64,
            file_offset: file_offset as u64,
            file_len: 0,
        });
        self.frame.diagnostics = diagnostics.into_boxed_slice();
        self
    }

    /// Attaches a complete warning prefix/detail that needs only source location decoration.
    fn with_callsite_location_warning(mut self, detail: &[u8]) -> Self {
        let mut bytes = self.frame.bytes.into_vec();
        let mut diagnostics = self.frame.diagnostics.into_vec();
        let message_offset = bytes.len();
        bytes.extend_from_slice(detail);
        let file_offset = bytes.len();
        self.frame.bytes = bytes.into_boxed_slice();
        diagnostics.push(Diagnostic {
            level: 2,
            domain: 0,
            code: 0,
            reserved: DIAGNOSTIC_FLAG_CALLSITE_LOCATION,
            line: 0,
            column: 0,
            message_offset: message_offset as u64,
            message_len: detail.len() as u64,
            file_offset: file_offset as u64,
            file_len: 0,
        });
        self.frame.diagnostics = diagnostics.into_boxed_slice();
        self
    }

    /// Builds one indexed result of detached PHP `LibXMLError` value objects.
    fn libxml_errors(errors: Vec<LibxmlErrorObject>) -> Self {
        let item_count = errors.len();
        let mut values = vec![
            Value {
                tag: VALUE_NULL,
                flags: 0,
                payload0: 0,
                payload1: 0,
            };
            item_count
        ];
        let mut bytes = Vec::new();
        for (index, error) in errors.into_iter().enumerate() {
            let field_start = values.len();
            libxml::append_error_fields(&error, &mut values, &mut bytes);
            values[index] = Value {
                tag: VALUE_OBJECT,
                flags: VALUE_OBJECT_LIBXML_ERROR,
                payload0: field_start as u64,
                payload1: 6,
            };
        }
        Self {
            frame: ResultFrame::array(item_count, values, bytes),
            value_tag: VALUE_ARRAY,
        }
    }

    /// Builds ordered detached `Dom\NamespaceInfo` values with canonical element fields.
    fn namespace_infos(
        context: &mut Context,
        graph: Rc<DocumentGraph>,
        infos: Vec<crate::native::NamespaceInfo>,
    ) -> Result<Self, ()> {
        let item_count = infos.len();
        let mut values = vec![
            Value {
                tag: VALUE_NULL,
                flags: 0,
                payload0: 0,
                payload1: 0,
            };
            item_count
        ];
        let mut bytes = Vec::new();
        for (index, info) in infos.into_iter().enumerate() {
            let field_start = values.len();
            values.push(optional_bytes_value(info.prefix, &mut bytes));
            values.push(optional_bytes_value(info.namespace_uri, &mut bytes));
            let (handle, kind) = canonical_pointer_handle(
                context,
                info.element,
                Rc::clone(&graph),
            )?;
            values.push(Value {
                tag: VALUE_BRIDGE_HANDLE,
                flags: 0,
                payload0: handle,
                payload1: kind,
            });
            values[index] = Value {
                tag: VALUE_OBJECT,
                flags: VALUE_OBJECT_NAMESPACE_INFO,
                payload0: field_start as u64,
                payload1: 3,
            };
        }
        Ok(Self {
            frame: ResultFrame::array(item_count, values, bytes),
            value_tag: VALUE_ARRAY,
        })
    }

    /// Builds one detached PHP `LibXMLError` value object.
    fn libxml_error(error: LibxmlErrorObject) -> Self {
        let mut values = Vec::with_capacity(6);
        let mut bytes = Vec::new();
        libxml::append_error_fields(&error, &mut values, &mut bytes);
        Self {
            frame: ResultFrame::object(values, bytes),
            value_tag: VALUE_OBJECT,
        }
    }

    /// Attaches ordered PHP warning bytes for one visible libxml parse-error sequence.
    fn with_libxml_warnings(
        self,
        method: &[u8],
        errors: &[LibxmlErrorObject],
    ) -> Self {
        self.with_libxml_warning_details(method, errors, false)
    }

    /// Attaches libxml warning details whose PHP file and line come from the call site.
    fn with_callsite_location_libxml_warnings(
        self,
        method: &[u8],
        errors: &[LibxmlErrorObject],
    ) -> Self {
        self.with_libxml_warning_details(method, errors, true)
    }

    /// Attaches ordered libxml warning details with their requested compiler decoration mode.
    fn with_libxml_warning_details(
        mut self,
        method: &[u8],
        errors: &[LibxmlErrorObject],
        callsite_location: bool,
    ) -> Self {
        let mut bytes = self.frame.bytes.into_vec();
        let mut diagnostics = self.frame.diagnostics.into_vec();
        diagnostics.reserve(errors.len());
        for error in errors {
            let mut error_message = error.message.as_slice();
            while let Some(trimmed) = error_message.strip_suffix(b"\n") {
                error_message = trimmed;
            }
            let message_offset = bytes.len();
            bytes.extend_from_slice(b"Warning: ");
            bytes.extend_from_slice(method);
            bytes.extend_from_slice(b"(): ");
            bytes.extend_from_slice(error_message);
            if !callsite_location {
                bytes.push(b'\n');
            }
            let message_len = bytes.len() - message_offset;
            let file_offset = bytes.len();
            bytes.extend_from_slice(&error.file);
            diagnostics.push(Diagnostic {
                level: error.level.max(0) as u32,
                domain: error.domain.max(0) as u32,
                code: error.code as i32,
                reserved: if callsite_location {
                    DIAGNOSTIC_FLAG_CALLSITE_LOCATION
                } else {
                    0
                },
                line: error.line.max(0) as u64,
                column: error.column.max(0) as u64,
                message_offset: message_offset as u64,
                message_len: message_len as u64,
                file_offset: file_offset as u64,
                file_len: error.file.len() as u64,
            });
        }
        self.frame.bytes = bytes.into_boxed_slice();
        self.frame.diagnostics = diagnostics.into_boxed_slice();
        self
    }

    /// Attaches libxml2 parser warnings with PHP's level filters and source suffix.
    fn with_libxml_parser_warnings(
        mut self,
        method: &[u8],
        errors: &[LibxmlErrorObject],
        options: i32,
    ) -> Self {
        const NO_ERROR: i32 = 32;
        const NO_WARNING: i32 = 64;

        let mut bytes = self.frame.bytes.into_vec();
        let mut diagnostics = self.frame.diagnostics.into_vec();
        diagnostics.reserve(errors.len());
        for error in errors {
            if (error.level == 1 && options & NO_WARNING != 0)
                || (error.level != 1 && options & NO_ERROR != 0)
            {
                continue;
            }
            let mut error_message = error.message.as_slice();
            while let Some(trimmed) = error_message.strip_suffix(b"\n") {
                error_message = trimmed;
            }
            let message_offset = bytes.len();
            bytes.extend_from_slice(b"Warning: ");
            bytes.extend_from_slice(method);
            bytes.extend_from_slice(b"(): ");
            bytes.extend_from_slice(error_message);
            bytes.extend_from_slice(b" in ");
            if error.file.is_empty() {
                bytes.extend_from_slice(b"Entity");
            } else {
                bytes.extend_from_slice(&error.file);
            }
            bytes.extend_from_slice(b", line: ");
            bytes.extend_from_slice(error.line.max(0).to_string().as_bytes());
            bytes.push(b'\n');
            let message_len = bytes.len() - message_offset;
            let file_offset = bytes.len();
            bytes.extend_from_slice(&error.file);
            diagnostics.push(Diagnostic {
                level: error.level.max(0) as u32,
                domain: error.domain.max(0) as u32,
                code: error.code as i32,
                reserved: 0,
                line: error.line.max(0) as u64,
                column: error.column.max(0) as u64,
                message_offset: message_offset as u64,
                message_len: message_len as u64,
                file_offset: file_offset as u64,
                file_len: error.file.len() as u64,
            });
        }
        self.frame.bytes = bytes.into_boxed_slice();
        self.frame.diagnostics = diagnostics.into_boxed_slice();
        self
    }
}

/// Appends one nullable byte field to a flat result and returns its ABI descriptor.
fn optional_bytes_value(value: Option<Vec<u8>>, bytes: &mut Vec<u8>) -> Value {
    let Some(value) = value else {
        return Value {
            tag: VALUE_NULL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        };
    };
    let offset = bytes.len();
    let length = value.len();
    bytes.extend_from_slice(&value);
    Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: offset as u64,
        payload1: length as u64,
    }
}

/// Executes one generated public opcode or rejects an operation not yet implemented.
pub(crate) fn dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    routes::dispatch(context, request)
}

/// Runs operations that may call PHP without holding the context's mutable `RefCell` borrow.
pub(crate) fn dispatch_reentrant(
    context_id: u64,
    operation_key: &str,
    request: &Request,
) -> Result<Option<DispatchResult>, ()> {
    reentrant::dispatch(context_id, operation_key, request)
}

/// Releases one wrapper-owned native document or node handle exactly once.
fn release_wrapper(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let mut result = DispatchResult::null();
    match handle_kind(request.header.receiver).map_err(|_| ())? {
        HANDLE_DOCUMENT => {
            let object = context
                .native_objects
                .remove(request.header.receiver, HANDLE_DOCUMENT)
                .map_err(|_| ())?;
            let pointer = object.document().ok_or(())?.pointer();
            context.document_handles.remove(&pointer);
        }
        HANDLE_NODE => {
            let object = context
                .native_objects
                .remove(request.header.receiver, HANDLE_NODE)
                .map_err(|_| ())?;
            if let Some(node) = object.node() {
                context.node_handles.remove(&node.pointer());
            } else if !object.is_invalid_node() {
                return Err(());
            }
        }
        HANDLE_COLLECTION => {
            context
                .native_objects
                .remove(request.header.receiver, HANDLE_COLLECTION)
                .map_err(|_| ())?
                .collection()
                .ok_or(())?;
        }
        HANDLE_IMPLEMENTATION => {
            let object = context
                .native_objects
                .remove(request.header.receiver, HANDLE_IMPLEMENTATION)
                .map_err(|_| ())?;
            if let Some(pointer) = object
                .implementation()
                .ok_or(())?
                .associated_document()
            {
                context.implementation_handles.remove(&pointer);
            }
        }
        HANDLE_TOKEN_LIST => {
            let object = context
                .native_objects
                .remove(request.header.receiver, HANDLE_TOKEN_LIST)
                .map_err(|_| ())?;
            if let Some(token_list) = object.token_list() {
                context.token_list_handles.remove(&token_list.element());
            } else if !object.is_invalid_token_list() {
                return Err(());
            }
        }
        HANDLE_XPATH => {
            let mut object = context
                .native_objects
                .remove(request.header.receiver, HANDLE_XPATH)
                .map_err(|_| ())?;
            let descriptors = object
                .xpath_mut()
                .ok_or(())?
                .take_callback_descriptors();
            for descriptor in descriptors {
                result = result.with_pending_host_action(
                    crate::context::PendingHostAction::ReleaseCallable {
                        host: context.host,
                        descriptor,
                    },
                );
            }
            object
                .xpath()
                .ok_or(())?;
        }
        HANDLE_NAMESPACE_NODE => {
            let fake = context
                .native_objects
                .get(request.header.receiver, HANDLE_NAMESPACE_NODE)
                .ok()
                .and_then(NativeObject::namespace_node)
                .map(|namespace_node| namespace_node.pointer());
            if let Some(fake) = fake {
                context.namespace_node_handles.remove(&fake);
            }
            let object = context
                .native_objects
                .remove(request.header.receiver, HANDLE_NAMESPACE_NODE)
                .map_err(|_| ())?;
            if object.namespace_node().is_none()
                && !object.is_invalid_namespace_node()
            {
                return Err(());
            }
        }
        HANDLE_SIMPLEXML => {
            context.release_simplexml_external(request.header.receiver)?;
        }
        _ => return Err(()),
    }
    Ok(result)
}

/// Validates and returns one retained wrapper handle.
fn retain_wrapper(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    match handle_kind(request.header.receiver).map_err(|_| ())? {
        HANDLE_DOCUMENT => {
            document(context, request.header.receiver)?;
        }
        HANDLE_NODE => {
            let object = context
                .native_objects
                .get(request.header.receiver, HANDLE_NODE)
                .map_err(|_| ())?;
            if object.node().is_none() && !object.is_invalid_node() {
                return Err(());
            }
        }
        HANDLE_COLLECTION => {
            collection(context, request.header.receiver)?;
        }
        HANDLE_IMPLEMENTATION => {
            context
                .native_objects
                .get(request.header.receiver, HANDLE_IMPLEMENTATION)
                .map_err(|_| ())?
                .implementation()
                .ok_or(())?;
        }
        HANDLE_TOKEN_LIST => {
            let object = context
                .native_objects
                .get(request.header.receiver, HANDLE_TOKEN_LIST)
                .map_err(|_| ())?;
            if object.token_list().is_none()
                && !object.is_invalid_token_list()
            {
                return Err(());
            }
        }
        HANDLE_XPATH => {
            xpath(context, request.header.receiver)?;
        }
        HANDLE_NAMESPACE_NODE => {
            let object = context
                .native_objects
                .get(request.header.receiver, HANDLE_NAMESPACE_NODE)
                .map_err(|_| ())?;
            if object.namespace_node().is_none()
                && !object.is_invalid_namespace_node()
            {
                return Err(());
            }
        }
        HANDLE_SIMPLEXML => {
            context
                .native_objects
                .get(request.header.receiver, HANDLE_SIMPLEXML)
                .map_err(|_| ())?
                .simplexml()
                .ok_or(())?;
        }
        _ => return Err(()),
    }
    Ok(DispatchResult::bridge_handle(request.header.receiver))
}

/// Borrows one validated native document receiver.
fn document(context: &Context, handle: u64) -> Result<&DocumentObject, ()> {
    context
        .native_objects
        .get(handle, HANDLE_DOCUMENT)
        .map_err(|_| ())?
        .document()
        .ok_or(())
}

/// Mutably borrows one validated native document receiver.
fn document_mut(
    context: &mut Context,
    handle: u64,
) -> Result<&mut DocumentObject, ()> {
    context
        .native_objects
        .get_mut(handle, HANDLE_DOCUMENT)
        .map_err(|_| ())?
        .document_mut()
        .ok_or(())
}

/// Borrows one validated native node receiver.
fn node(context: &Context, handle: u64) -> Result<&NodeObject, ()> {
    context
        .native_objects
        .get(handle, HANDLE_NODE)
        .map_err(|_| ())?
        .node()
        .ok_or(())
}

/// Borrows one validated live collection receiver.
fn collection(context: &Context, handle: u64) -> Result<&CollectionObject, ()> {
    context
        .native_objects
        .get(handle, HANDLE_COLLECTION)
        .map_err(|_| ())?
        .collection()
        .ok_or(())
}

/// Mutably borrows one validated live collection receiver.
fn collection_mut(
    context: &mut Context,
    handle: u64,
) -> Result<&mut CollectionObject, ()> {
    context
        .native_objects
        .get_mut(handle, HANDLE_COLLECTION)
        .map_err(|_| ())?
        .collection_mut()
        .ok_or(())
}

/// Borrows one validated modern class-token list receiver.
fn token_list(
    context: &Context,
    handle: u64,
) -> Result<&crate::objects::TokenListObject, ()> {
    context
        .native_objects
        .get(handle, HANDLE_TOKEN_LIST)
        .map_err(|_| ())?
        .token_list()
        .ok_or(())
}

/// Borrows one validated legacy or modern XPath context receiver.
fn xpath(
    context: &Context,
    handle: u64,
) -> Result<&crate::objects::XPathObject, ()> {
    context
        .native_objects
        .get(handle, HANDLE_XPATH)
        .map_err(|_| ())?
        .xpath()
        .ok_or(())
}

/// Mutably borrows one validated legacy or modern XPath context receiver.
fn xpath_mut(
    context: &mut Context,
    handle: u64,
) -> Result<&mut crate::objects::XPathObject, ()> {
    context
        .native_objects
        .get_mut(handle, HANDLE_XPATH)
        .map_err(|_| ())?
        .xpath_mut()
        .ok_or(())
}

/// Borrows one validated namespace-declaration receiver.
fn namespace_node(
    context: &Context,
    handle: u64,
) -> Result<&crate::objects::NamespaceNodeObject, ()> {
    context
        .native_objects
        .get(handle, HANDLE_NAMESPACE_NODE)
        .map_err(|_| ())?
        .namespace_node()
        .ok_or(())
}

/// Resolves a document or node receiver into its pointer, graph, and document marker.
fn receiver_pointer_and_graph(
    context: &Context,
    handle: u64,
) -> Result<(usize, Rc<DocumentGraph>, bool), ()> {
    match handle_kind(handle).map_err(|_| ())? {
        HANDLE_DOCUMENT => {
            let document = document(context, handle)?;
            Ok((document.pointer(), document.graph(), true))
        }
        HANDLE_NODE => {
            let node = node(context, handle)?;
            Ok((node.pointer(), node.document(), false))
        }
        _ => Err(()),
    }
}

/// Returns the live canonical handle for a native node pointer, inserting it if absent.
fn canonical_node_handle(
    context: &mut Context,
    pointer: usize,
    document: Rc<DocumentGraph>,
    wrapper_kind: u64,
) -> u64 {
    if let Some(handle) = context.node_handles.get(&pointer).copied() {
        let valid = context
            .native_objects
            .get(handle, HANDLE_NODE)
            .ok()
            .and_then(NativeObject::node)
            .is_some_and(|node| node.pointer() == pointer);
        if valid {
            return handle;
        }
        context.node_handles.remove(&pointer);
    }
    let handle = context.native_objects.insert(
        HANDLE_NODE,
        NativeObject::Node(NodeObject::new(pointer, document, wrapper_kind)),
    );
    context.node_handles.insert(pointer, handle);
    handle
}

/// Returns a fresh wrapper for one synthesized DTD notation fake node.
///
/// php-src never interns notation wrappers: every DTD-map lookup owns a distinct
/// fake `XML_NOTATION_NODE`. The node has no native `doc` pointer, so it bypasses
/// canonical pointer validation and its `NodeObject` owns the allocation directly.
fn notation_pointer_result(
    context: &mut Context,
    pointer: usize,
    document: Rc<DocumentGraph>,
) -> DispatchResult {
    let native_kind = wrapper_kind(&document, pointer);
    let kind =
        register_node_class::mapped_wrapper_kind(context, &document, native_kind);
    let handle = context.native_objects.insert(
        HANDLE_NODE,
        NativeObject::Node(NodeObject::notation(pointer, document, kind)),
    );
    context.node_handles.insert(pointer, handle);
    DispatchResult::typed_bridge_handle(handle, kind)
}

/// Registers one directly constructed legacy node with its native owner hidden from PHP.
fn direct_node_result(
    context: &mut Context,
    pointer: usize,
    document: Rc<DocumentGraph>,
) -> DispatchResult {
    context.register_detached_root(pointer, Rc::clone(&document));
    let kind = wrapper_kind(&document, pointer);
    let handle = context.native_objects.insert(
        HANDLE_NODE,
        NativeObject::Node(NodeObject::without_owner_document(
            pointer,
            document,
            kind,
        )),
    );
    context.node_handles.insert(pointer, handle);
    DispatchResult::bridge_handle(handle)
}

/// Rehomes one live node handle and any detached-root owner after modern adoption.
fn rehome_node_handle(
    context: &mut Context,
    handle: u64,
    document: Rc<DocumentGraph>,
) -> Result<(), ()> {
    let pointer = context
        .native_objects
        .get_mut(handle, HANDLE_NODE)
        .map_err(|_| ())?
        .node_mut()
        .ok_or(())?
        .pointer();
    context
        .native_objects
        .get_mut(handle, HANDLE_NODE)
        .map_err(|_| ())?
        .node_mut()
        .ok_or(())?
        .replace_document(Rc::clone(&document));
    context.rehome_detached_root(pointer, document);
    Ok(())
}

/// Rehomes every materialized wrapper rooted at one adopted native subtree.
fn rehome_subtree_handles(
    context: &mut Context,
    root: usize,
    document: Rc<DocumentGraph>,
) -> Result<(), ()> {
    let handles = context
        .node_handles
        .iter()
        .filter_map(|(pointer, handle)| {
            (*pointer == root || crate::native::node_contains(root, *pointer))
                .then_some(*handle)
        })
        .collect::<Vec<_>>();
    for handle in handles {
        rehome_node_handle(context, handle, Rc::clone(&document))?;
    }
    let token_list_handles = context
        .token_list_handles
        .iter()
        .filter_map(|(pointer, handle)| {
            (*pointer == root || crate::native::node_contains(root, *pointer))
                .then_some(*handle)
        })
        .collect::<Vec<_>>();
    for handle in token_list_handles {
        context
            .native_objects
            .get_mut(handle, HANDLE_TOKEN_LIST)
            .map_err(|_| ())?
            .token_list_mut()
            .ok_or(())?
            .replace_document(Rc::clone(&document));
    }
    Ok(())
}

/// Returns the live canonical handle for an authoritative document graph.
fn canonical_document_handle(
    context: &mut Context,
    graph: Rc<DocumentGraph>,
) -> u64 {
    let pointer = graph.pointer();
    if let Some(handle) = context.document_handles.get(&pointer).copied() {
        let valid = context
            .native_objects
            .get(handle, HANDLE_DOCUMENT)
            .ok()
            .and_then(NativeObject::document)
            .is_some_and(|document| document.pointer() == pointer);
        if valid {
            return handle;
        }
        context.document_handles.remove(&pointer);
    }
    let handle = context.native_objects.insert(
        HANDLE_DOCUMENT,
        NativeObject::Document(DocumentObject::from_graph(graph)),
    );
    context.document_handles.insert(pointer, handle);
    handle
}

/// Returns a canonical document or node handle for one pointer in an authoritative graph.
fn canonical_pointer_result(
    context: &mut Context,
    pointer: usize,
    graph: Rc<DocumentGraph>,
) -> Result<DispatchResult, ()> {
    let (handle, kind) = canonical_pointer_handle(context, pointer, graph)?;
    Ok(DispatchResult::typed_bridge_handle(handle, kind))
}

/// Returns one canonical native handle and its concrete wrapper discriminator.
fn canonical_pointer_handle(
    context: &mut Context,
    pointer: usize,
    graph: Rc<DocumentGraph>,
) -> Result<(u64, u64), ()> {
    if pointer == 0 {
        return Err(());
    }
    if crate::native::node_type(pointer) == 18 {
        let handle = canonical_namespace_node_handle(context, pointer, graph, None);
        return Ok((handle, 118));
    }
    if pointer != graph.pointer() {
        let document = crate::native::node_document(pointer).ok_or(())?;
        if document != graph.pointer() {
            return Err(());
        }
    }
    let native_kind = wrapper_kind(&graph, pointer);
    let kind =
        register_node_class::mapped_wrapper_kind(context, &graph, native_kind);
    let is_document = pointer == graph.pointer();
    let handle = if is_document {
        canonical_document_handle(context, graph)
    } else {
        canonical_node_handle(context, pointer, graph, kind)
    };
    let kind = if is_document {
        kind
    } else {
        node(context, handle)?.wrapper_kind()
    };
    Ok((handle, kind))
}

/// Returns the live canonical handle for one standalone namespace-declaration node.
///
/// Namespace fake nodes are owned by a shared `NamespaceNodeAllocation` retained by
/// the originating snapshot slot and every materialized wrapper, so the cache is
/// keyed by the fake pointer itself. Repeated `item()` calls return the same wrapper
/// while the slot keeps its allocation; after the wrapper is released the slot can
/// recreate it, and after the snapshot is released a live wrapper keeps the fake
/// node alive. When `allocation` is `None` a fresh owning allocation is created for
/// the pointer.
fn canonical_namespace_node_handle(
    context: &mut Context,
    pointer: usize,
    graph: Rc<DocumentGraph>,
    allocation: Option<Rc<NamespaceNodeAllocation>>,
) -> u64 {
    if let Some(handle) = context.namespace_node_handles.get(&pointer).copied() {
        let valid = context
            .native_objects
            .get(handle, HANDLE_NAMESPACE_NODE)
            .ok()
            .and_then(NativeObject::namespace_node)
            .is_some_and(|namespace_node| namespace_node.pointer() == pointer);
        if valid {
            return handle;
        }
        context.namespace_node_handles.remove(&pointer);
    }
    let allocation = allocation.unwrap_or_else(|| {
        let parent = crate::native::node_parent(pointer).unwrap_or(0);
        Rc::new(NamespaceNodeAllocation::new(pointer, parent))
    });
    let handle = context.native_objects.insert(
        HANDLE_NAMESPACE_NODE,
        NativeObject::NamespaceNode(NamespaceNodeObject::new(allocation, graph)),
    );
    context.namespace_node_handles.insert(pointer, handle);
    handle
}

/// Builds the typed bridge-handle result for one namespace-declaration node.
fn canonical_namespace_node_result(
    context: &mut Context,
    pointer: usize,
    graph: Rc<DocumentGraph>,
    allocation: Option<Rc<NamespaceNodeAllocation>>,
) -> DispatchResult {
    let handle = canonical_namespace_node_handle(context, pointer, graph, allocation);
    DispatchResult::typed_bridge_handle(handle, 118)
}

/// Returns the canonical bridge handle and stable wrapper kind for an XPath callback node.
pub(crate) fn xpath_callback_wrapper(
    context: &mut Context,
    graph: Rc<DocumentGraph>,
    pointer: usize,
) -> Result<(u64, u64), ()> {
    canonical_pointer_handle(context, pointer, graph)
}

/// Resolves one callback-returned DOM document or node handle to its live native pointer.
pub(crate) fn xpath_callback_pointer(
    context: &Context,
    handle: u64,
) -> Result<usize, ()> {
    receiver_pointer_and_graph(context, handle)
        .map(|(pointer, _, _)| pointer)
}

/// Computes the stable ABI wrapper kind from the document class and element namespace.
fn wrapper_kind(graph: &DocumentGraph, pointer: usize) -> u64 {
    if pointer == graph.pointer() {
        return match graph.family() {
            DocumentFamily::Legacy => 109,
            DocumentFamily::ModernXml => 209,
            DocumentFamily::ModernHtml => 313,
        };
    }
    let node_type = u64::from(crate::native::node_type(pointer));
    match graph.family() {
        DocumentFamily::Legacy => 100 + node_type,
        DocumentFamily::ModernXml | DocumentFamily::ModernHtml
            if node_type == 1
                && crate::native::node_is_html_element(pointer) =>
        {
            301
        }
        DocumentFamily::ModernXml | DocumentFamily::ModernHtml => {
            200 + node_type
        }
    }
}

/// Returns one qualified node name with HTML-document element casing.
fn node_name(
    graph: &DocumentGraph,
    pointer: usize,
) -> Result<Vec<u8>, ()> {
    let mut name = crate::native::node_name(pointer).ok_or(())?;
    if graph.family() == DocumentFamily::ModernHtml
        && crate::native::node_is_html_element(pointer)
    {
        name.make_ascii_uppercase();
    }
    Ok(name)
}

/// Registers one detached native root and returns its canonical bridge handle result.
fn register_detached_node(
    context: &mut Context,
    graph: Rc<DocumentGraph>,
    pointer: usize,
) -> Result<DispatchResult, ()> {
    context.register_detached_root(pointer, Rc::clone(&graph));
    canonical_pointer_result(context, pointer, graph)
}

/// Inserts one fresh live collection descriptor and returns its wrapper handle.
fn collection_result(
    context: &mut Context,
    root: usize,
    graph: Rc<DocumentGraph>,
    kind: CollectionKind,
) -> DispatchResult {
    let handle = context.native_objects.insert(
        HANDLE_COLLECTION,
        NativeObject::Collection(CollectionObject::new(root, graph, kind)),
    );
    DispatchResult::bridge_handle(handle)
}

/// Inserts one fresh collection and attaches eager XPath wrapper descriptors.
fn xpath_collection_result(
    context: &mut Context,
    root: usize,
    graph: Rc<DocumentGraph>,
    kind: CollectionKind,
    eager_wrappers: Vec<Value>,
) -> DispatchResult {
    let handle = context.native_objects.insert(
        HANDLE_COLLECTION,
        NativeObject::Collection(CollectionObject::new(root, graph, kind)),
    );
    DispatchResult::typed_bridge_handle_with_values(
        handle,
        0,
        eager_wrappers,
    )
}

/// Returns one supplied byte argument or a static default when omitted.
fn optional_bytes<'a>(
    request: &'a Request,
    index: usize,
    default: &'a [u8],
) -> Result<&'a [u8], ()> {
    if request.values.len() > index {
        request.byte_string(index)
    } else {
        Ok(default)
    }
}

/// Returns one supplied integer narrowed to C `int`, or a default when omitted.
fn optional_i32(request: &Request, index: usize, default: i32) -> Result<i32, ()> {
    if request.values.len() <= index {
        return Ok(default);
    }
    i32::try_from(request.integer(index)?).map_err(|_| ())
}

/// Builds PHP's canonical `DOMException` result for one standard numeric code.
fn dom_exception(code: i32) -> DispatchResult {
    let message: &[u8] = match code {
        1 => b"Index Size Error",
        3 => b"Hierarchy Request Error",
        4 => b"Wrong Document Error",
        5 => b"Invalid Character Error",
        7 => b"No Modification Allowed Error",
        8 => b"Not Found Error",
        9 => b"Not Supported Error",
        10 => b"Inuse Attribute Error",
        11 => b"Invalid State Error",
        12 => b"Syntax Error",
        14 => b"Namespace Error",
        _ => b"Unhandled Error",
    };
    DispatchResult::dom_exception(code, message)
}

/// Rejects unexpected receiver handles for constructors and static factories.
fn require_no_receiver(request: &Request) -> Result<(), ()> {
    (request.header.receiver == 0).then_some(()).ok_or(())
}

/// Rejects unexpected arguments for property reads.
fn require_no_values(request: &Request) -> Result<(), ()> {
    request.values.is_empty().then_some(()).ok_or(())
}

/// Rejects operations implemented entirely by ordinary compiler-managed PHP values.
fn reject_compiler_resident_operation(
    _request: &Request,
) -> Result<DispatchResult, ()> {
    Err(())
}
