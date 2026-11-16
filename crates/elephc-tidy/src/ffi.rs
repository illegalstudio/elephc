//! Purpose:
//! Raw `extern "C"` declarations for the libtidy 5.8.0 public API used by the
//! `elephc-tidy` bridge, plus the public `TidyBuffer` layout and option enums.
//!
//! Called from:
//! - `crate::registry`, `crate::document`, `crate::option`, and `crate::node`.
//!
//! Key details:
//! - These symbols are declared, never linked, at `cargo build -p elephc-tidy`
//!   time: the crate's staticlib leaves them unresolved until the final PHP
//!   program link supplies the managed `libtidy.a`. No bindgen and no sys crate.
//! - Every libtidy handle is opaque. `TidyBuffer` is the one public struct the
//!   API requires the caller to allocate, and its layout is part of libtidy's
//!   public header, so it is mirrored here field for field.
//! - The option ids used by the bridge (`TidyForceOutput`, `TidyMark`) and the
//!   option types come from tidyenum.h in libtidy 5.8.0.

use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};

/// libtidy's `TidyDoc`, an opaque document handle.
pub(crate) enum TidyDoc {}

/// libtidy's `TidyNode`, an opaque node handle owned by a document.
pub(crate) enum TidyNode {}

/// libtidy's `TidyAttr`, an opaque attribute handle owned by a node.
pub(crate) enum TidyAttr {}

/// libtidy's `TidyOption`, an opaque configuration-option handle.
pub(crate) enum TidyOption {}

/// libtidy's iterator token, an opaque pointer the caller must not dereference.
pub(crate) type TidyIterator = *mut c_void;

/// libtidy's `Bool`: `0` is false, any non-zero value is true.
pub(crate) type Bool = c_int;

/// libtidy's public `TidyBuffer` (`include/tidybuffio.h`).
#[repr(C)]
pub(crate) struct TidyBuffer {
    /// Allocator used for `bp`; null means the default allocator.
    pub allocator: *mut c_void,
    /// Heap bytes owned by the buffer.
    pub bp: *mut u8,
    /// Number of bytes currently in use, including the trailing NUL for text.
    pub size: c_uint,
    /// Number of bytes allocated at `bp`.
    pub allocated: c_uint,
    /// Current input position for attached buffers.
    pub next: c_uint,
}

unsafe impl Send for TidyBuffer {}

impl TidyBuffer {
    /// Returns an all-zero buffer ready for `tidyBufInit`.
    pub(crate) fn zeroed() -> Self {
        Self {
            allocator: std::ptr::null_mut(),
            bp: std::ptr::null_mut(),
            size: 0,
            allocated: 0,
            next: 0,
        }
    }
}

/// `TidyOptionType::TidyString`.
pub(crate) const OPTION_TYPE_STRING: c_int = 0;
/// `TidyOptionType::TidyInteger`.
pub(crate) const OPTION_TYPE_INTEGER: c_int = 1;
/// `TidyOptionType::TidyBoolean`.
pub(crate) const OPTION_TYPE_BOOLEAN: c_int = 2;
/// `TidyConfigCategory::TidyInternalCategory` in libtidy 5.8.0.
pub(crate) const OPTION_CATEGORY_INTERNAL: c_int = 312;
/// `TidyOptionId::TidyForceOutput`.
pub(crate) const OPTION_FORCE_OUTPUT: c_int = 30;
/// `TidyOptionId::TidyMark`.
pub(crate) const OPTION_MARK: c_int = 49;
/// `TidyNodeType::TidyNode_Comment`.
pub(crate) const NODE_TYPE_COMMENT: c_int = 2;
/// `TidyNodeType::TidyNode_Text`.
pub(crate) const NODE_TYPE_TEXT: c_int = 4;
/// `TidyNodeType::TidyNode_Start`.
pub(crate) const NODE_TYPE_START: c_int = 5;
/// `TidyNodeType::TidyNode_End`.
pub(crate) const NODE_TYPE_END: c_int = 6;
/// `TidyNodeType::TidyNode_StartEnd`.
pub(crate) const NODE_TYPE_START_END: c_int = 7;
/// `TidyNodeType::TidyNode_Asp`.
pub(crate) const NODE_TYPE_ASP: c_int = 10;
/// `TidyNodeType::TidyNode_Jste`.
pub(crate) const NODE_TYPE_JSTE: c_int = 11;
/// `TidyNodeType::TidyNode_Php`.
pub(crate) const NODE_TYPE_PHP: c_int = 12;

extern "C" {
    /// Creates a document with the default allocator.
    pub(crate) fn tidyCreate() -> *mut TidyDoc;
    /// Frees a document and everything it owns.
    pub(crate) fn tidyRelease(doc: *mut TidyDoc);
    /// Installs a growable error buffer on `doc`; returns 0 on success.
    pub(crate) fn tidySetErrorBuffer(doc: *mut TidyDoc, buffer: *mut TidyBuffer) -> c_int;
    /// Initializes a caller-owned buffer.
    pub(crate) fn tidyBufInit(buffer: *mut TidyBuffer);
    /// Frees the heap bytes owned by a buffer.
    pub(crate) fn tidyBufFree(buffer: *mut TidyBuffer);
    /// Attaches an existing byte range to a buffer without taking ownership.
    pub(crate) fn tidyBufAttach(buffer: *mut TidyBuffer, bytes: *mut u8, size: c_uint);
    /// Applies a boolean option value by id.
    pub(crate) fn tidyOptSetBool(doc: *mut TidyDoc, option: c_int, value: Bool) -> Bool;
    /// Selects the input character encoding; negative means unsupported.
    pub(crate) fn tidySetCharEncoding(doc: *mut TidyDoc, encoding: *const c_char) -> c_int;
    /// Loads a configuration file; negative is an I/O failure, positive a parse error.
    pub(crate) fn tidyLoadConfig(doc: *mut TidyDoc, path: *const c_char) -> c_int;
    /// Parses an attached byte buffer.
    pub(crate) fn tidyParseBuffer(doc: *mut TidyDoc, buffer: *mut TidyBuffer) -> c_int;
    /// Runs cleanup and repair.
    pub(crate) fn tidyCleanAndRepair(doc: *mut TidyDoc) -> c_int;
    /// Runs diagnostics after a parse.
    pub(crate) fn tidyRunDiagnostics(doc: *mut TidyDoc) -> c_int;
    /// Serializes the current document into `buffer`; returns the document status.
    pub(crate) fn tidySaveBuffer(doc: *mut TidyDoc, buffer: *mut TidyBuffer) -> c_int;
    /// Returns 0 for clean, 1 for warnings, 2 for errors.
    pub(crate) fn tidyStatus(doc: *mut TidyDoc) -> c_int;
    /// Returns the detected HTML version.
    pub(crate) fn tidyDetectedHtmlVersion(doc: *mut TidyDoc) -> c_int;
    /// Returns whether the document was detected as XHTML.
    pub(crate) fn tidyDetectedXhtml(doc: *mut TidyDoc) -> Bool;
    /// Returns whether the document was detected as generic XML.
    pub(crate) fn tidyDetectedGenericXml(doc: *mut TidyDoc) -> Bool;
    /// Number of errors.
    pub(crate) fn tidyErrorCount(doc: *mut TidyDoc) -> c_uint;
    /// Number of warnings.
    pub(crate) fn tidyWarningCount(doc: *mut TidyDoc) -> c_uint;
    /// Number of accessibility warnings.
    pub(crate) fn tidyAccessWarningCount(doc: *mut TidyDoc) -> c_uint;
    /// Number of configuration errors.
    pub(crate) fn tidyConfigErrorCount(doc: *mut TidyDoc) -> c_uint;
    /// Starts an iteration over every configuration option.
    pub(crate) fn tidyGetOptionList(doc: *mut TidyDoc) -> TidyIterator;
    /// Advances an option iterator; returns null at the end.
    pub(crate) fn tidyGetNextOption(doc: *mut TidyDoc, iter: *mut TidyIterator) -> *mut TidyOption;
    /// Looks up one option by its exact name.
    pub(crate) fn tidyGetOptionByName(doc: *mut TidyDoc, name: *const c_char) -> *mut TidyOption;
    /// Returns an option's canonical name.
    pub(crate) fn tidyOptGetName(option: *mut TidyOption) -> *const c_char;
    /// Returns an option's type (`TidyOptionType`).
    pub(crate) fn tidyOptGetType(option: *mut TidyOption) -> c_int;
    /// Returns an option's configuration category.
    pub(crate) fn tidyOptGetCategory(option: *mut TidyOption) -> c_int;
    /// Returns an option's numeric id.
    pub(crate) fn tidyOptGetId(option: *mut TidyOption) -> c_int;
    /// Returns a string option's current value.
    pub(crate) fn tidyOptGetValue(doc: *mut TidyDoc, option: c_int) -> *const c_char;
    /// Returns an integer option's current value.
    pub(crate) fn tidyOptGetInt(doc: *mut TidyDoc, option: c_int) -> c_ulong;
    /// Returns a boolean option's current value.
    pub(crate) fn tidyOptGetBool(doc: *mut TidyDoc, option: c_int) -> Bool;
    /// Sets a string option value; returns false when the value is rejected.
    pub(crate) fn tidyOptSetValue(doc: *mut TidyDoc, option: c_int, value: *const c_char) -> Bool;
    /// Sets an integer option value; returns false when the value is rejected.
    pub(crate) fn tidyOptSetInt(doc: *mut TidyDoc, option: c_int, value: c_ulong) -> Bool;
    /// Returns an option's help text, or null when it has none.
    pub(crate) fn tidyOptGetDoc(doc: *mut TidyDoc, option: *mut TidyOption) -> *const c_char;
    /// Returns the document root node.
    pub(crate) fn tidyGetRoot(doc: *mut TidyDoc) -> *mut TidyNode;
    /// Returns the `html` element node.
    pub(crate) fn tidyGetHtml(doc: *mut TidyDoc) -> *mut TidyNode;
    /// Returns the `head` element node.
    pub(crate) fn tidyGetHead(doc: *mut TidyDoc) -> *mut TidyNode;
    /// Returns the `body` element node.
    pub(crate) fn tidyGetBody(doc: *mut TidyDoc) -> *mut TidyNode;
    /// Returns a node's parent.
    pub(crate) fn tidyGetParent(node: *mut TidyNode) -> *mut TidyNode;
    /// Returns a node's first child.
    pub(crate) fn tidyGetChild(node: *mut TidyNode) -> *mut TidyNode;
    /// Returns a node's next sibling.
    pub(crate) fn tidyGetNext(node: *mut TidyNode) -> *mut TidyNode;
    /// Returns a node's previous sibling.
    pub(crate) fn tidyGetPrev(node: *mut TidyNode) -> *mut TidyNode;
    /// Returns a node's type (`TidyNodeType`).
    pub(crate) fn tidyNodeGetType(node: *mut TidyNode) -> c_int;
    /// Returns a node's element name, or null for nodes without one.
    pub(crate) fn tidyNodeGetName(node: *mut TidyNode) -> *const c_char;
    /// Serializes a node subtree into `buffer`; returns false on failure.
    pub(crate) fn tidyNodeGetText(
        doc: *mut TidyDoc,
        node: *mut TidyNode,
        buffer: *mut TidyBuffer,
    ) -> Bool;
    /// Returns whether the node is proprietary.
    pub(crate) fn tidyNodeIsProp(doc: *mut TidyDoc, node: *mut TidyNode) -> Bool;
    /// Returns the node's element id (`TidyTagId`).
    pub(crate) fn tidyNodeGetId(node: *mut TidyNode) -> c_int;
    /// Returns the node's source line.
    pub(crate) fn tidyNodeLine(node: *mut TidyNode) -> c_uint;
    /// Returns the node's source column.
    pub(crate) fn tidyNodeColumn(node: *mut TidyNode) -> c_uint;
    /// Returns a node's first attribute.
    pub(crate) fn tidyAttrFirst(node: *mut TidyNode) -> *mut TidyAttr;
    /// Returns the next attribute.
    pub(crate) fn tidyAttrNext(attribute: *mut TidyAttr) -> *mut TidyAttr;
    /// Returns an attribute's name.
    pub(crate) fn tidyAttrName(attribute: *mut TidyAttr) -> *const c_char;
    /// Returns an attribute's value.
    pub(crate) fn tidyAttrValue(attribute: *mut TidyAttr) -> *const c_char;
    /// Returns libtidy's release date string.
    pub(crate) fn tidyReleaseDate() -> *const c_char;
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Layout and constant guards for the libtidy FFI mirror.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - `TidyBuffer` is part of libtidy's public ABI; its field order cannot drift.

    use super::*;

    /// The mirrored buffer layout has the five public fields in header order.
    #[test]
    fn tidy_buffer_layout_matches_the_public_header() {
        let buffer = TidyBuffer::zeroed();
        assert!(buffer.bp.is_null());
        assert_eq!(buffer.size, 0);
        assert_eq!(buffer.allocated, 0);
        assert_eq!(buffer.next, 0);
        assert_eq!(OPTION_TYPE_STRING, 0);
        assert_eq!(OPTION_TYPE_INTEGER, 1);
        assert_eq!(OPTION_TYPE_BOOLEAN, 2);
    }
}
