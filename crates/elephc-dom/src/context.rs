//! Purpose:
//! Owns process-local DOM execution contexts and independently retained result frames.
//! Prevents stale context/result IDs and re-entrant calls from invalidating live result pointers.
//!
//! Called from:
//! - `crate::exports` after request boundary validation.
//!
//! Key details:
//! - Public IDs are monotonic and never raw pointers.
//! - Result buffers are boxed and remain stable while their result ID is registered.
//! - SimpleXML iterator data uses balanced internal and PHP-wrapper owners.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::abi::{
    Diagnostic, DomClassMetadataEntry, HostCall, ResultHeader, Value, ABI_VERSION,
    DOM_CLASS_NO_PARENT,
    PHP_ERROR_KIND_DOM_EXCEPTION, PHP_ERROR_KIND_ERROR,
    PHP_ERROR_KIND_EXCEPTION, PHP_ERROR_KIND_TYPE_ERROR,
    PHP_ERROR_KIND_PENDING_HOST_THROWABLE, PHP_ERROR_KIND_VALUE_ERROR,
    STATUS_OK, STATUS_THROW, VALUE_BYTES,
};
use crate::handles::HandleTable;
use crate::objects::{
    DocumentFamily, DocumentGraph, LibxmlErrorObject, NativeObject,
    SimpleXmlObject, HANDLE_DOCUMENT, HANDLE_NODE, HANDLE_SIMPLEXML,
};

/// One copied PHP class metadata row used by `registerNodeClass()` validation.
#[derive(Clone, Debug)]
pub(crate) struct ClassMeta {
    /// Declared spelling retained from the compiler's canonical class entry.
    pub(crate) canonical_name: Vec<u8>,
    /// Runtime class id assigned by the compiler.
    pub(crate) id: u64,
    /// Parent class id, or `DOM_CLASS_NO_PARENT` for a root class.
    parent_id: u64,
    /// Whether PHP forbids direct construction of this abstract class.
    pub(crate) is_abstract: bool,
}

/// Indexed class metadata used for case-insensitive lookup and parent-chain walks.
#[derive(Debug, Default)]
pub(crate) struct ClassMetadataTable {
    by_name: HashMap<Vec<u8>, ClassMeta>,
    by_id: HashMap<u64, ClassMeta>,
}

impl ClassMetadataTable {
    /// Builds the empty table installed in a newly allocated DOM context.
    fn new() -> Self {
        Self::default()
    }

    /// Replaces the table only after every foreign row passes structural validation.
    pub(crate) fn install(
        &mut self,
        entries: &[DomClassMetadataEntry],
    ) -> Result<(), ()> {
        let mut by_name = HashMap::with_capacity(entries.len());
        let mut by_id = HashMap::with_capacity(entries.len());
        for entry in entries {
            let canonical_name = read_class_name(entry).ok_or(())?;
            let meta = ClassMeta {
                canonical_name: canonical_name.clone(),
                id: entry.class_id,
                parent_id: entry.parent_class_id,
                is_abstract: entry.is_abstract != 0,
            };
            if by_name
                .insert(php_class_key(&canonical_name), meta.clone())
                .is_some()
                || by_id.insert(entry.class_id, meta).is_some()
            {
                return Err(());
            }
        }
        self.by_name = by_name;
        self.by_id = by_id;
        Ok(())
    }

    /// Returns copied metadata for one case-insensitively matched PHP class name.
    pub(crate) fn by_name(&self, name: &[u8]) -> Option<ClassMeta> {
        self.by_name.get(&php_class_key(name)).cloned()
    }

    /// Reports whether one class is identical to or derives from another class.
    pub(crate) fn is_subclass_of(&self, descendant_id: u64, ancestor_id: u64) -> bool {
        let mut cursor = descendant_id;
        for _ in 0..self.by_id.len().saturating_add(1) {
            if cursor == ancestor_id {
                return true;
            }
            let Some(meta) = self.by_id.get(&cursor) else {
                return false;
            };
            if meta.parent_id == DOM_CLASS_NO_PARENT {
                return false;
            }
            cursor = meta.parent_id;
        }
        false
    }
}

/// Copies one valid class-name byte slice from an ABI metadata row.
fn read_class_name(entry: &DomClassMetadataEntry) -> Option<Vec<u8>> {
    if entry.name_ptr.is_null()
        || entry.name_len == 0
        || entry.reserved != 0
    {
        return None;
    }
    let length = usize::try_from(entry.name_len).ok()?;
    if length > isize::MAX as usize {
        return None;
    }
    let name = unsafe { std::slice::from_raw_parts(entry.name_ptr, length) };
    Some(name.to_vec())
}

/// Normalizes a PHP class name for ASCII-case-insensitive metadata lookup.
fn php_class_key(name: &[u8]) -> Vec<u8> {
    name.strip_prefix(b"\\")
        .unwrap_or(name)
        .iter()
        .map(u8::to_ascii_lowercase)
        .collect()
}

/// Copied host callback metadata safe to retain inside a process-local context.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Host {
    pub user_data: usize,
    pub call: Option<HostCall>,
}

/// One PHP-runtime action deferred until the mutable DOM context borrow is released.
pub(crate) enum PendingHostAction {
    /// Drops one retained callable descriptor and may run PHP destructors.
    ReleaseCallable { host: Host, descriptor: u64 },
    /// Drops one leased PHP callback result after the mutable context borrow ends.
    ReleaseResult { host: Host, result_id: u64 },
}

impl PendingHostAction {
    /// Executes one deferred host action through the contained no-unwind callback boundary.
    pub(crate) fn execute(self) -> Result<(), crate::host::HostCallError> {
        match self {
            Self::ReleaseCallable { host, descriptor } => {
                crate::host::release_callable(host, descriptor)
            }
            Self::ReleaseResult { host, result_id } => {
                crate::host::release_result(host, result_id)
            }
        }
    }
}

/// One independently owned result backing the pointer fields returned to generated code.
pub(crate) struct ResultFrame {
    pub status: u32,
    pub php_error_kind: u32,
    pub dom_exception_code: i32,
    pub payload0: u64,
    pub payload1: u64,
    pub bytes: Box<[u8]>,
    pub values: Box<[Value]>,
    pub diagnostics: Box<[Diagnostic]>,
    pub pending_host_actions: Vec<PendingHostAction>,
}

impl ResultFrame {
    /// Builds the public result record whose pointers borrow this registered frame.
    pub(crate) fn header(&self, result_id: u64, value_tag: u32) -> ResultHeader {
        ResultHeader {
            abi_version: ABI_VERSION,
            struct_size: std::mem::size_of::<ResultHeader>() as u32,
            status: self.status,
            value_tag,
            php_error_kind: self.php_error_kind,
            dom_exception_code: self.dom_exception_code,
            result_id,
            payload0: self.payload0,
            payload1: self.payload1,
            bytes_ptr: pointer_or_null(&self.bytes),
            bytes_len: self.bytes.len() as u64,
            values_ptr: pointer_or_null(&self.values),
            values_len: self.values.len() as u64,
            diagnostics_ptr: pointer_or_null(&self.diagnostics),
            diagnostics_len: self.diagnostics.len() as u64,
        }
    }

    /// Builds the ABI ping frame returned by the non-mutating health-check opcode.
    pub(crate) fn ping() -> Self {
        Self {
            bytes: format!("elephc-dom-abi-{ABI_VERSION}")
                .into_bytes()
                .into_boxed_slice(),
            ..Self::null()
        }
    }

    /// Builds a pointer-free null result.
    pub(crate) fn null() -> Self {
        Self {
            status: STATUS_OK,
            php_error_kind: 0,
            dom_exception_code: 0,
            payload0: 0,
            payload1: 0,
            bytes: Box::default(),
            values: Box::default(),
            diagnostics: Box::default(),
            pending_host_actions: Vec::new(),
        }
    }

    /// Builds a pointer-free result for one non-PHP ABI status.
    pub(crate) fn abi_status(status: u32) -> Self {
        Self {
            status,
            ..Self::null()
        }
    }

    /// Builds a boolean scalar result.
    pub(crate) fn boolean(value: bool) -> Self {
        Self {
            payload0: u64::from(value),
            ..Self::null()
        }
    }

    /// Builds an owned byte-string result preserving embedded NUL bytes.
    pub(crate) fn bytes(bytes: Vec<u8>) -> Self {
        let payload1 = bytes.len() as u64;
        Self {
            payload0: 0,
            payload1,
            bytes: bytes.into_boxed_slice(),
            ..Self::null()
        }
    }

    /// Builds one opaque bridge-handle scalar result.
    pub(crate) fn bridge_handle(handle: u64) -> Self {
        Self {
            payload0: handle,
            ..Self::null()
        }
    }

    /// Builds one opaque host-owned scalar result such as a retained callable descriptor.
    pub(crate) fn host_handle(handle: u64) -> Self {
        Self {
            payload0: handle,
            ..Self::null()
        }
    }

    /// Builds one indexed ABI value range plus nested records retained by the result frame.
    pub(crate) fn array(item_count: usize, values: Vec<Value>, bytes: Vec<u8>) -> Self {
        Self {
            payload0: 0,
            payload1: item_count as u64,
            bytes: bytes.into_boxed_slice(),
            values: values.into_boxed_slice(),
            ..Self::null()
        }
    }

    /// Builds one associative ABI range whose values alternate between keys and payloads.
    pub(crate) fn map(item_count: usize, values: Vec<Value>, bytes: Vec<u8>) -> Self {
        Self {
            payload0: 0,
            payload1: item_count as u64,
            bytes: bytes.into_boxed_slice(),
            values: values.into_boxed_slice(),
            ..Self::null()
        }
    }

    /// Builds one PHP value-object result whose fields occupy the complete value vector.
    pub(crate) fn object(values: Vec<Value>, bytes: Vec<u8>) -> Self {
        Self {
            payload0: 0,
            payload1: values.len() as u64,
            bytes: bytes.into_boxed_slice(),
            values: values.into_boxed_slice(),
            ..Self::null()
        }
    }

    /// Builds a catchable `DOMException` result carrying its exact message and integer code.
    pub(crate) fn dom_exception(code: i32, message: &[u8]) -> Self {
        Self {
            status: STATUS_THROW,
            php_error_kind: PHP_ERROR_KIND_DOM_EXCEPTION,
            dom_exception_code: code,
            payload1: message.len() as u64,
            bytes: message.to_vec().into_boxed_slice(),
            ..Self::null()
        }
    }

    /// Builds a catchable `ValueError` result carrying its exact PHP message.
    pub(crate) fn value_error(message: &[u8]) -> Self {
        Self {
            status: STATUS_THROW,
            php_error_kind: PHP_ERROR_KIND_VALUE_ERROR,
            payload1: message.len() as u64,
            bytes: message.to_vec().into_boxed_slice(),
            ..Self::null()
        }
    }

    /// Builds a catchable base `Error` result carrying its exact PHP message.
    pub(crate) fn error(message: &[u8]) -> Self {
        Self {
            status: STATUS_THROW,
            php_error_kind: PHP_ERROR_KIND_ERROR,
            payload1: message.len() as u64,
            bytes: message.to_vec().into_boxed_slice(),
            ..Self::null()
        }
    }

    /// Builds a catchable base `Exception` result carrying its exact PHP message.
    pub(crate) fn exception(message: &[u8]) -> Self {
        Self {
            status: STATUS_THROW,
            php_error_kind: PHP_ERROR_KIND_EXCEPTION,
            payload1: message.len() as u64,
            bytes: message.to_vec().into_boxed_slice(),
            ..Self::null()
        }
    }

    /// Builds a catchable `TypeError` result carrying its exact PHP message.
    pub(crate) fn type_error(message: &[u8]) -> Self {
        Self {
            status: STATUS_THROW,
            php_error_kind: PHP_ERROR_KIND_TYPE_ERROR,
            payload1: message.len() as u64,
            bytes: message.to_vec().into_boxed_slice(),
            ..Self::null()
        }
    }

    /// Builds a pointer-free signal that asks generated code to rethrow the host's active Throwable.
    pub(crate) fn pending_host_throwable() -> Self {
        Self {
            status: STATUS_THROW,
            php_error_kind: PHP_ERROR_KIND_PENDING_HOST_THROWABLE,
            ..Self::null()
        }
    }
}

/// Mutable state isolated to one PHP execution context.
pub(crate) struct Context {
    pub host: Host,
    pub class_metadata: ClassMetadataTable,
    pub results: HashMap<u64, Box<ResultFrame>>,
    pub release_violations: u64,
    pub native_objects: HandleTable<NativeObject>,
    pub document_handles: HashMap<usize, u64>,
    pub node_handles: HashMap<usize, u64>,
    pub implementation_handles: HashMap<usize, u64>,
    pub token_list_handles: HashMap<usize, u64>,
    pub namespace_node_handles: HashMap<usize, u64>,
    pub detached_roots: HashMap<usize, Rc<DocumentGraph>>,
    pub internal_errors: bool,
    pub entity_loader_disabled: bool,
    pub external_entity_loader: Option<u64>,
    pub stream_context: Option<u64>,
    pub errors: Vec<LibxmlErrorObject>,
    pub last_error: Option<LibxmlErrorObject>,
}

impl Context {
    /// Creates a context and verifies its generation-checked handle arena.
    pub(crate) fn new(host: Host) -> Self {
        let native_objects = HandleTable::new();
        Self {
            host,
            class_metadata: ClassMetadataTable::new(),
            results: HashMap::new(),
            release_violations: 0,
            native_objects,
            document_handles: HashMap::new(),
            node_handles: HashMap::new(),
            implementation_handles: HashMap::new(),
            token_list_handles: HashMap::new(),
            namespace_node_handles: HashMap::new(),
            detached_roots: HashMap::new(),
            internal_errors: false,
            entity_loader_disabled: false,
            external_entity_loader: None,
            stream_context: None,
            errors: Vec::new(),
            last_error: None,
        }
    }

    /// Inserts one fresh SimpleXML handle owned by a newly materialized PHP wrapper.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn insert_simplexml_external(
        &mut self,
        mut object: SimpleXmlObject,
    ) -> u64 {
        object.expose_external();
        self.native_objects
            .insert(HANDLE_SIMPLEXML, NativeObject::SimpleXml(object))
    }

    /// Marks an iterator-owned SimpleXML handle as exposed through one PHP wrapper.
    ///
    /// Re-exposing the same handle is intentionally idempotent: the runtime's weak
    /// cache returns the same PHP object and only that object's finalizer releases
    /// the one external native owner.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn expose_simplexml_handle(&mut self, handle: u64) -> Result<(), ()> {
        self.native_objects
            .get_mut(handle, HANDLE_SIMPLEXML)
            .map_err(|_| ())?
            .simplexml_mut()
            .ok_or(())?
            .expose_external();
        Ok(())
    }

    /// Returns the current iterator-data handle without creating a second identity.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn simplexml_iterator_current(
        &self,
        handle: u64,
    ) -> Result<Option<u64>, ()> {
        Ok(self
            .native_objects
            .get(handle, HANDLE_SIMPLEXML)
            .map_err(|_| ())?
            .simplexml()
            .ok_or(())?
            .iterator()
            .current())
    }

    /// Installs one fresh internally owned iterator-data wrapper and releases the prior one.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn install_fresh_simplexml_iterator_current(
        &mut self,
        parent: u64,
        mut current: SimpleXmlObject,
    ) -> Result<u64, ()> {
        self.native_objects
            .get(parent, HANDLE_SIMPLEXML)
            .map_err(|_| ())?
            .simplexml()
            .ok_or(())?;
        current.retain_internal();
        let current_handle = self
            .native_objects
            .insert(HANDLE_SIMPLEXML, NativeObject::SimpleXml(current));
        let prior = self
            .native_objects
            .get_mut(parent, HANDLE_SIMPLEXML)
            .map_err(|_| ())?
            .simplexml_mut()
            .ok_or(())?
            .replace_iterator_current(Some(current_handle));
        if let Some(prior) = prior {
            self.release_simplexml_internal(prior)?;
        }
        Ok(current_handle)
    }

    /// Clears and releases the current iterator-data handle when iteration advances.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn clear_simplexml_iterator_current(
        &mut self,
        parent: u64,
    ) -> Result<(), ()> {
        let prior = self
            .native_objects
            .get_mut(parent, HANDLE_SIMPLEXML)
            .map_err(|_| ())?
            .simplexml_mut()
            .ok_or(())?
            .replace_iterator_current(None);
        if let Some(prior) = prior {
            self.release_simplexml_internal(prior)?;
        }
        Ok(())
    }

    /// Releases a PHP wrapper's external owner and retires unreachable iterator state.
    pub(crate) fn release_simplexml_external(&mut self, handle: u64) -> Result<(), ()> {
        self.native_objects
            .get_mut(handle, HANDLE_SIMPLEXML)
            .map_err(|_| ())?
            .simplexml_mut()
            .ok_or(())?
            .release_external()?;
        self.retire_simplexml_if_unowned(handle)
    }

    /// Releases one parent iterator's internal owner and retires unreachable state.
    fn release_simplexml_internal(&mut self, handle: u64) -> Result<(), ()> {
        self.native_objects
            .get_mut(handle, HANDLE_SIMPLEXML)
            .map_err(|_| ())?
            .simplexml_mut()
            .ok_or(())?
            .release_internal()?;
        self.retire_simplexml_if_unowned(handle)
    }

    /// Removes one ownerless SimpleXML entry and recursively releases its current view.
    fn retire_simplexml_if_unowned(&mut self, handle: u64) -> Result<(), ()> {
        let unowned = self
            .native_objects
            .get(handle, HANDLE_SIMPLEXML)
            .map_err(|_| ())?
            .simplexml()
            .ok_or(())?
            .is_unowned();
        if !unowned {
            return Ok(());
        }
        let mut object = self
            .native_objects
            .remove(handle, HANDLE_SIMPLEXML)
            .map_err(|_| ())?;
        let current = object
            .simplexml_mut()
            .ok_or(())?
            .replace_iterator_current(None);
        if let Some(current) = current {
            if current == handle {
                return Err(());
            }
            self.release_simplexml_internal(current)?;
        }
        Ok(())
    }

    /// Clears request-scoped result and native-object state.
    pub(crate) fn reset(&mut self) {
        self.release_external_entity_loader();
        self.release_xpath_callbacks();
        self.results.clear();
        self.release_violations = 0;
        self.free_detached_roots();
        self.document_handles.clear();
        self.node_handles.clear();
        self.implementation_handles.clear();
        self.token_list_handles.clear();
        self.namespace_node_handles.clear();
        self.native_objects.clear();
        self.internal_errors = false;
        self.entity_loader_disabled = false;
        self.stream_context = None;
        self.errors.clear();
        self.last_error = None;
        debug_assert_eq!(self.native_objects.len(), 0);
        let _ = self.host.user_data;
        let _ = self.host.call;
    }

    /// Registers one newly allocated detached root for request-lifetime cleanup.
    pub(crate) fn register_detached_root(
        &mut self,
        pointer: usize,
        document: Rc<DocumentGraph>,
    ) {
        assert_ne!(pointer, 0, "detached native node pointer must not be null");
        let previous = self.detached_roots.insert(pointer, document);
        debug_assert!(previous.is_none());
    }

    /// Rebinds one live document handle to a fresh graph without changing PHP identity.
    ///
    /// Node wrappers retaining the old graph keep that graph alive, while future
    /// document lookups canonicalize the replacement pointer to the original handle.
    pub(crate) fn reconstruct_legacy_document(
        &mut self,
        handle: u64,
        pointer: usize,
    ) -> Result<(), ()> {
        let previous_pointer = {
            let document = self
                .native_objects
                .get_mut(handle, HANDLE_DOCUMENT)
                .map_err(|_| ())?
                .document_mut()
                .ok_or(())?;
            let previous_pointer = document.pointer();
            document.reconstruct(pointer, DocumentFamily::Legacy);
            previous_pointer
        };
        if self.document_handles.get(&previous_pointer) == Some(&handle) {
            self.document_handles.remove(&previous_pointer);
        }
        self.document_handles.insert(pointer, handle);
        Ok(())
    }

    /// Rebinds one live node handle to a fresh hidden-owner node without changing PHP identity.
    ///
    /// The prior pointer leaves the canonical wrapper cache, but an attached prior
    /// node stays in its old document and a detached prior tree remains retained in
    /// `detached_roots` until request teardown. This mirrors php-src's independent
    /// native node resources while preventing dangling descendant wrappers.
    pub(crate) fn reconstruct_direct_node(
        &mut self,
        handle: u64,
        pointer: usize,
        document: Rc<DocumentGraph>,
    ) -> Result<(), ()> {
        let previous_pointer = self
            .native_objects
            .get_mut(handle, HANDLE_NODE)
            .map_err(|_| ())?
            .node_mut()
            .ok_or(())?
            .reconstruct_without_owner_document(pointer, Rc::clone(&document));
        if self.node_handles.get(&previous_pointer) == Some(&handle) {
            self.node_handles.remove(&previous_pointer);
        }
        self.register_detached_root(pointer, document);
        self.node_handles.insert(pointer, handle);
        Ok(())
    }

    /// Transfers one detached root into a parent tree after successful append.
    pub(crate) fn attach_detached_root(&mut self, pointer: usize) {
        self.detached_roots.remove(&pointer);
    }

    /// Rehomes one still-detached native root under an adopted document graph.
    pub(crate) fn rehome_detached_root(
        &mut self,
        pointer: usize,
        document: Rc<DocumentGraph>,
    ) {
        if let Some(retained) = self.detached_roots.get_mut(&pointer) {
            *retained = document;
        }
    }

    /// Invalidates every wrapper and derived view backed by XInclude-owned nodes.
    pub(crate) fn invalidate_node_pointers(&mut self, pointers: &[usize]) {
        let pointers = pointers.iter().copied().collect::<HashSet<_>>();
        for pointer in &pointers {
            if let Some(handle) = self.node_handles.remove(pointer) {
                if let Ok(object) = self
                    .native_objects
                    .get_mut(handle, crate::objects::HANDLE_NODE)
                {
                    if object.node().is_some() {
                        *object = NativeObject::InvalidNode;
                    }
                }
            }
            if let Some(handle) = self.token_list_handles.remove(pointer) {
                if let Ok(object) = self
                    .native_objects
                    .get_mut(handle, crate::objects::HANDLE_TOKEN_LIST)
                {
                    if object.token_list().is_some() {
                        *object = NativeObject::InvalidTokenList;
                    }
                }
            }
        }
        let mut invalidated_namespace_fakes = HashSet::new();
        for (fake, handle) in self.namespace_node_handles.clone() {
            let Some(object) = self.native_objects.get_mut(handle, crate::objects::HANDLE_NAMESPACE_NODE).ok() else {
                continue;
            };
            if let Some(namespace_node) = object.namespace_node() {
                if pointers.contains(&namespace_node.parent()) {
                    invalidated_namespace_fakes.insert(fake);
                }
            }
        }
        for fake in &invalidated_namespace_fakes {
            if let Some(handle) = self.namespace_node_handles.remove(fake) {
                if let Ok(object) = self
                    .native_objects
                    .get_mut(handle, crate::objects::HANDLE_NAMESPACE_NODE)
                {
                    if object.namespace_node().is_some() {
                        *object = NativeObject::InvalidNamespaceNode;
                    }
                }
            }
        }
        self.native_objects.for_each_mut(|object| {
            if let Some(collection) = object.collection_mut() {
                collection.invalidate_pointers(&pointers);
            }
        });
    }

    /// Frees every still-detached native tree while its document graph is retained.
    fn free_detached_roots(&mut self) {
        for (pointer, _document) in self.detached_roots.drain() {
            unsafe {
                crate::native::node_free(pointer);
            }
        }
    }

    /// Replaces the retained PHP external-entity loader with ownership balanced through the host.
    pub(crate) fn set_external_entity_loader(
        &mut self,
        descriptor: Option<u64>,
    ) -> Result<Option<PendingHostAction>, crate::host::HostCallError> {
        if let Some(descriptor) = descriptor {
            crate::host::retain_callable(self.host, descriptor)?;
        }
        let previous = std::mem::replace(&mut self.external_entity_loader, descriptor);
        Ok(previous.map(|descriptor| PendingHostAction::ReleaseCallable {
            host: self.host,
            descriptor,
        }))
    }

    /// Returns a newly retained callable descriptor for transfer back to PHP.
    pub(crate) fn retained_external_entity_loader(
        &self,
    ) -> Result<Option<u64>, crate::host::HostCallError> {
        let Some(descriptor) = self.external_entity_loader else {
            return Ok(None);
        };
        crate::host::retain_callable(self.host, descriptor)?;
        Ok(Some(descriptor))
    }

    /// Releases and clears the retained external-entity loader, ignoring shutdown failures.
    fn release_external_entity_loader(&mut self) {
        if let Some(descriptor) = self.external_entity_loader.take() {
            let _ = crate::host::release_callable(self.host, descriptor);
        }
    }

    /// Releases every callable descriptor retained by live XPath wrappers.
    fn release_xpath_callbacks(&mut self) {
        let mut descriptors = Vec::new();
        self.native_objects.for_each_mut(|object| {
            if let Some(xpath) = object.xpath_mut() {
                descriptors.extend(xpath.take_callback_descriptors());
            }
        });
        for descriptor in descriptors {
            let _ = crate::host::release_callable(self.host, descriptor);
        }
    }
}

impl Drop for Context {
    /// Balances host-owned callable state if a context is destroyed without an explicit reset.
    fn drop(&mut self) {
        self.release_external_entity_loader();
        self.release_xpath_callbacks();
        self.free_detached_roots();
    }
}

thread_local! {
    /// Contexts owned by the current PHP execution thread.
    static CONTEXTS: RefCell<HashMap<u64, Rc<RefCell<Context>>>> =
        RefCell::new(HashMap::new());
}

/// Registers one current-thread context and returns its fresh opaque ID.
pub(crate) fn register_context(context: Context) -> u64 {
    let context_id = next_id();
    CONTEXTS.with(|contexts| {
        contexts
            .borrow_mut()
            .insert(context_id, Rc::new(RefCell::new(context)));
    });
    context_id
}

/// Returns a retained reference-counted cell for one current-thread context.
pub(crate) fn context(context_id: u64) -> Option<Rc<RefCell<Context>>> {
    CONTEXTS.with(|contexts| contexts.borrow().get(&context_id).cloned())
}

/// Removes one context from the current thread and drops it when no call retains it.
pub(crate) fn remove_context(context_id: u64) {
    CONTEXTS.with(|contexts| {
        contexts.borrow_mut().remove(&context_id);
    });
}

/// Allocates a fresh process-local opaque ID.
pub(crate) fn next_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Registers a result frame and returns its stable public header.
pub(crate) fn register_result(
    context: &mut Context,
    frame: ResultFrame,
    value_tag: u32,
) -> ResultHeader {
    let result_id = next_id();
    let frame = Box::new(frame);
    let header = frame.header(result_id, value_tag);
    context.results.insert(result_id, frame);
    header
}

/// Returns the bridge value tag used by the ABI ping result.
pub(crate) fn ping_value_tag() -> u32 {
    VALUE_BYTES
}

/// Returns a slice pointer or null for an empty result buffer.
fn pointer_or_null<T>(values: &[T]) -> *const T {
    if values.is_empty() {
        std::ptr::null()
    } else {
        values.as_ptr()
    }
}
