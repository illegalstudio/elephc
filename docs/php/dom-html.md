---
title: "DOM HTML (Termwind subset)"
description: "A pay-for-use DOMDocument HTML fragment walker for Termwind-style tree walks, not full PHP DOM."
sidebar:
  order: 24
---

elephc injects a small **HTML-only DOM subset** when a program names `DOMDocument`,
`DOMNode`, `DOMElement`, `DOMText`, `DOMComment`, `DOMCharacterData`, `DOMProcessingInstruction`, or
`DOMNodeList` through the global prelude surface, including references inside
Composer-autoloaded application classes. References bound to user-defined
classes, including namespace and import aliases, keep their own definitions and
do not trigger conflicting prelude declarations. The surface is enough for Termwind's `HtmlRenderer::parse` to
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
| `loadHTML(string $source, int $options = 0): bool` | Forgiving HTML fragment parse. Wraps element/text content in `html`/`body`; unmatched closing tags are ignored, and a matching ancestor closes its open descendants. Text and attributes decode HTML4 named entities, `&apos;`, and numeric references. Script/style content remains raw until its matching closing tag. This is not full HTML tree-repair semantics. |
| `getElementsByTagName(string $name): DOMNodeList` | Document-order descendant elements. Names are compared literally after HTML parse lowercases tag names. `*` matches every element. |
| `DOMNodeList::item(int $index)` / `$length` / `count()` / `foreach` | Indexed access, `Countable` counts, and independent `IteratorAggregate` traversals, including nested walks and descendant queries during a walk. |
| `nodeName`, `nodeValue` | HTML tag names are lowercased. Element `nodeValue` is concatenated descendant text, excluding comments and processing instructions. Document `nodeValue` remains `null`. A `DOMComment` retains its own comment body and serializes normally. |
| `childNodes`, `previousSibling`, `nextSibling`, `parentNode`, `ownerDocument` | Wired after parse. The document's `ownerDocument` is `null`; child nodes refer to their document. The tree is treated as immutable. |
| `DOMElement::getAttribute(string $name): string` | Attribute names are lowercased on parse; duplicate names keep their first value. Queries are compared literally, so `class` matches parsed `CLASS`, but a query for `CLASS` does not. Missing attributes return `""`. |
| `instanceof DOMElement` / `DOMText` / `DOMComment` / `DOMDocument` | Class hierarchy matches PHP (`DOMText`/`DOMComment` extend `DOMCharacterData` extend `DOMNode`). |
| `DOMProcessingInstruction` | Extends `DOMNode`; parsed instructions preserve target case, `target`, `data`, `nodeName`, `nodeValue`, and ordinary node links. Serialization emits `<?target data?>`. Legacy HTML consumes through `>`, retaining a preceding `?` in the data. |
| `saveXML(?DOMNode $node = null): string` | Serializes a node or the whole document. Text escapes `&`, `<`, `>`, and CR; double-quoted attributes also escape `"`, LF, and TAB, but not `'`. Non-ASCII attribute code points use uppercase hexadecimal references, while text stays UTF-8. Script/style raw text serializes as safely split CDATA. No XML declaration (Termwind passes `LIBXML_NOXMLDECL`). |

Leading comments and processing instructions remain document-level siblings before
`html`. After an explicit `</html>`, trailing comments and instructions remain
document-level siblings after it; trailing whitespace is ignored without adding
another `html` or `body`. Comment-only and instruction-only sources do not create `html` or `body`.
Ignored closing tags do not split adjacent text nodes. An unfinished comment and
its remaining input are discarded, following the legacy libxml HTML tokenizer.

An explicit `</body>` is honored: in `<body>A</body><div>B</div>`, the `div` is
a sibling of `body`, whose child walk contains only `A`. Extra content after an
explicit `</html>` follows the legacy PHP/libxml 2.9.14 fallback: it reopens an
`html` child without adding another `body` when one already exists. If the
original document contained only a head, the reopened content receives its first
body. Immediately trailing comments
and instructions stay after the outer `html`. This is not a promise to preserve
source order for malformed whole documents; newer libxml tree-repair rules can differ.

When an explicit `html` has no `body`, its `head` and leading wrapper nodes stay
outside the implied body. Head-compatible metadata (`base`, `link`, `meta`,
`title`, `script`, and `style`) also stays outside until body content begins.
Content following that closed `html` is attached only
after the original body is built; an explicit reopened `html` is not wrapped in
another `html`. A repeated `html` start tag while the original is still open is
ignored, including its attributes. A head-only document does not gain an empty
body. Legacy void elements include `frame` and `isindex` but exclude `embed`,
`source`, `track`, and `wbr`, which can retain children
under the libxml 2.9.14 compatibility behavior used here.

Ordinary content creates an implied body that an explicit `</body>` can close.
While a body is open, an extra non-self-closing `body` and its paired structural
end tag are ignored, including redundant attributes. Its contents stay in the
existing body. An extra self-closing `<body/>` closes the current top element
immediately (which can be a descendant of the body), then consumes the next
`body`, `html`, or `head` end tag. Separate explicit bodies after a closed body
remain separate and retain their attributes. The legacy `frame`, `frameset`,
and `noframes` tags do not themselves imply a body; a later explicit body is
kept, including when nested inside `frameset` or `noframes`. This follows the
libxml 2.9.14 subset, not newer `noframes` CDATA or HTML5 tree repair.

Only an immediate `/>` self-closes a start tag. A slash beginning a malformed
attribute consumes that run through its next whitespace or closing `>`; slashes
inside unquoted attribute values remain part of the value.

### `LIBXML_*` flags Termwind uses

These constants are always available (including inside a namespace, and in
`eval()`), with php-src's integer values:

| Constant | Value | Effect here |
|---|---|---|
| `LIBXML_NOXMLDECL` | 2 | Save flag; parse ignores it. `saveXML()` never emits `<?xml …?>`. |
| `LIBXML_HTML_NODEFDTD` | 4 | No default doctype is added (none is added anyway). |
| `LIBXML_NOERROR` | 32 | Parse is silent and forgiving. |
| `LIBXML_NOBLANKS` | 256 | Drops trailing document-level whitespace padding and whitespace-only text that remains directly under `html` or `head` after body insertion. Whitespace moved into an implied body is retained, including between body siblings and after the last child. Whitespace inside `body`, `title`, and other elements remains. Without this flag, wrapper whitespace and trailing fragment padding are preserved; padding after an explicit `</html>` is always ignored. XML blank-node filtering is outside this HTML-only subset. |
| `LIBXML_COMPACT` | 65536 | Accepted and ignored (libxml compaction is an optimizer hint). |

## Remaining gaps versus full PHP DOM

Tracked against PHP's `ext/dom` / `ext/libxml` and against #654:

Unknown `<!...>` markup stays text, while a doctype is ignored. This follows
the legacy libxml HTML tokenizer used by PHP with libxml 2.9.14; newer libxml
versions can tokenize malformed declarations differently.

Processing instructions follow that same legacy HTML tokenizer. For example,
`<div>a<?php echo 1; ?>b</div>` has three children and element `nodeValue` `ab`;
`saveXML()` emits `<div>a<?php echo 1; ??>b</div>`. At end of input a bare
unterminated target such as `<?foo` becomes text, while an incomplete instruction
with data is discarded.

- No libxml2 or Lexbor engine, no `elephc-dom` crate, no XML `load()` / `loadXML()`.
- No modern `Dom\` HTML API (`Dom\HTMLDocument`, `Dom\Element`, …).
- No XPath, CSS selectors, DTD/schema validation, XInclude, C14N, or token lists.
- No tree mutation (`appendChild`, `removeChild`, `setAttribute`, `createElement`, …).
- No `DOMAttr` / `DOMNamedNodeMap`, `DOMCdataSection` / entity-node APIs, or notations. HTML entity decoding and script/style CDATA serialization are supported internally.
- No `LIBXML_*` constants beyond the five Termwind flags.
- No SimpleXML and no DOM ↔ SimpleXML import.
- `loadHTMLFile()`, encoding detection, error collection (`libxml_use_internal_errors`), and default `<head>` insertion are unimplemented.
- Specialized Termwind renderers that need richer markup (`<table>`, `<code>`, `<pre>` via `getHtml()` / `saveXML` of mixed subtrees) are not the goal of this subset; `saveXML` covers a simple element so those paths can be added later.
- Opaque `eval()` source alone does not trigger DOM injection. Once static program references load the prelude, `eval()` can discover, construct, and query the registered DOM classes through native dispatch. This remains a limited HTML subset, not full PHP DOM.

Do not vendor Termwind itself. Compile programs that already depend on Termwind
against this subset; keep Termwind's sources in the application tree.

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

Classes: `DOMCharacterData`, `DOMComment`, `DOMDocument`, `DOMElement`, `DOMNode`, `DOMNodeList`, `DOMProcessingInstruction`, `DOMText`.

<!-- elephc:generated:symbols:end -->
