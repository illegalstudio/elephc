---
title: "DOM HTML (Termwind subset)"
description: "A pay-for-use DOMDocument HTML fragment walker for Termwind-style tree walks, not full PHP DOM."
sidebar:
  order: 24
---

elephc injects a small **HTML-only DOM subset** when a program names `DOMDocument`,
`DOMNode`, `DOMElement`, `DOMText`, `DOMComment`, `DOMCharacterData`, or
`DOMNodeList`. The surface is enough for Termwind's `HtmlRenderer::parse` to
walk a fragment such as `<div class="text-green-500">Hi</div>`: load the HTML,
take `body`, and read tag names, attributes, text, comments, and siblings.

This is **not** a second native libxml/DOM stack. Draft
[PR #654](https://github.com/illegalstudio/elephc/pull/654) (issue
[#622](https://github.com/illegalstudio/elephc/issues/622)) remains the path to
full PHP 8.5 DOM, libxml, and SimpleXML on a statically linked `elephc-dom`
bridge. This prelude uses the same class names and `LIBXML_*` integers so that
work can replace it without changing Termwind call sites.

```php
<?php
$dom = new DOMDocument();
$dom->loadHTML(
    '<div class="text-green-500">Hi</div>',
    LIBXML_NOERROR | LIBXML_COMPACT | LIBXML_HTML_NODEFDTD | LIBXML_NOBLANKS | LIBXML_NOXMLDECL
);
$body = $dom->getElementsByTagName('body')->item(0);
foreach ($body->childNodes as $node) {
    echo $node->nodeName, ' ', $node->getAttribute('class'), ' ', $node->nodeValue;
}
```

## Supported surface

| Piece | Behavior |
|---|---|
| `new DOMDocument()` | Empty document. Optional constructor args are accepted and ignored. |
| `loadHTML(string $source, int $options = 0): bool` | Forgiving HTML fragment parse. Wraps content in `html`/`body` the way PHP's HTML parser does. |
| `getElementsByTagName(string $name): DOMNodeList` | Document-order descendant elements. `*` matches every element. |
| `DOMNodeList::item(int $index)` / `$length` / `foreach` | Indexed access and `Iterator` traversal. |
| `nodeName`, `nodeValue` | HTML tag names are lowercased. Element `nodeValue` is concatenated descendant text. |
| `childNodes`, `previousSibling`, `nextSibling`, `parentNode`, `ownerDocument` | Wired after parse. The tree is treated as immutable. |
| `DOMElement::getAttribute(string $name): string` | Attribute names are lowercased on parse. Missing attributes return `""`. |
| `instanceof DOMElement` / `DOMText` / `DOMComment` / `DOMDocument` | Class hierarchy matches PHP (`DOMText`/`DOMComment` extend `DOMCharacterData` extend `DOMNode`). |
| `saveXML(?DOMNode $node = null): string` | Serializes a node or the whole document. No XML declaration (Termwind passes `LIBXML_NOXMLDECL`). |

### `LIBXML_*` flags Termwind uses

These constants are always available (including inside a namespace, and in
`eval()`), with php-src's integer values:

| Constant | Value | Effect here |
|---|---|---|
| `LIBXML_NOXMLDECL` | 2 | Save flag; parse ignores it. `saveXML()` never emits `<?xml …?>`. |
| `LIBXML_HTML_NODEFDTD` | 4 | No default doctype is added (none is added anyway). |
| `LIBXML_NOERROR` | 32 | Parse is silent and forgiving. |
| `LIBXML_NOBLANKS` | 256 | Whitespace-only text nodes are dropped. |
| `LIBXML_COMPACT` | 65536 | Accepted and ignored (libxml compaction is an optimizer hint). |

## Remaining gaps versus full PHP DOM

Tracked against PHP's `ext/dom` / `ext/libxml` and against #654:

- No libxml2 or Lexbor engine, no `elephc-dom` crate, no XML `load()` / `loadXML()`.
- No modern `Dom\` HTML API (`Dom\HTMLDocument`, `Dom\Element`, …).
- No XPath, CSS selectors, DTD/schema validation, XInclude, C14N, or token lists.
- No tree mutation (`appendChild`, `removeChild`, `setAttribute`, `createElement`, …).
- No `DOMAttr` / `DOMNamedNodeMap`, processing instructions, CDATA, entities, or notations.
- No `LIBXML_*` constants beyond the five Termwind flags.
- No SimpleXML and no DOM ↔ SimpleXML import.
- `loadHTMLFile()`, encoding detection, error collection (`libxml_use_internal_errors`), and default `<head>` insertion are unimplemented.
- Specialized Termwind renderers that need richer markup (`<table>`, `<code>`, `<pre>` via `getHtml()` / `saveXML` of mixed subtrees) are not the goal of this subset; `saveXML` covers a simple element so those paths can be added later.
- The class surface is AOT-prelude only. `eval()` sees the `LIBXML_*` constants but not the DOM classes until #654 or a Magician binding lands.

Do not vendor Termwind itself. Compile programs that already depend on Termwind
against this subset; keep Termwind's sources in the application tree.
