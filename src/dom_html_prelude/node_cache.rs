//! Purpose:
//! Preserves PHP DOM wrapper identity without retaining otherwise unused native nodes.
//!
//! Called from:
//! - SimpleXML's DOM wrapper creation and HTML import binding.
//!
//! Key details:
//! - Native node ids are compared only while their weak PHP wrappers remain alive.
//! - Geometric sweeps bound stale weak entries with amortized linear cleanup.

/// Looks up and remembers native-backed nodes in each document's weak wrapper cache.
pub(super) const SRC: &str = r###"
function __elephc_dom_cached_node(mixed $document, int $identity): ?DOMNode {
    if (!($document instanceof DOMDocument) || $identity === 0) { return null; }
    $weak = $document->__elephc_dom_nodes[$identity] ?? null;
    if ($weak instanceof WeakReference) {
        $node = $weak->get();
        if ($node instanceof DOMNode) { return $node; }
    }
    return null;
}
function __elephc_dom_remember_node(mixed $document, int $identity, DOMNode $node): void {
    if (!($document instanceof DOMDocument) || $identity === 0) { return; }
    $document->__elephc_dom_nodes[$identity] = WeakReference::create($node);
    if (count($document->__elephc_dom_nodes) < $document->__elephc_dom_sweep) { return; }
    $live = [];
    foreach ($document->__elephc_dom_nodes as $key => $weak) {
        if ($weak instanceof WeakReference && $weak->get() !== null) { $live[$key] = $weak; }
    }
    $document->__elephc_dom_nodes = $live;
    $next = count($live) * 2;
    $document->__elephc_dom_sweep = $next < 64 ? 64 : $next;
}
function __elephc_dom_drop_snapshot(DOMNode $node): void {
    $node->childNodes = null;
    $node->parentNode = null;
    $node->previousSibling = null;
    $node->nextSibling = null;
}
"###;
