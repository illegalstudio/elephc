//! Purpose:
//! Owns native object records stored behind generation-checked bridge handles.
//! Releases authoritative libxml2 documents when their final native record is dropped.
//!
//! Called from:
//! - `crate::dispatch` for construction, parsing, serialization, and tree operations.
//! - `crate::context` when request reset or context destruction drops handle state.
//!
//! Key details:
//! - Raw native pointers remain private numeric storage and never cross the public ABI.
//! - XML/HTML representation and the document-wide legacy/modern claim are independent.
//! - SimpleXML views are fresh handles sharing one authoritative `Rc<DocumentGraph>`.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::rc::Rc;

/// Handle kind for authoritative libxml2 documents.
pub(crate) const HANDLE_DOCUMENT: u8 = 1;
/// Handle kind for libxml2 nodes whose lifetime is anchored by a document graph.
pub(crate) const HANDLE_NODE: u8 = 2;
/// Handle kind for live DOM node-list and named-node-map query descriptors.
pub(crate) const HANDLE_COLLECTION: u8 = 3;
/// Handle kind for stateless legacy or modern DOM implementation wrappers.
pub(crate) const HANDLE_IMPLEMENTATION: u8 = 4;
/// Handle kind for modern element-backed `Dom\TokenList` wrappers.
pub(crate) const HANDLE_TOKEN_LIST: u8 = 5;
/// Handle kind for legacy and modern XPath evaluation contexts.
pub(crate) const HANDLE_XPATH: u8 = 6;
/// Handle kind for standalone legacy XPath namespace-declaration wrappers.
pub(crate) const HANDLE_NAMESPACE_NODE: u8 = 7;
/// Handle kind for fresh SimpleXML views over nodes in one shared document graph.
pub(crate) const HANDLE_SIMPLEXML: u8 = 8;
/// Stable ABI object-type flag for `LibXMLError` values nested in result arrays.
pub(crate) const VALUE_OBJECT_LIBXML_ERROR: u32 = 1;
/// Stable ABI object-type flag for `Dom\NamespaceInfo` values nested in result arrays.
pub(crate) const VALUE_OBJECT_NAMESPACE_INFO: u32 = 2;

/// One copied libxml structured error exposed through a PHP `LibXMLError` wrapper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LibxmlErrorObject {
    pub(crate) level: i64,
    pub(crate) domain: i32,
    pub(crate) code: i64,
    pub(crate) line: i64,
    pub(crate) column: i64,
    pub(crate) message: Vec<u8>,
    pub(crate) file: Vec<u8>,
}

/// PHP wrapper family controlling legacy and modern behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DocumentFamily {
    Legacy,
    ModernXml,
    ModernHtml,
}

/// Document-wide DOM API claim shared by DOM and SimpleXML wrappers.
///
/// php-src leaves this claim unset for a document parsed by SimpleXML. The first
/// DOM import pins the whole document to one API generation; the concrete XML or
/// HTML representation remains a separate property of the graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DomApiFamily {
    Legacy,
    Modern,
}

/// Concrete native document representation independent from its DOM API claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DocumentKind {
    Xml,
    Html,
}

/// Failure while pinning a previously unclaimed SimpleXML document to a DOM API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum DomClaimError {
    /// A wrapper from the other DOM API generation already claimed the document.
    ConflictingFamily,
    /// libxml2 could not install PHP's modern XML namespace representation.
    ModernConversionFailed,
}

/// One mutable legacy `DOMDocument` behavior flag retained across loads.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LegacyDocumentFlag {
    PreserveWhitespace = 1 << 0,
    Recover = 1 << 1,
    ResolveExternals = 1 << 2,
    StrictErrorChecking = 1 << 3,
    SubstituteEntities = 1 << 4,
    ValidateOnParse = 1 << 5,
}

const DEFAULT_LEGACY_DOCUMENT_FLAGS: u8 =
    LegacyDocumentFlag::PreserveWhitespace as u8
        | LegacyDocumentFlag::StrictErrorChecking as u8;

/// One live query or static snapshot retained by a DOM collection wrapper.
pub(crate) enum CollectionKind {
    ChildNodes,
    ChildElements,
    ElementsByTagName {
        name: Vec<u8>,
    },
    ElementsByTagNameNs {
        namespace_uri: Option<Vec<u8>>,
        local_name: Vec<u8>,
    },
    ElementsByClassName {
        names: Vec<Vec<u8>>,
        full_quirks: bool,
    },
    Snapshot {
        pointers: Vec<Option<usize>>,
        /// Parallel to `pointers`: `Some(allocation)` marks a namespace-declaration
        /// slot whose fake node is owned by a shared `NamespaceNodeAllocation`. This
        /// field intentionally precedes document retainers so Rust drops the native
        /// fake nodes before their parent documents.
        namespace_allocations: Vec<Option<Rc<NamespaceNodeAllocation>>>,
        /// Parallel to `pointers`: a foreign callback-returned member retains its
        /// own authoritative document graph instead of being rehomed into the
        /// XPath context document. Ordinary snapshot members use `None` and the
        /// collection's primary document.
        member_documents: Vec<Option<Rc<DocumentGraph>>>,
    },
    Attributes,
    /// Live libxml2 DTD entity hash table rooted at one doctype node.
    DtdEntities,
    /// Live libxml2 DTD notation hash table rooted at one doctype node.
    DtdNotations,
}

/// One document-retaining live or snapshot DOM collection descriptor.
pub(crate) struct CollectionObject {
    root: usize,
    /// Dropped before `document` so namespace snapshot allocations never outlive
    /// the native document that owns their copied libxml strings and parent nodes.
    kind: CollectionKind,
    document: Rc<DocumentGraph>,
    root_invalidated: bool,
}

/// One document-retaining modern class-token list associated with an element.
pub(crate) struct TokenListObject {
    element: usize,
    document: Rc<DocumentGraph>,
}

/// One retained custom XPath function registered by namespace URI and local name.
#[derive(Clone)]
pub(crate) struct XPathCallback {
    namespace_uri: Vec<u8>,
    name: Vec<u8>,
    descriptor: u64,
}

/// Registration mode governing PHP's reserved `php:function*` XPath callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum XPathPhpCallbackMode {
    /// `registerPhpFunctions()` was never called.
    None,
    /// Every runtime-resolvable PHP callable name is allowed.
    All,
    /// Only explicitly registered aliases are allowed.
    Set,
}

/// One retained PHP callable registered under its exact XPath handler alias.
#[derive(Clone)]
struct XPathPhpCallback {
    alias: Vec<u8>,
    descriptor: u64,
}

/// One document-retaining XPath context with persistent namespace and callback state.
pub(crate) struct XPathObject {
    document: Rc<DocumentGraph>,
    register_node_namespaces: bool,
    namespaces: Vec<(Vec<u8>, Vec<u8>)>,
    callbacks: Vec<XPathCallback>,
    php_callback_mode: XPathPhpCallbackMode,
    php_callbacks: Vec<XPathPhpCallback>,
}

/// One stateless DOM implementation wrapper, optionally cached by a document.
pub(crate) struct ImplementationObject {
    family: DocumentFamily,
    associated_document: Option<usize>,
}

impl ImplementationObject {
    /// Creates one implementation wrapper for a selected API family.
    pub(crate) fn new(
        family: DocumentFamily,
        associated_document: Option<usize>,
    ) -> Self {
        Self {
            family,
            associated_document,
        }
    }

    /// Returns the legacy or modern API family exposed by this wrapper.
    pub(crate) fn family(&self) -> DocumentFamily {
        self.family
    }

    /// Returns the document pointer whose modern implementation identity is cached.
    pub(crate) fn associated_document(&self) -> Option<usize> {
        self.associated_document
    }
}

impl CollectionObject {
    /// Creates one collection rooted at a non-null native document or node pointer.
    pub(crate) fn new(
        root: usize,
        document: Rc<DocumentGraph>,
        kind: CollectionKind,
    ) -> Self {
        assert_ne!(root, 0, "native collection root must not be null");
        Self {
            root,
            document,
            kind,
            root_invalidated: false,
        }
    }

    /// Returns the private native root pointer for immediate query evaluation.
    pub(crate) fn root(&self) -> usize {
        self.root
    }

    /// Retains the authoritative document graph for materialized collection members.
    pub(crate) fn document(&self) -> Rc<DocumentGraph> {
        Rc::clone(&self.document)
    }

    /// Returns the legacy or modern family governing collection query semantics.
    pub(crate) fn family(&self) -> DocumentFamily {
        self.document.family()
    }

    /// Returns the immutable live-query or snapshot description.
    pub(crate) fn kind(&self) -> &CollectionKind {
        &self.kind
    }

    /// Reports whether XInclude destroyed this live descriptor's native root.
    pub(crate) fn root_is_invalidated(&self) -> bool {
        self.root_invalidated
    }

    /// Clears every native collection pointer destroyed during XInclude.
    pub(crate) fn invalidate_pointers(&mut self, pointers: &HashSet<usize>) {
        if pointers.contains(&self.root) {
            self.root_invalidated = true;
        }
        if let CollectionKind::Snapshot {
            pointers: members,
            member_documents,
            namespace_allocations,
        } = &mut self.kind
        {
            for ((member, document), slot) in members
                .iter_mut()
                .zip(member_documents.iter_mut())
                .zip(namespace_allocations.iter_mut())
            {
                if member.is_some_and(|pointer| pointers.contains(&pointer)) {
                    *member = None;
                    *document = None;
                }
                if let Some(allocation) = slot {
                    if pointers.contains(&allocation.parent()) {
                        *slot = None;
                        *member = None;
                        *document = None;
                    }
                }
            }
        }
    }

    /// Retains the authoritative document graph for one snapshot member.
    ///
    /// XPath callbacks may return a node from a document other than the context
    /// document. Such slots carry a per-member override so later `item()` calls
    /// preserve PHP wrapper identity and keep the foreign graph alive.
    pub(crate) fn member_document(&self, index: usize) -> Rc<DocumentGraph> {
        if let CollectionKind::Snapshot {
            member_documents,
            ..
        } = &self.kind
        {
            if let Some(Some(document)) = member_documents.get(index) {
                return Rc::clone(document);
            }
        }
        self.document()
    }

    /// Returns the shared namespace-declaration allocation for one snapshot slot.
    ///
    /// Clones the slot's `Rc<NamespaceNodeAllocation>` so the caller can hand it to
    /// a materialized wrapper without transferring ownership: the snapshot keeps its
    /// reference and can recreate the wrapper on a later `item()` call. Returns
    /// `None` for ordinary node slots, non-snapshot collections, or cleared slots.
    pub(crate) fn namespace_allocation(
        &self,
        index: usize,
    ) -> Option<Rc<NamespaceNodeAllocation>> {
        if let CollectionKind::Snapshot {
            namespace_allocations,
            ..
        } = &self.kind
        {
            namespace_allocations
                .get(index)
                .and_then(|slot| slot.as_ref().map(Rc::clone))
        } else {
            None
        }
    }
}

impl TokenListObject {
    /// Creates one token-list record associated with a non-null modern element.
    pub(crate) fn new(element: usize, document: Rc<DocumentGraph>) -> Self {
        assert_ne!(element, 0, "native token-list element must not be null");
        Self { element, document }
    }

    /// Returns the private native element pointer used for class-attribute access.
    pub(crate) fn element(&self) -> usize {
        self.element
    }

    /// Rehomes this token list after its associated element is adopted.
    pub(crate) fn replace_document(&mut self, document: Rc<DocumentGraph>) {
        self.document = document;
    }
}

impl XPathObject {
    /// Creates an XPath context retaining one authoritative document graph.
    pub(crate) fn new(
        document: Rc<DocumentGraph>,
        register_node_namespaces: bool,
    ) -> Self {
        Self {
            document,
            register_node_namespaces,
            namespaces: Vec::new(),
            callbacks: Vec::new(),
            php_callback_mode: XPathPhpCallbackMode::None,
            php_callbacks: Vec::new(),
        }
    }

    /// Retains the authoritative document graph used by every evaluation.
    pub(crate) fn document(&self) -> Rc<DocumentGraph> {
        Rc::clone(&self.document)
    }

    /// Returns whether context-node namespaces are registered by default.
    pub(crate) fn register_node_namespaces(&self) -> bool {
        self.register_node_namespaces
    }

    /// Updates the default context-node namespace registration behavior.
    pub(crate) fn set_register_node_namespaces(&mut self, value: bool) {
        self.register_node_namespaces = value;
    }

    /// Replaces or appends one persistent prefix-to-namespace binding.
    pub(crate) fn register_namespace(
        &mut self,
        prefix: Vec<u8>,
        namespace_uri: Vec<u8>,
    ) {
        if let Some((_, current_uri)) = self
            .namespaces
            .iter_mut()
            .find(|(current_prefix, _)| *current_prefix == prefix)
        {
            *current_uri = namespace_uri;
        } else {
            self.namespaces.push((prefix, namespace_uri));
        }
    }

    /// Returns persistent namespace bindings in registration order.
    pub(crate) fn namespaces(&self) -> &[(Vec<u8>, Vec<u8>)] {
        &self.namespaces
    }

    /// Replaces or appends one custom namespace function and returns the previous descriptor.
    pub(crate) fn register_callback(
        &mut self,
        namespace_uri: Vec<u8>,
        name: Vec<u8>,
        descriptor: u64,
    ) -> Option<u64> {
        if let Some(callback) = self.callbacks.iter_mut().find(|callback| {
            callback.namespace_uri == namespace_uri && callback.name == name
        }) {
            return Some(std::mem::replace(
                &mut callback.descriptor,
                descriptor,
            ));
        }
        self.callbacks.push(XPathCallback {
            namespace_uri,
            name,
            descriptor,
        });
        None
    }

    /// Returns custom namespace functions in registration order for native evaluation.
    pub(crate) fn callbacks(&self) -> &[XPathCallback] {
        &self.callbacks
    }

    /// Resolves one custom namespace function to its retained callable descriptor.
    pub(crate) fn callback_descriptor(
        &self,
        namespace_uri: &[u8],
        name: &[u8],
    ) -> Option<u64> {
        self.callbacks
            .iter()
            .find(|callback| {
                callback.namespace_uri == namespace_uri
                    && callback.name == name
            })
            .map(|callback| callback.descriptor)
    }

    /// Switches reserved PHP XPath callbacks to unrestricted dynamic resolution.
    pub(crate) fn allow_all_php_callbacks(&mut self) {
        self.php_callback_mode = XPathPhpCallbackMode::All;
    }

    /// Switches reserved PHP XPath callbacks to the explicit alias set.
    pub(crate) fn restrict_php_callbacks(&mut self) {
        self.php_callback_mode = XPathPhpCallbackMode::Set;
    }

    /// Returns the current reserved PHP XPath callback registration mode.
    pub(crate) fn php_callback_mode(&self) -> XPathPhpCallbackMode {
        self.php_callback_mode
    }

    /// Resolves one exact registered PHP XPath handler alias.
    pub(crate) fn php_callback_descriptor(&self, alias: &[u8]) -> Option<u64> {
        self.php_callbacks
            .iter()
            .find(|callback| callback.alias == alias)
            .map(|callback| callback.descriptor)
    }

    /// Replaces or appends one exact PHP XPath handler alias.
    pub(crate) fn register_php_callback(
        &mut self,
        alias: Vec<u8>,
        descriptor: u64,
    ) -> Option<u64> {
        if let Some(callback) = self
            .php_callbacks
            .iter_mut()
            .find(|callback| callback.alias == alias)
        {
            return Some(std::mem::replace(
                &mut callback.descriptor,
                descriptor,
            ));
        }
        self.php_callbacks.push(XPathPhpCallback { alias, descriptor });
        None
    }

    /// Copies every retained custom and reserved PHP callable descriptor.
    pub(crate) fn callback_descriptors(&self) -> Vec<u64> {
        self.callbacks
            .iter()
            .map(|callback| callback.descriptor)
            .chain(
                self.php_callbacks
                    .iter()
                    .map(|callback| callback.descriptor),
            )
            .collect()
    }

    /// Removes and returns every retained custom and reserved PHP callable descriptor.
    pub(crate) fn take_callback_descriptors(&mut self) -> Vec<u64> {
        let mut descriptors = std::mem::take(&mut self.callbacks)
            .into_iter()
            .map(|callback| callback.descriptor)
            .collect::<Vec<_>>();
        descriptors.extend(
            std::mem::take(&mut self.php_callbacks)
                .into_iter()
                .map(|callback| callback.descriptor),
        );
        descriptors
    }

    /// Returns the namespace URI registered for this custom function.
    pub(crate) fn callback_namespace(callback: &XPathCallback) -> &[u8] {
        &callback.namespace_uri
    }

    /// Returns the local name registered for this custom function.
    pub(crate) fn callback_name(callback: &XPathCallback) -> &[u8] {
        &callback.name
    }

    /// Clones namespace and callback metadata after the caller retained every descriptor.
    pub(crate) fn clone_with_retained_callbacks(&self) -> Self {
        Self {
            document: Rc::clone(&self.document),
            register_node_namespaces: self.register_node_namespaces,
            namespaces: self.namespaces.clone(),
            callbacks: self.callbacks.clone(),
            php_callback_mode: self.php_callback_mode,
            php_callbacks: self.php_callbacks.clone(),
        }
    }
}

/// Native iteration mode retained by one SimpleXML view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum SimpleXmlIteratorType {
    /// Direct view over the wrapped node with no collection traversal.
    None,
    /// Named-child view used by property and dimension access.
    Element,
    /// All-child-elements view produced by `children()`.
    Child,
    /// Attribute-list view produced by `attributes()`.
    AttrList,
}

/// Namespace/name filtering and live iterator identity retained by one view.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct SimpleXmlIteratorState {
    kind: SimpleXmlIteratorType,
    name: Option<Vec<u8>>,
    namespace_or_prefix: Option<Vec<u8>>,
    is_prefix: bool,
    current: Option<u64>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl SimpleXmlIteratorState {
    /// Creates one iterator description with no materialized current wrapper.
    pub(crate) fn new(
        kind: SimpleXmlIteratorType,
        name: Option<Vec<u8>>,
        namespace_or_prefix: Option<Vec<u8>>,
        is_prefix: bool,
    ) -> Self {
        Self {
            kind,
            name,
            namespace_or_prefix,
            is_prefix,
            current: None,
        }
    }

    /// Creates an ordinary direct-node view while preserving its namespace filter.
    pub(crate) fn direct(
        namespace_or_prefix: Option<Vec<u8>>,
        is_prefix: bool,
    ) -> Self {
        Self::new(
            SimpleXmlIteratorType::None,
            None,
            namespace_or_prefix,
            is_prefix,
        )
    }

    /// Returns the native iteration mode.
    pub(crate) fn kind(&self) -> SimpleXmlIteratorType {
        self.kind
    }

    /// Returns the optional named-child or attribute filter.
    pub(crate) fn name(&self) -> Option<&[u8]> {
        self.name.as_deref()
    }

    /// Returns the optional namespace URI or prefix filter.
    pub(crate) fn namespace_or_prefix(&self) -> Option<&[u8]> {
        self.namespace_or_prefix.as_deref()
    }

    /// Reports whether the namespace selector is a prefix rather than a URI.
    pub(crate) fn is_prefix(&self) -> bool {
        self.is_prefix
    }

    /// Returns the internally retained current-wrapper handle.
    pub(crate) fn current(&self) -> Option<u64> {
        self.current
    }

    /// Replaces the current-wrapper handle and returns the prior internal owner.
    fn replace_current(&mut self, current: Option<u64>) -> Option<u64> {
        std::mem::replace(&mut self.current, current)
    }
}

/// One fresh SimpleXML wrapper view retaining its authoritative document graph.
///
/// Unlike DOM node wrappers, SimpleXML views are never pointer-canonicalized. Two
/// property reads may therefore carry different handles while sharing this same
/// graph. The iterator's current handle is the deliberate exception: it is held by
/// an internal owner and re-exposed so `current()` and `getChildren()` preserve PHP
/// object identity until iteration advances.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct SimpleXmlObject {
    node: Option<NonZeroUsize>,
    document: Rc<DocumentGraph>,
    wrapper_kind: u64,
    iterator: SimpleXmlIteratorState,
    xpath_namespaces: Vec<(Vec<u8>, Vec<u8>)>,
    external_owner: bool,
    internal_owners: usize,
}

#[cfg_attr(not(test), allow(dead_code))]
impl SimpleXmlObject {
    /// Creates one unowned SimpleXML view for insertion through `Context` ownership helpers.
    pub(crate) fn new(
        pointer: usize,
        document: Rc<DocumentGraph>,
        wrapper_kind: u64,
        iterator: SimpleXmlIteratorState,
    ) -> Self {
        let node = NonZeroUsize::new(pointer)
            .expect("native SimpleXML node pointer must not be null");
        Self {
            node: Some(node),
            document,
            wrapper_kind,
            iterator,
            xpath_namespaces: Vec::new(),
            external_owner: false,
            internal_owners: 0,
        }
    }

    /// Creates one parsed SimpleXML document whose recovery produced no root node.
    pub(crate) fn new_without_node(
        document: Rc<DocumentGraph>,
        wrapper_kind: u64,
        iterator: SimpleXmlIteratorState,
    ) -> Self {
        Self {
            node: None,
            document,
            wrapper_kind,
            iterator,
            xpath_namespaces: Vec::new(),
            external_owner: false,
            internal_owners: 0,
        }
    }

    /// Returns the optional private libxml2 node pointer for guarded native calls.
    pub(crate) fn node_pointer(&self) -> Option<usize> {
        self.node.map(NonZeroUsize::get)
    }

    /// Returns the private libxml2 node pointer, or zero for a recovered rootless document.
    ///
    /// New rootless-aware code must prefer `node_pointer()` and branch before entering C.
    pub(crate) fn pointer(&self) -> usize {
        self.node_pointer().unwrap_or(0)
    }

    /// Retains the shared document graph keeping this node and its metadata alive.
    pub(crate) fn document(&self) -> Rc<DocumentGraph> {
        Rc::clone(&self.document)
    }

    /// Returns the exact SimpleXMLElement subclass or iterator discriminator.
    pub(crate) fn wrapper_kind(&self) -> u64 {
        self.wrapper_kind
    }

    /// Returns the immutable view and iteration description.
    pub(crate) fn iterator(&self) -> &SimpleXmlIteratorState {
        &self.iterator
    }

    /// Replaces one receiver after a successful constructor re-entry parse.
    ///
    /// The caller must release any internally owned iterator-current handle first.
    /// The concrete PHP wrapper discriminator and its external/internal owners remain
    /// unchanged, while parser-derived node, graph, filters, and XPath state are reset.
    pub(crate) fn replace_parsed_view(
        &mut self,
        pointer: usize,
        document: Rc<DocumentGraph>,
        iterator: SimpleXmlIteratorState,
    ) {
        let node = NonZeroUsize::new(pointer)
            .expect("native SimpleXML node pointer must not be null");
        debug_assert!(iterator.current().is_none());
        self.node = Some(node);
        self.document = document;
        self.iterator = iterator;
        self.xpath_namespaces.clear();
    }

    /// Replaces one receiver with a recovered document that owns no root node.
    pub(crate) fn replace_parsed_document_without_node(
        &mut self,
        document: Rc<DocumentGraph>,
        iterator: SimpleXmlIteratorState,
    ) {
        debug_assert!(iterator.current().is_none());
        self.node = None;
        self.document = document;
        self.iterator = iterator;
        self.xpath_namespaces.clear();
    }

    /// Registers or replaces one wrapper-local XPath namespace binding.
    pub(crate) fn register_xpath_namespace(
        &mut self,
        prefix: Vec<u8>,
        namespace_uri: Vec<u8>,
    ) {
        if let Some((_, current_uri)) = self
            .xpath_namespaces
            .iter_mut()
            .find(|(current_prefix, _)| *current_prefix == prefix)
        {
            *current_uri = namespace_uri;
        } else {
            self.xpath_namespaces.push((prefix, namespace_uri));
        }
    }

    /// Returns this wrapper's XPath bindings in registration order.
    pub(crate) fn xpath_namespaces(&self) -> &[(Vec<u8>, Vec<u8>)] {
        &self.xpath_namespaces
    }

    /// Records that one live PHP wrapper owns this fresh native handle.
    pub(crate) fn expose_external(&mut self) {
        self.external_owner = true;
    }

    /// Releases the one possible live PHP-wrapper owner.
    pub(crate) fn release_external(&mut self) -> Result<(), ()> {
        if !self.external_owner {
            return Err(());
        }
        self.external_owner = false;
        Ok(())
    }

    /// Adds one parent-iterator owner for this handle.
    pub(crate) fn retain_internal(&mut self) {
        self.internal_owners = self
            .internal_owners
            .checked_add(1)
            .expect("SimpleXML internal-owner count overflow");
    }

    /// Releases one parent-iterator owner for this handle.
    pub(crate) fn release_internal(&mut self) -> Result<(), ()> {
        if self.internal_owners == 0 {
            return Err(());
        }
        self.internal_owners -= 1;
        Ok(())
    }

    /// Reports whether no PHP wrapper or iterator can still reach this handle.
    pub(crate) fn is_unowned(&self) -> bool {
        !self.external_owner && self.internal_owners == 0
    }

    /// Replaces the internally owned iterator-data handle.
    pub(crate) fn replace_iterator_current(
        &mut self,
        current: Option<u64>,
    ) -> Option<u64> {
        self.iterator.replace_current(current)
    }
}

/// One shared authoritative libxml2 graph retained by document and node wrappers.
pub(crate) struct DocumentGraph {
    pointer: usize,
    kind: DocumentKind,
    dom_api: Cell<Option<DomApiFamily>>,
    format_output: Cell<bool>,
    legacy_flags: Cell<u8>,
    /// Per-document `registerNodeClass()` map from native base to extended class.
    classmap: RefCell<HashMap<Vec<u8>, Vec<u8>>>,
}

impl DocumentGraph {
    /// Takes ownership of one DOM-created document with php-src's initial API claim.
    fn new(pointer: usize, family: DocumentFamily) -> Self {
        assert_ne!(pointer, 0, "native document pointer must not be null");
        let (kind, dom_api) = match family {
            DocumentFamily::Legacy => (DocumentKind::Xml, None),
            DocumentFamily::ModernXml => {
                (DocumentKind::Xml, Some(DomApiFamily::Modern))
            }
            DocumentFamily::ModernHtml => {
                (DocumentKind::Html, Some(DomApiFamily::Modern))
            }
        };
        Self {
            pointer,
            kind,
            dom_api: Cell::new(dom_api),
            format_output: Cell::new(false),
            legacy_flags: Cell::new(DEFAULT_LEGACY_DOCUMENT_FLAGS),
            classmap: RefCell::new(HashMap::new()),
        }
    }

    /// Takes ownership of one XML document parsed by SimpleXML before any DOM import.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new_unclaimed_xml(pointer: usize) -> Rc<Self> {
        assert_ne!(pointer, 0, "native document pointer must not be null");
        Rc::new(Self {
            pointer,
            kind: DocumentKind::Xml,
            dom_api: Cell::new(None),
            format_output: Cell::new(false),
            legacy_flags: Cell::new(DEFAULT_LEGACY_DOCUMENT_FLAGS),
            classmap: RefCell::new(HashMap::new()),
        })
    }

    /// Replaces a loaded native graph while retaining PHP wrapper configuration.
    pub(crate) fn replacement(&self, pointer: usize) -> Self {
        assert_ne!(pointer, 0, "native document pointer must not be null");
        Self {
            pointer,
            kind: self.kind,
            dom_api: Cell::new(self.dom_api.get()),
            format_output: Cell::new(self.format_output.get()),
            legacy_flags: Cell::new(self.legacy_flags.get()),
            classmap: RefCell::new(self.classmap.borrow().clone()),
        }
    }

    /// Returns the private libxml2 pointer for an immediate native adapter call.
    pub(crate) fn pointer(&self) -> usize {
        self.pointer
    }

    /// Returns the DOM API generation currently claimed by this document.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn dom_api(&self) -> Option<DomApiFamily> {
        self.dom_api.get()
    }

    /// Claims this entire document for one DOM API generation.
    ///
    /// The first modern claim converts a SimpleXML-created XML graph before the
    /// claim becomes visible. A failed conversion leaves the graph unclaimed so a
    /// later legacy import remains valid. Repeated compatible claims are inert.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn claim_dom_api(
        &self,
        requested: DomApiFamily,
    ) -> Result<DocumentFamily, DomClaimError> {
        if let Some(current) = self.dom_api.get() {
            return if current == requested {
                Ok(self.family_for(current))
            } else {
                Err(DomClaimError::ConflictingFamily)
            };
        }
        if requested == DomApiFamily::Modern
            && self.kind == DocumentKind::Xml
            && !crate::native::document_convert_modern_xml(self.pointer)
        {
            return Err(DomClaimError::ModernConversionFailed);
        }
        self.dom_api.set(Some(requested));
        Ok(self.family_for(requested))
    }

    /// Returns the concrete DOM wrapper family used by ordinary graph operations.
    ///
    /// php-src leaves a legacy `DOMDocument`'s document-wide interop claim unset
    /// until its first SimpleXML-to-DOM import. Existing legacy operations still
    /// materialize legacy wrappers during that interval.
    pub(crate) fn family(&self) -> DocumentFamily {
        self.dom_api
            .get()
            .map_or(DocumentFamily::Legacy, |dom_api| {
                self.family_for(dom_api)
            })
    }

    /// Combines the concrete representation and one API claim into the public family.
    fn family_for(&self, dom_api: DomApiFamily) -> DocumentFamily {
        match (dom_api, self.kind) {
            (DomApiFamily::Legacy, _) => DocumentFamily::Legacy,
            (DomApiFamily::Modern, DocumentKind::Xml) => DocumentFamily::ModernXml,
            (DomApiFamily::Modern, DocumentKind::Html) => DocumentFamily::ModernHtml,
        }
    }

    /// Returns the current pretty-print serialization flag.
    pub(crate) fn format_output(&self) -> bool {
        self.format_output.get()
    }

    /// Updates the shared pretty-print serialization flag.
    pub(crate) fn set_format_output(&self, value: bool) {
        self.format_output.set(value);
    }

    /// Returns one mutable legacy-document behavior flag.
    pub(crate) fn legacy_flag(&self, flag: LegacyDocumentFlag) -> bool {
        self.legacy_flags.get() & flag as u8 != 0
    }

    /// Updates one mutable legacy-document behavior flag.
    pub(crate) fn set_legacy_flag(
        &self,
        flag: LegacyDocumentFlag,
        value: bool,
    ) {
        let mask = flag as u8;
        let current = self.legacy_flags.get();
        self.legacy_flags.set(if value {
            current | mask
        } else {
            current & !mask
        });
    }

    /// Builds php-src's effective libxml parser options from legacy properties.
    pub(crate) fn legacy_parser_options(&self) -> i32 {
        let mut options = 0;
        if self.legacy_flag(LegacyDocumentFlag::Recover) {
            options |= 1;
        }
        if self.legacy_flag(LegacyDocumentFlag::SubstituteEntities) {
            options |= 2;
        }
        if self.legacy_flag(LegacyDocumentFlag::ResolveExternals) {
            options |= 8;
        }
        if self.legacy_flag(LegacyDocumentFlag::ValidateOnParse) {
            options |= 16;
        }
        if !self.legacy_flag(LegacyDocumentFlag::PreserveWhitespace) {
            options |= 256;
        }
        options
    }

    /// Registers, replaces, or resets one base-to-extended class mapping.
    pub(crate) fn set_node_class(&self, base: Vec<u8>, extended: Option<Vec<u8>>) {
        let base = php_class_key(&base);
        let mut classmap = self.classmap.borrow_mut();
        match extended {
            Some(extended) => {
                classmap.insert(base, extended);
            }
            None => {
                classmap.remove(&base);
            }
        }
    }

    /// Returns the extended class registered for one native base class.
    pub(crate) fn node_class(&self, base: &[u8]) -> Option<Vec<u8>> {
        self.classmap.borrow().get(&php_class_key(base)).cloned()
    }
}

/// Normalizes a PHP class name for ASCII-case-insensitive classmap lookup.
fn php_class_key(name: &[u8]) -> Vec<u8> {
    name.strip_prefix(b"\\")
        .unwrap_or(name)
        .iter()
        .map(u8::to_ascii_lowercase)
        .collect()
}

impl Drop for DocumentGraph {
    /// Frees the authoritative libxml2 graph exactly once.
    fn drop(&mut self) {
        unsafe {
            crate::native::document_free(self.pointer);
        }
    }
}

/// One PHP document wrapper retaining its authoritative shared graph.
pub(crate) struct DocumentObject {
    graph: Rc<DocumentGraph>,
}

impl DocumentObject {
    /// Takes ownership of one new authoritative document graph.
    pub(crate) fn new(pointer: usize, family: DocumentFamily) -> Self {
        Self {
            graph: Rc::new(DocumentGraph::new(pointer, family)),
        }
    }

    /// Builds another document handle retaining an existing authoritative graph.
    pub(crate) fn from_graph(graph: Rc<DocumentGraph>) -> Self {
        Self { graph }
    }

    /// Returns the private libxml2 document pointer.
    pub(crate) fn pointer(&self) -> usize {
        self.graph.pointer()
    }

    /// Returns the PHP wrapper family associated with this document.
    pub(crate) fn family(&self) -> DocumentFamily {
        self.graph.family()
    }

    /// Returns the current shared serialization-format flag.
    pub(crate) fn format_output(&self) -> bool {
        self.graph.format_output()
    }

    /// Updates the shared serialization-format flag.
    pub(crate) fn set_format_output(&self, value: bool) {
        self.graph.set_format_output(value);
    }

    /// Returns one mutable legacy-document behavior flag.
    pub(crate) fn legacy_flag(&self, flag: LegacyDocumentFlag) -> bool {
        self.graph.legacy_flag(flag)
    }

    /// Updates one mutable legacy-document behavior flag.
    pub(crate) fn set_legacy_flag(
        &self,
        flag: LegacyDocumentFlag,
        value: bool,
    ) {
        self.graph.set_legacy_flag(flag, value);
    }

    /// Builds effective legacy parsing options from retained wrapper flags.
    pub(crate) fn legacy_parser_options(&self) -> i32 {
        self.graph.legacy_parser_options()
    }

    /// Retains the authoritative graph for a node or secondary document handle.
    pub(crate) fn graph(&self) -> Rc<DocumentGraph> {
        Rc::clone(&self.graph)
    }

    /// Replaces the receiver graph after a successful legacy load operation.
    pub(crate) fn replace_pointer(&mut self, pointer: usize) {
        self.graph = Rc::new(self.graph.replacement(pointer));
    }

    /// Replaces a manually reconstructed document with a fresh default graph.
    ///
    /// Unlike a successful load, calling `DOMDocument::__construct()` starts a new
    /// php-src document resource rather than carrying parser flags, formatting, or
    /// registered node classes forward from the prior document.
    pub(crate) fn reconstruct(&mut self, pointer: usize, family: DocumentFamily) {
        self.graph = Rc::new(DocumentGraph::new(pointer, family));
    }
}

/// One libxml2 node handle retaining the document allocation that owns its metadata.
pub(crate) struct NodeObject {
    pointer: usize,
    document: Rc<DocumentGraph>,
    wrapper_kind: u64,
    owner_document_exposed: bool,
    _notation_allocation: Option<Rc<NotationNodeAllocation>>,
}

impl NodeObject {
    /// Creates a node record around one non-null libxml2 pointer and its document graph.
    pub(crate) fn new(
        pointer: usize,
        document: Rc<DocumentGraph>,
        wrapper_kind: u64,
    ) -> Self {
        assert_ne!(pointer, 0, "native node pointer must not be null");
        Self {
            pointer,
            document,
            wrapper_kind,
            owner_document_exposed: true,
            _notation_allocation: None,
        }
    }

    /// Creates a directly constructed legacy node whose private owner document is hidden.
    pub(crate) fn without_owner_document(
        pointer: usize,
        document: Rc<DocumentGraph>,
        wrapper_kind: u64,
    ) -> Self {
        assert_ne!(pointer, 0, "native node pointer must not be null");
        Self {
            pointer,
            document,
            wrapper_kind,
            owner_document_exposed: false,
            _notation_allocation: None,
        }
    }

    /// Takes ownership of one synthesized DTD notation node with no PHP owner document.
    pub(crate) fn notation(
        pointer: usize,
        document: Rc<DocumentGraph>,
        wrapper_kind: u64,
    ) -> Self {
        assert_ne!(pointer, 0, "native notation pointer must not be null");
        Self {
            pointer,
            document,
            wrapper_kind,
            owner_document_exposed: false,
            _notation_allocation: Some(Rc::new(NotationNodeAllocation::new(pointer))),
        }
    }

    /// Returns the private libxml2 node pointer for an immediate adapter call.
    pub(crate) fn pointer(&self) -> usize {
        self.pointer
    }

    /// Retains the authoritative graph used to validate cross-document mutations.
    pub(crate) fn document(&self) -> Rc<DocumentGraph> {
        Rc::clone(&self.document)
    }

    /// Returns the concrete PHP class discriminator fixed when this wrapper was minted.
    pub(crate) fn wrapper_kind(&self) -> u64 {
        self.wrapper_kind
    }

    /// Reports whether PHP exposes this node's retained native owner document.
    pub(crate) fn owner_document_exposed(&self) -> bool {
        self.owner_document_exposed
    }

    /// Rehomes one adopted native node under its new authoritative document graph.
    pub(crate) fn replace_document(&mut self, document: Rc<DocumentGraph>) {
        self.document = document;
        self.owner_document_exposed = true;
    }

    /// Rebinds the same PHP wrapper to a newly constructed standalone legacy node.
    ///
    /// The wrapper discriminator deliberately survives so a user subclass keeps
    /// its PHP identity. The replacement node starts with `ownerDocument === null`
    /// even when the prior node had already been attached to another graph.
    pub(crate) fn reconstruct_without_owner_document(
        &mut self,
        pointer: usize,
        document: Rc<DocumentGraph>,
    ) -> usize {
        assert_ne!(pointer, 0, "native node pointer must not be null");
        let previous_pointer = self.pointer;
        self.pointer = pointer;
        self.document = document;
        self.owner_document_exposed = false;
        self._notation_allocation = None;
        previous_pointer
    }
}

/// One standalone fake DTD notation node retained by exactly one PHP wrapper.
///
/// php-src creates a new `xmlEntity`-shaped `XML_NOTATION_NODE` for every map
/// lookup. It is detached from the document graph and must therefore be freed when
/// that wrapper's handle is released, rather than during broad context teardown.
pub(crate) struct NotationNodeAllocation {
    pointer: usize,
}

impl NotationNodeAllocation {
    /// Takes ownership of one synthesized notation pointer.
    pub(crate) fn new(pointer: usize) -> Self {
        assert_ne!(pointer, 0, "native notation pointer must not be null");
        Self { pointer }
    }
}

impl Drop for NotationNodeAllocation {
    /// Releases the fake notation and its separately allocated libxml2 strings.
    fn drop(&mut self) {
        unsafe {
            crate::native::notation_node_free(self.pointer);
        }
    }
}

/// One standalone legacy XPath namespace-declaration wrapper.
///
/// One shared owner of a standalone fake namespace-declaration `xmlNode`.
///
/// The fake node (type `XML_NAMESPACE_DECL`) is freed exactly once when the last
/// `Rc` reference drops. Both the originating XPath snapshot slot and every
/// materialized `NamespaceNodeObject` retain this allocation, so the wrapper cache
/// can disappear without clearing the slot (the list can recreate the wrapper) and
/// the snapshot can be released while a wrapper keeps the fake node alive.
pub(crate) struct NamespaceNodeAllocation {
    pointer: usize,
    parent: usize,
}

impl NamespaceNodeAllocation {
    /// Takes ownership of one fake namespace-declaration node and its parent element.
    pub(crate) fn new(pointer: usize, parent: usize) -> Self {
        assert_ne!(pointer, 0, "native namespace node pointer must not be null");
        Self { pointer, parent }
    }

    /// Returns the private fake namespace-declaration node pointer.
    pub(crate) fn pointer(&self) -> usize {
        self.pointer
    }

    /// Returns the owning parent element pointer used for invalidation and traversal.
    pub(crate) fn parent(&self) -> usize {
        self.parent
    }
}

impl Drop for NamespaceNodeAllocation {
    /// Frees the standalone fake namespace-declaration node exactly once.
    fn drop(&mut self) {
        unsafe {
            crate::native::namespace_node_free(self.pointer);
        }
    }
}

/// One standalone legacy XPath namespace-declaration wrapper.
///
/// Shares a `NamespaceNodeAllocation` with the originating snapshot slot (and with
/// sibling wrappers materialized from the same slot), plus the authoritative
/// document graph that keeps its parent element alive. The fake node is freed by the
/// shared allocation when its last reference drops, so releasing the PHP wrapper
/// never leaves a dangling libxml pointer and never clears the snapshot slot.
pub(crate) struct NamespaceNodeObject {
    allocation: Rc<NamespaceNodeAllocation>,
    document: Rc<DocumentGraph>,
}

impl NamespaceNodeObject {
    /// Shares one namespace-declaration allocation and its document graph.
    pub(crate) fn new(
        allocation: Rc<NamespaceNodeAllocation>,
        document: Rc<DocumentGraph>,
    ) -> Self {
        Self {
            allocation,
            document,
        }
    }

    /// Returns the private fake namespace-declaration node pointer.
    pub(crate) fn pointer(&self) -> usize {
        self.allocation.pointer()
    }

    /// Returns the owning parent element pointer used for invalidation and traversal.
    pub(crate) fn parent(&self) -> usize {
        self.allocation.parent()
    }

    /// Retains the authoritative document graph keeping the parent element alive.
    pub(crate) fn document(&self) -> Rc<DocumentGraph> {
        Rc::clone(&self.document)
    }
}

/// Native object variants retained by one bridge context.
pub(crate) enum NativeObject {
    Document(DocumentObject),
    Node(NodeObject),
    InvalidNode,
    Collection(CollectionObject),
    Implementation(ImplementationObject),
    TokenList(TokenListObject),
    InvalidTokenList,
    XPath(XPathObject),
    NamespaceNode(NamespaceNodeObject),
    InvalidNamespaceNode,
    #[cfg_attr(not(test), allow(dead_code))]
    SimpleXml(SimpleXmlObject),
}

impl NativeObject {
    /// Borrows this entry as a document or rejects a cross-kind variant.
    pub(crate) fn document(&self) -> Option<&DocumentObject> {
        match self {
            Self::Document(document) => Some(document),
            Self::Node(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::Implementation(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList
            | Self::XPath(_)
            | Self::NamespaceNode(_)
            | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
        }
    }

    /// Mutably borrows this entry as a document or rejects a cross-kind variant.
    pub(crate) fn document_mut(&mut self) -> Option<&mut DocumentObject> {
        match self {
            Self::Document(document) => Some(document),
            Self::Node(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::Implementation(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList
            | Self::XPath(_) | Self::NamespaceNode(_) | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
        }
    }

    /// Borrows this entry as a node or rejects a cross-kind variant.
    pub(crate) fn node(&self) -> Option<&NodeObject> {
        match self {
            Self::Document(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::Implementation(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList
            | Self::XPath(_) | Self::NamespaceNode(_) | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
            Self::Node(node) => Some(node),
        }
    }

    /// Mutably borrows this entry as a node or rejects a cross-kind variant.
    pub(crate) fn node_mut(&mut self) -> Option<&mut NodeObject> {
        match self {
            Self::Document(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::Implementation(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList
            | Self::XPath(_) | Self::NamespaceNode(_) | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
            Self::Node(node) => Some(node),
        }
    }

    /// Borrows this entry as a live collection or rejects a cross-kind variant.
    pub(crate) fn collection(&self) -> Option<&CollectionObject> {
        match self {
            Self::Collection(collection) => Some(collection),
            Self::Document(_)
            | Self::Node(_)
            | Self::InvalidNode
            | Self::Implementation(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList
            | Self::XPath(_) | Self::NamespaceNode(_) | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
        }
    }

    /// Mutably borrows this entry as a live collection descriptor.
    pub(crate) fn collection_mut(&mut self) -> Option<&mut CollectionObject> {
        match self {
            Self::Collection(collection) => Some(collection),
            Self::Document(_)
            | Self::Node(_)
            | Self::InvalidNode
            | Self::Implementation(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList
            | Self::XPath(_) | Self::NamespaceNode(_) | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
        }
    }

    /// Borrows this entry as a stateless DOM implementation wrapper.
    pub(crate) fn implementation(&self) -> Option<&ImplementationObject> {
        match self {
            Self::Implementation(implementation) => Some(implementation),
            Self::Document(_)
            | Self::Node(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList
            | Self::XPath(_) | Self::NamespaceNode(_) | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
        }
    }

    /// Borrows this entry as a modern class-token list wrapper.
    pub(crate) fn token_list(&self) -> Option<&TokenListObject> {
        match self {
            Self::TokenList(token_list) => Some(token_list),
            Self::Document(_)
            | Self::Node(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::Implementation(_)
            | Self::InvalidTokenList
            | Self::XPath(_) | Self::NamespaceNode(_) | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
        }
    }

    /// Mutably borrows this entry as a modern class-token list wrapper.
    pub(crate) fn token_list_mut(&mut self) -> Option<&mut TokenListObject> {
        match self {
            Self::TokenList(token_list) => Some(token_list),
            Self::Document(_)
            | Self::Node(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::Implementation(_)
            | Self::InvalidTokenList
            | Self::XPath(_) | Self::NamespaceNode(_) | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
        }
    }

    /// Borrows this entry as an XPath context or rejects a cross-kind variant.
    pub(crate) fn xpath(&self) -> Option<&XPathObject> {
        match self {
            Self::XPath(xpath) => Some(xpath),
            Self::Document(_)
            | Self::Node(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::Implementation(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList | Self::NamespaceNode(_) | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
        }
    }

    /// Mutably borrows this entry as an XPath context.
    pub(crate) fn xpath_mut(&mut self) -> Option<&mut XPathObject> {
        match self {
            Self::XPath(xpath) => Some(xpath),
            Self::Document(_)
            | Self::Node(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::Implementation(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList | Self::NamespaceNode(_) | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
        }
    }

    /// Borrows this entry as a namespace-declaration wrapper or rejects a cross-kind variant.
    pub(crate) fn namespace_node(&self) -> Option<&NamespaceNodeObject> {
        match self {
            Self::NamespaceNode(namespace_node) => Some(namespace_node),
            Self::Document(_)
            | Self::Node(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::Implementation(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList
            | Self::XPath(_)
            | Self::InvalidNamespaceNode
            | Self::SimpleXml(_) => None,
        }
    }

    /// Borrows this entry as a SimpleXML view or rejects another handle kind.
    pub(crate) fn simplexml(&self) -> Option<&SimpleXmlObject> {
        match self {
            Self::SimpleXml(simplexml) => Some(simplexml),
            Self::Document(_)
            | Self::Node(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::Implementation(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList
            | Self::XPath(_)
            | Self::NamespaceNode(_)
            | Self::InvalidNamespaceNode => None,
        }
    }

    /// Mutably borrows this entry as a SimpleXML view or rejects another handle kind.
    pub(crate) fn simplexml_mut(&mut self) -> Option<&mut SimpleXmlObject> {
        match self {
            Self::SimpleXml(simplexml) => Some(simplexml),
            Self::Document(_)
            | Self::Node(_)
            | Self::InvalidNode
            | Self::Collection(_)
            | Self::Implementation(_)
            | Self::TokenList(_)
            | Self::InvalidTokenList
            | Self::XPath(_)
            | Self::NamespaceNode(_)
            | Self::InvalidNamespaceNode => None,
        }
    }

    /// Returns whether XInclude invalidated the native node behind this handle.
    pub(crate) fn is_invalid_node(&self) -> bool {
        matches!(self, Self::InvalidNode)
    }

    /// Returns whether XInclude invalidated this token list's native element.
    pub(crate) fn is_invalid_token_list(&self) -> bool {
        matches!(self, Self::InvalidTokenList)
    }

    /// Returns whether XInclude invalidated this namespace-declaration wrapper's parent.
    pub(crate) fn is_invalid_namespace_node(&self) -> bool {
        matches!(self, Self::InvalidNamespaceNode)
    }
}
