//! Purpose:
//! Implements PHP DOM native-wrapper lifecycle guards shared by legacy and modern nodes.
//! Produces the exact concrete-class serialization and unserialization exceptions.
//!
//! Called from:
//! - `super::routes::dispatch()` for inherited `DOMNode` and `Dom\Node` hooks.
//!
//! Key details:
//! - The native wrapper discriminator selects the PHP-visible concrete wrapper class.
//! - These hooks are inherited only when a subclass has not supplied its own method.

use crate::context::Context;
use crate::request::Request;

use super::{
    receiver_pointer_and_graph, require_no_values, wrapper_kind, DispatchResult,
};

/// Rejects native DOM node serialization or unserialization with php-src's exact message.
pub(super) fn reject_node_serialization(
    context: &Context,
    request: &Request,
    serializing: bool,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let class_name = wrapper_class_name(wrapper_kind(&graph, pointer)).ok_or(())?;
    let action = if serializing {
        "Serialization"
    } else {
        "Unserialization"
    };
    Ok(DispatchResult::exception(
        format!(
            "{action} of '{class_name}' is not allowed, unless {} methods are implemented in a subclass",
            if serializing {
                "serialization"
            } else {
                "unserialization"
            }
        )
        .as_bytes(),
    ))
}

/// Maps one stable native wrapper discriminator to its PHP-visible concrete class.
fn wrapper_class_name(kind: u64) -> Option<&'static str> {
    match kind {
        101 => Some("DOMElement"),
        102 => Some("DOMAttr"),
        103 => Some("DOMText"),
        104 => Some("DOMCdataSection"),
        105 => Some("DOMEntityReference"),
        107 => Some("DOMProcessingInstruction"),
        108 => Some("DOMComment"),
        109 => Some("DOMDocument"),
        110 | 114 => Some("DOMDocumentType"),
        111 => Some("DOMDocumentFragment"),
        112 => Some("DOMNotation"),
        115 | 117 => Some("DOMEntity"),
        118 => Some("DOMNameSpaceNode"),
        201 => Some("Dom\\Element"),
        202 => Some("Dom\\Attr"),
        203 => Some("Dom\\Text"),
        204 => Some("Dom\\CDATASection"),
        205 => Some("Dom\\EntityReference"),
        207 => Some("Dom\\ProcessingInstruction"),
        208 => Some("Dom\\Comment"),
        209 => Some("Dom\\XMLDocument"),
        210 | 214 => Some("Dom\\DocumentType"),
        211 => Some("Dom\\DocumentFragment"),
        212 => Some("Dom\\Notation"),
        215 | 217 => Some("Dom\\Entity"),
        301 => Some("Dom\\HTMLElement"),
        302 => Some("Dom\\Attr"),
        303 => Some("Dom\\Text"),
        304 => Some("Dom\\CDATASection"),
        305 => Some("Dom\\EntityReference"),
        307 => Some("Dom\\ProcessingInstruction"),
        308 => Some("Dom\\Comment"),
        310 => Some("Dom\\DocumentType"),
        311 => Some("Dom\\DocumentFragment"),
        312 => Some("Dom\\Notation"),
        313 => Some("Dom\\HTMLDocument"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::wrapper_class_name;

    /// Verifies representative legacy, modern XML, and modern HTML wrapper names.
    #[test]
    fn wrapper_names_follow_the_stable_dom_discriminators() {
        assert_eq!(wrapper_class_name(101), Some("DOMElement"));
        assert_eq!(wrapper_class_name(109), Some("DOMDocument"));
        assert_eq!(wrapper_class_name(201), Some("Dom\\Element"));
        assert_eq!(wrapper_class_name(209), Some("Dom\\XMLDocument"));
        assert_eq!(wrapper_class_name(301), Some("Dom\\HTMLElement"));
        assert_eq!(wrapper_class_name(313), Some("Dom\\HTMLDocument"));
        assert_eq!(wrapper_class_name(999), None);
    }
}
