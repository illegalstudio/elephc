//! Purpose:
//! Reads unbound DOM document text without including comments or processing instructions.
//!
//! Called from:
//! - The legacy DOMDocument textContent property hook.
//!
//! Key details:
//! - Bound documents use their shared native tree; HTML-only documents walk their node lists.

/// Collects text and CDATA through ordinary element/document descendants.
pub(super) const SRC: &str = r###"
function __elephc_dom_descendant_text(DOMNode $node): string {
    if ($node instanceof DOMText) { return (string)$node->nodeValue; }
    if (!($node instanceof DOMElement) && !($node instanceof DOMDocument)) { return ''; }
    $text = '';
    $children = $node->childNodes;
    if ($children instanceof DOMNodeList) {
        foreach ($children as $child) {
            if ($child instanceof DOMNode) { $text = $text . __elephc_dom_descendant_text($child); }
        }
    }
    return $text;
}
"###;
