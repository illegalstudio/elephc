//! Purpose:
//! PHP's classic DOM extension -- `DOMDocument`, `DOMNode`, `DOMElement`, `DOMText`, `DOMComment`,
//! `DOMCdataSection`, `DOMAttr`, `DOMDocumentFragment`, `DOMDocumentType`,
//! `DOMProcessingInstruction`, `DOMNodeList`, `DOMNamedNodeMap`, `DOMException`, `LibXMLError` -- with an XML
//! parser, an HTML 4 parser behind `loadHTML()`, and the XML / HTML serializers behind
//! `saveXML()` / `saveHTML()`, built as AST.
//!
//! Called from:
//! - `crate::pipeline::compile()`, twice like the other class-bearing preludes: once before name
//!   resolution and once after autoload expansion, since the program that names the surface is
//!   usually an autoloaded vendor class (Symfony's `XmlDescriptor`, Termwind's `HtmlRenderer`).
//! - The frontend test harnesses, through `inject()` (unconditional) or `inject_if_used()`.
//!
//! Key details:
//! - THE MODEL. `fragments` holds the whole surface as PHP; `build` is that PHP transcribed to
//!   builder calls (`synthetic_class::transcribe`), and `oracle` proves the two are the same AST.
//!   The PHP model itself was checked against php 8.5 (libxml 2.9.13): a randomized operation
//!   fuzzer over tree building and mutation compared serialized trees, the exceptions raised and
//!   their order; ~10,000 generated HTML documents compared parsed trees and the bytes of
//!   `saveHTML()` / `saveXML()` with and without `formatOutput`; the XML parser and the
//!   serializer were compared on targeted documents.
//! - STORAGE. Properties are real, eagerly maintained slots, not magic: the checker resolves a
//!   `__get` only on a receiver whose class it knows, and a `foreach` over a `DOMNodeList` yields
//!   an untyped element. `firstChild`, `parentNode`, `nextSibling` ... are relinked on every
//!   mutation; `textContent` and `nodeValue` are refreshed up the ancestor chain, and a parser
//!   defers that to one bottom-up pass (`DOMDocument::$building`). For character data, attributes
//!   and processing instructions, `data` / `value`, `nodeValue` and `textContent` are one
//!   reference-bound cell, so writing any of them is seen by all.
//! - KNOWN GAPS. Assigning `textContent` / `nodeValue` straight to an ELEMENT is applied lazily
//!   (at the next `saveXML()` / `saveHTML()` / `cloneNode()`), not immediately: a plain property
//!   store cannot be intercepted. `getElementsByTagName()` and the `DOMNodeList` of a query are
//!   snapshots, not live. There is no `DOMXPath`, no DTD/schema validation, no entity
//!   declarations in a DOCTYPE, and no libxml error list (`libxml_get_errors()` answers an empty list of `LibXMLError`);
//!   parse problems do not print php's warnings.
//! - HTML. `loadHTML()` reproduces libxml2 2.9.13's HTML 4 parser, not an HTML5 one: implied
//!   `html` / `head` / `body` and `<p>`, the auto-close table, end-tag priorities, `<script>` as
//!   a CDATA node, Latin-1 input unless a `<?xml encoding>` prolog or a `<meta charset>` says
//!   UTF-8, `LIBXML_HTML_NOIMPLIED`, `LIBXML_HTML_NODEFDTD` and `LIBXML_NOBLANKS`. It does not share
//!   code with the `Dom\HTMLDocument` HTML5 parser (`crate::dom_html_prelude`), whose tree builder
//!   differs in every way those flags observe and which is gated on php >= 8.4.
//! - Pay-for-use: injected only when the program names a DOM class or a `libxml_*` function and
//!   declares none of those classes itself. A program that reaches the surface only through
//!   `eval()` opaque source is not detected.

mod build;
#[cfg(test)]
mod fragments;
#[cfg(test)]
mod oracle;

use crate::parser::ast::{Program, Stmt, StmtKind};

/// Prelude inventory group holding the declarations.
pub const GROUP: &str = "dom";

/// Constants the prelude declares. They are php's PREDEFINED `dom` / `libxml` constants, so they
/// must not be reported as user constants: `get_defined_constants(true)['user']` listed
/// `XML_DOCUMENT_TYPE_NODE` in every program, ahead of the program's own.
pub const PRELUDE_CONSTANTS: [&str; 67] = [
    "LIBXML_VERSION",
    "LIBXML_DOTTED_VERSION",
    "LIBXML_LOADED_VERSION",
    "LIBXML_RECOVER",
    "LIBXML_NOENT",
    "LIBXML_DTDLOAD",
    "LIBXML_DTDATTR",
    "LIBXML_DTDVALID",
    "LIBXML_NOERROR",
    "LIBXML_NOWARNING",
    "LIBXML_PEDANTIC",
    "LIBXML_NOBLANKS",
    "LIBXML_XINCLUDE",
    "LIBXML_NSCLEAN",
    "LIBXML_NOCDATA",
    "LIBXML_NOXMLDECL",
    "LIBXML_PARSEHUGE",
    "LIBXML_BIGLINES",
    "LIBXML_NOEMPTYTAG",
    "LIBXML_SCHEMA_CREATE",
    "LIBXML_HTML_NOIMPLIED",
    "LIBXML_HTML_NODEFDTD",
    "XML_ELEMENT_NODE",
    "XML_ATTRIBUTE_NODE",
    "XML_TEXT_NODE",
    "XML_CDATA_SECTION_NODE",
    "XML_ENTITY_REF_NODE",
    "XML_ENTITY_NODE",
    "XML_PI_NODE",
    "XML_COMMENT_NODE",
    "XML_DOCUMENT_NODE",
    "XML_DOCUMENT_TYPE_NODE",
    "XML_DOCUMENT_FRAG_NODE",
    "XML_NOTATION_NODE",
    "XML_HTML_DOCUMENT_NODE",
    "XML_DTD_NODE",
    "XML_ELEMENT_DECL_NODE",
    "XML_ATTRIBUTE_DECL_NODE",
    "XML_ENTITY_DECL_NODE",
    "XML_NAMESPACE_DECL_NODE",
    "XML_LOCAL_NAMESPACE",
    "XML_ATTRIBUTE_CDATA",
    "XML_ATTRIBUTE_ID",
    "XML_ATTRIBUTE_IDREF",
    "XML_ATTRIBUTE_IDREFS",
    "XML_ATTRIBUTE_ENTITY",
    "XML_ATTRIBUTE_NMTOKEN",
    "XML_ATTRIBUTE_NMTOKENS",
    "XML_ATTRIBUTE_ENUMERATION",
    "XML_ATTRIBUTE_NOTATION",
    "DOM_PHP_ERR",
    "DOM_INDEX_SIZE_ERR",
    "DOMSTRING_SIZE_ERR",
    "DOM_HIERARCHY_REQUEST_ERR",
    "DOM_WRONG_DOCUMENT_ERR",
    "DOM_INVALID_CHARACTER_ERR",
    "DOM_NO_DATA_ALLOWED_ERR",
    "DOM_NO_MODIFICATION_ALLOWED_ERR",
    "DOM_NOT_FOUND_ERR",
    "DOM_NOT_SUPPORTED_ERR",
    "DOM_INUSE_ATTRIBUTE_ERR",
    "DOM_INVALID_STATE_ERR",
    "DOM_SYNTAX_ERR",
    "DOM_INVALID_MODIFICATION_ERR",
    "DOM_NAMESPACE_ERR",
    "DOM_INVALID_ACCESS_ERR",
    "DOM_VALIDATION_ERR",
];

/// The classes the prelude declares, lower-cased: what makes a program a DOM user.
const CLASSES: [&str; 14] = [
    "domexception",
    "domnode",
    "domdocument",
    "domelement",
    "domtext",
    "domcomment",
    "domcdatasection",
    "domattr",
    "domdocumentfragment",
    "domdocumenttype",
    "domprocessinginstruction",
    "domnodelist",
    "domnamednodemap",
    "libxmlerror",
];

/// Free functions the prelude declares: a program calling one of them wants the surface too.
const FUNCTIONS: [&str; 4] = [
    "libxml_use_internal_errors",
    "libxml_clear_errors",
    "libxml_get_errors",
    "libxml_disable_entity_loader",
];

/// Builds the declarations, one per helper.
pub(crate) fn declarations() -> Program {
    build::dom_declarations()
}

/// Whether a statement list declares one of the prelude's classes.
fn program_declares_dom_class(program: &[Stmt]) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::ClassDecl { name, .. } => CLASSES.contains(&name.to_ascii_lowercase().as_str()),
        StmtKind::NamespaceBlock { name: None, body }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => program_declares_dom_class(body),
        _ => false,
    })
}

/// Whether the program names a DOM class or calls a `libxml_*` function, however the reference
/// is written.
fn program_uses_dom(program: &[Stmt]) -> bool {
    let used = crate::prelude_prune::usage::collect(program);
    used.classes
        .iter()
        .chain(used.literals.iter())
        .any(|name| CLASSES.contains(&name.trim_start_matches('\\').to_ascii_lowercase().as_str()))
        || used.functions.iter().any(|name| FUNCTIONS.contains(&name.as_str()))
}

/// Prepends the DOM declarations unconditionally, without recording them anywhere.
///
/// Used by the frontend harnesses that type-check a snippet against the surface; the compiler
/// itself goes through `inject_if_used()`.
pub fn inject(program: Program) -> Program {
    let mut combined = declarations();
    combined.extend(program);
    combined
}

/// Prepends the declarations when the program names a DOM class or a `libxml_*` function and
/// declares none of the classes itself; records them in the prelude inventory so closed-world
/// pruning can drop what the program never reaches.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if inventory.groups.contains_key(GROUP) {
        return program;
    }
    if !program_uses_dom(&program) || program_declares_dom_class(&program) {
        return program;
    }
    let mut combined = declarations();
    inventory.record_program(GROUP, &combined);
    combined.extend(program);
    combined
}
