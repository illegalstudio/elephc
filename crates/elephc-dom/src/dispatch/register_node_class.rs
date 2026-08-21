//! Purpose:
//! Implements legacy and modern `registerNodeClass()` validation and classmap updates.
//! Resolves document-local wrapper overrides without PHP runtime callbacks.
//!
//! Called from:
//! - `super::routes::dispatch()` for the two locked register-node-class opcodes.
//! - `super::canonical_pointer_handle()` when a native wrapper is materialized.
//!
//! Key details:
//! - Validation mirrors php-src's class existence, inheritance, and abstract checks.
//! - Legacy returns `true`; modern returns void. A null extended class resets the map.
//! - The bridge returns mapped runtime class ids with the high-bit protocol marker.

use crate::context::Context;
use crate::objects::{DocumentFamily, DocumentGraph};
use crate::request::Request;

use super::{document, DispatchResult};

/// High-bit marker distinguishing a mapped userland class id from native wrapper kinds.
pub(super) const USER_WRAPPER_MARKER: u64 = 1u64 << 63;

/// Family root class used by php-src to validate the base-class argument.
const LEGACY_BASE_NODE: &[u8] = b"DOMNode";
/// Modern family root class used by php-src to validate the base-class argument.
const MODERN_BASE_NODE: &[u8] = b"Dom\\Node";

/// PHP-visible declaring class used by legacy validation diagnostics.
const LEGACY_DECLARING: &str = "DOMDocument";
/// PHP-visible declaring class used by modern validation diagnostics.
const MODERN_DECLARING: &str = "Dom\\Document";

/// Runs `registerNodeClass()` for one legacy or modern document family.
pub(super) fn register(
    context: &mut Context,
    request: &Request,
    modern: bool,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let graph = document(context, request.header.receiver)?.graph();
    let family_matches = if modern {
        graph.family() != DocumentFamily::Legacy
    } else {
        graph.family() == DocumentFamily::Legacy
    };
    if !family_matches {
        return Err(());
    }

    let base_name = request.byte_string(0)?;
    let extended_name = request.optional_byte_string(1)?;
    let declaring = if modern {
        MODERN_DECLARING
    } else {
        LEGACY_DECLARING
    };
    let base_node_name = if modern {
        MODERN_BASE_NODE
    } else {
        LEGACY_BASE_NODE
    };

    let base = context.class_metadata.by_name(base_name);
    let family_base = context.class_metadata.by_name(base_node_name);
    let base_is_derived = match (base.as_ref(), family_base.as_ref()) {
        (Some(base), Some(family_base)) => {
            context
                .class_metadata
                .is_subclass_of(base.id, family_base.id)
        }
        _ => false,
    };
    if !base_is_derived {
        let message = format!(
            "{declaring}::registerNodeClass(): Argument #1 ($baseClass) must be a class name \
             derived from {}, {} given",
            String::from_utf8_lossy(base_node_name),
            String::from_utf8_lossy(base_name),
        );
        return Ok(DispatchResult::type_error(message.as_bytes()));
    }

    let base = base.expect("derived base classes have metadata");
    if base.is_abstract {
        let message = format!(
            "{declaring}::registerNodeClass(): Argument #1 ($baseClass) must not be an abstract class"
        );
        return Ok(DispatchResult::value_error(message.as_bytes()));
    }

    let Some(extended_name) = extended_name else {
        graph.set_node_class(base.canonical_name, None);
        return Ok(success_result(modern));
    };
    let Some(extended) = context.class_metadata.by_name(extended_name) else {
        let message = format!(
            "{declaring}::registerNodeClass(): Argument #2 ($extendedClass) must be a valid class name or null, {} given",
            String::from_utf8_lossy(extended_name),
        );
        return Ok(DispatchResult::type_error(message.as_bytes()));
    };
    if !context
        .class_metadata
        .is_subclass_of(extended.id, base.id)
    {
        let message = format!(
            "{declaring}::registerNodeClass(): Argument #2 ($extendedClass) must be a \
             class name derived from {} or null, {} given",
            String::from_utf8_lossy(&base.canonical_name),
            String::from_utf8_lossy(&extended.canonical_name),
        );
        return Ok(DispatchResult::error(message.as_bytes()));
    }
    if extended.is_abstract {
        let message = format!(
            "{declaring}::registerNodeClass(): Argument #2 ($extendedClass) must not be an abstract class"
        );
        return Ok(DispatchResult::value_error(message.as_bytes()));
    }

    graph.set_node_class(base.canonical_name, Some(extended.canonical_name));
    Ok(success_result(modern))
}

/// Applies one document-local classmap entry to a native wrapper discriminator.
pub(super) fn mapped_wrapper_kind(
    context: &Context,
    graph: &DocumentGraph,
    native_kind: u64,
) -> u64 {
    let Some(base_name) = native_wrapper_class_name(native_kind) else {
        return native_kind;
    };
    let Some(extended_name) = graph.node_class(base_name) else {
        return native_kind;
    };
    let Some(extended) = context.class_metadata.by_name(&extended_name) else {
        return native_kind;
    };
    extended.id | USER_WRAPPER_MARKER
}

/// Builds the family-correct success result: `true` for legacy and void for modern.
fn success_result(modern: bool) -> DispatchResult {
    if modern {
        DispatchResult::null()
    } else {
        DispatchResult::boolean(true)
    }
}

/// Maps one native wrapper discriminator to its exact PHP-visible base class name.
fn native_wrapper_class_name(kind: u64) -> Option<&'static [u8]> {
    match kind {
        101 => Some(b"DOMElement"),
        102 => Some(b"DOMAttr"),
        103 => Some(b"DOMText"),
        104 => Some(b"DOMCdataSection"),
        105 => Some(b"DOMEntityReference"),
        107 => Some(b"DOMProcessingInstruction"),
        108 => Some(b"DOMComment"),
        109 => Some(b"DOMDocument"),
        110 | 114 => Some(b"DOMDocumentType"),
        111 => Some(b"DOMDocumentFragment"),
        112 => Some(b"DOMNotation"),
        115 | 117 => Some(b"DOMEntity"),
        118 => Some(b"DOMNameSpaceNode"),
        201 => Some(b"Dom\\Element"),
        202 => Some(b"Dom\\Attr"),
        203 => Some(b"Dom\\Text"),
        204 => Some(b"Dom\\CDATASection"),
        205 => Some(b"Dom\\EntityReference"),
        207 => Some(b"Dom\\ProcessingInstruction"),
        208 => Some(b"Dom\\Comment"),
        209 => Some(b"Dom\\XMLDocument"),
        210 | 214 => Some(b"Dom\\DocumentType"),
        211 => Some(b"Dom\\DocumentFragment"),
        212 => Some(b"Dom\\Notation"),
        215 | 217 => Some(b"Dom\\Entity"),
        301 => Some(b"Dom\\HTMLElement"),
        302 => Some(b"Dom\\Attr"),
        303 => Some(b"Dom\\Text"),
        304 => Some(b"Dom\\CDATASection"),
        305 => Some(b"Dom\\EntityReference"),
        307 => Some(b"Dom\\ProcessingInstruction"),
        308 => Some(b"Dom\\Comment"),
        310 => Some(b"Dom\\DocumentType"),
        311 => Some(b"Dom\\DocumentFragment"),
        312 => Some(b"Dom\\Notation"),
        313 => Some(b"Dom\\HTMLDocument"),
        _ => None,
    }
}
