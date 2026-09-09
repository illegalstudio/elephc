//! Purpose:
//! php-src's `XML_ErrorString` message table (`ext/xml/compat.c` `error_mapping[]`), the
//! one piece of `ext/xml` error handling that lives in PHP rather than in libxml2: the
//! numeric codes themselves come from the library through the shim.
//!
//! Called from:
//! - `crate::parser::error_string`, and through it `crate::abi::elephc_xml_error_string`.
//!
//! Key details:
//! - The table is copied verbatim, including the entry php-src dropped on purpose (so
//!   codes from 44 on are shifted against `xmlerror.h`) and the `"Unknown"` fallback.

/// php-src `compat.c` `error_mapping[]`, index = error code. Entry 44 onwards is shifted
/// by one relative to `xmlerror.h` because `XML_ERR_COMMENT_NOT_STARTED` was dropped on
/// purpose; the table is copied verbatim.
const ERROR_MAPPING: [&str; 102] = [
    "No error",
    "No memory",
    "Invalid document start",
    "Empty document",
    "Not well-formed (invalid token)",
    "Invalid document end",
    "Invalid hexadecimal character reference",
    "Invalid decimal character reference",
    "Invalid character reference",
    "Invalid character",
    "XML_ERR_CHARREF_AT_EOF",
    "XML_ERR_CHARREF_IN_PROLOG",
    "XML_ERR_CHARREF_IN_EPILOG",
    "XML_ERR_CHARREF_IN_DTD",
    "XML_ERR_ENTITYREF_AT_EOF",
    "XML_ERR_ENTITYREF_IN_PROLOG",
    "XML_ERR_ENTITYREF_IN_EPILOG",
    "XML_ERR_ENTITYREF_IN_DTD",
    "PEReference at end of document",
    "PEReference in prolog",
    "PEReference in epilog",
    "PEReference: forbidden within markup decl in internal subset",
    "XML_ERR_ENTITYREF_NO_NAME",
    "EntityRef: expecting ';'",
    "PEReference: no name",
    "PEReference: expecting ';'",
    "Undeclared entity error",
    "Undeclared entity warning",
    "Unparsed Entity",
    "XML_ERR_ENTITY_IS_EXTERNAL",
    "XML_ERR_ENTITY_IS_PARAMETER",
    "Unknown encoding",
    "Unsupported encoding",
    "String not started expecting ' or \"",
    "String not closed expecting \" or '",
    "Namespace declaration error",
    "EntityValue: \" or ' expected",
    "EntityValue: \" or ' expected",
    "< in attribute",
    "Attribute not started",
    "Attribute not finished",
    "Attribute without value",
    "Attribute redefined",
    "SystemLiteral \" or ' expected",
    "SystemLiteral \" or ' expected",
    "Comment not finished",
    "Processing Instruction not started",
    "Processing Instruction not finished",
    "NOTATION: Name expected here",
    "'>' required to close NOTATION declaration",
    "'(' required to start ATTLIST enumeration",
    "'(' required to start ATTLIST enumeration",
    "MixedContentDecl : '|' or ')*' expected",
    "XML_ERR_MIXED_NOT_FINISHED",
    "ELEMENT in DTD not started",
    "ELEMENT in DTD not finished",
    "XML declaration not started",
    "XML declaration not finished",
    "XML_ERR_CONDSEC_NOT_STARTED",
    "XML conditional section not closed",
    "Content error in the external subset",
    "DOCTYPE not finished",
    "Sequence ']]>' not allowed in content",
    "CDATA not finished",
    "Reserved XML Name",
    "Space required",
    "XML_ERR_SEPARATOR_REQUIRED",
    "NmToken expected in ATTLIST enumeration",
    "XML_ERR_NAME_REQUIRED",
    "MixedContentDecl : '#PCDATA' expected",
    "SYSTEM or PUBLIC, the URI is missing",
    "PUBLIC, the Public Identifier is missing",
    "< required",
    "> required",
    "</ required",
    "= required",
    "Mismatched tag",
    "Tag not finished",
    "standalone accepts only 'yes' or 'no'",
    "Invalid XML encoding name",
    "Comment must not contain '--' (double-hyphen)",
    "Invalid encoding",
    "external parsed entities cannot be standalone",
    "XML conditional section '[' expected",
    "Entity value required",
    "chunk is not well balanced",
    "extra content at the end of well balanced chunk",
    "XML_ERR_ENTITY_CHAR_ERROR",
    "PEReferences forbidden in internal subset",
    "Detected an entity reference loop",
    "XML_ERR_ENTITY_BOUNDARY",
    "Invalid URI",
    "Fragment not allowed",
    "XML_WAR_CATALOG_PI",
    "XML_ERR_NO_DTD",
    "conditional section INCLUDE or IGNORE keyword expected",
    "Version in XML Declaration missing",
    "XML_WAR_UNKNOWN_VERSION",
    "XML_WAR_LANG_VALUE",
    "XML_WAR_NS_URI",
    "XML_WAR_NS_URI_RELATIVE",
    "Missing encoding in text declaration",
];

/// php-src's `XML_ErrorString`: the table entry, or `"Unknown"` outside it.
pub(super) fn error_string(code: i32) -> &'static str {
    if code < 0 {
        return "Unknown";
    }
    ERROR_MAPPING.get(code as usize).copied().unwrap_or("Unknown")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The message table matches php-src, including the shifted tail and the fallback.
    #[test]
    fn message_table_matches_php() {
        assert_eq!(error_string(0), "No error");
        assert_eq!(error_string(23), "EntityRef: expecting ';'");
        assert_eq!(error_string(44), "SystemLiteral \" or ' expected");
        assert_eq!(error_string(76), "Mismatched tag");
        assert_eq!(error_string(101), "Missing encoding in text declaration");
        assert_eq!(error_string(102), "Unknown");
        assert_eq!(error_string(114), "Unknown");
        assert_eq!(error_string(-1), "Unknown");
    }
}
