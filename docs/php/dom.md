---
title: "DOM"
description: "The classic DOM extension: DOMDocument, DOMNode, DOMElement and friends, loadXML/loadHTML, saveXML/saveHTML, and the libxml constants."
sidebar:
  order: 25
---

elephc provides PHP's classic DOM extension (`DOMDocument`, `DOMNode`, `DOMElement`,
`DOMText`, `DOMComment`, `DOMCdataSection`, `DOMAttr`, `DOMDocumentFragment`,
`DOMDocumentType`, `DOMProcessingInstruction`, `DOMNodeList`, `DOMNamedNodeMap`,
`DOMException`) as ordinary compiled code. It is **pay-for-use**: a program that never names
a DOM class or a `libxml_*` function carries none of it.

The surface was checked against PHP 8.5 with libxml2 2.9.13. Tree building and mutation was
compared with a randomized operation fuzzer (serialized trees, the exceptions raised and their
order); `loadHTML()` and `saveHTML()` were compared on some 10,000 generated documents (parsed
trees, `saveHTML()` and `saveXML()` bytes, with and without `formatOutput`).

```php
<?php
$dom = new DOMDocument('1.0', 'UTF-8');
$dom->appendChild($routes = $dom->createElement('routes'));
$routes->appendChild($route = $dom->createElement('route'));
$route->setAttribute('name', 'home');
$route->appendChild(new DOMText('/'));
$dom->formatOutput = true;
echo $dom->saveXML();
```

## What is supported

- **Tree**: `appendChild`, `insertBefore`, `removeChild`, `replaceChild`, `cloneNode`,
  `importNode`, `adoptNode`, `normalize`, `hasChildNodes`, `firstChild`, `lastChild`,
  `previousSibling`, `nextSibling`, `parentNode`, `ownerDocument`, `documentElement`,
  `childNodes` (iterable, countable, `length`, `item()`), `nodeName`, `nodeType`, `nodeValue`,
  `textContent`, `firstElementChild` and the other element-sibling properties, `contains`,
  `getNodePath`, and the ParentNode / ChildNode methods (`append`, `prepend`, `before`,
  `after`, `remove`, `replaceWith`, `replaceChildren`).
- **Elements and attributes**: `tagName`, `getAttribute`, `setAttribute` (scalars are coerced to
  strings), `hasAttribute`, `removeAttribute`, the `*Node` and `*NS` variants, `attributes`
  (`DOMNamedNodeMap`: iterable, `length`, `item()`, `getNamedItem()`),
  `getElementsByTagName`, `getElementsByTagNameNS`.
- **Creation**: `createElement`, `createElementNS`, `createTextNode`, `createComment`,
  `createCDATASection`, `createDocumentFragment`, `createAttribute`,
  `createProcessingInstruction`, and the constructors `new DOMText($data)`,
  `new DOMElement($name, $value)`, ...
- **Character data**: `data`, `length`, `substringData`, `appendData`, `insertData`,
  `deleteData`, `replaceData`, `splitText`, `wholeText`.
- **Serialization**: `saveXML(?DOMNode)`, `saveHTML(?DOMNode)`, `save()`, `saveHTMLFile()`,
  `formatOutput`, the XML declaration (version, encoding, standalone), escaping, namespace
  declarations, XHTML empty-element rules, `LIBXML_NOXMLDECL` and `LIBXML_NOEMPTYTAG`.
- **Parsing**: `loadXML()` / `load()` for well-formed documents (namespaces, entities,
  character references, CDATA, comments, processing instructions, a `DOCTYPE`,
  `LIBXML_NOBLANKS`, `LIBXML_NOCDATA`); a malformed document returns `false`.
  `loadHTML()` / `loadHTMLFile()` follow libxml2's **HTML 4** parser, not an HTML5 one: implied
  `html`/`head`/`body`, implied paragraphs, the auto-close table, end-tag priorities, `script`
  and `style` text as CDATA nodes, the default HTML 4.0 Transitional doctype,
  `LIBXML_HTML_NOIMPLIED`, `LIBXML_HTML_NODEFDTD`, and `LIBXML_NOBLANKS`. Input is read as
  ISO-8859-1 unless it starts with a `<?xml ... ?>` prolog or has a `<meta charset>` naming
  UTF-8 (the `'<?xml encoding="UTF-8">' . $html` idiom works).
- **Constants and functions**: the `LIBXML_*` constants, `XML_*_NODE`, `DOM_*_ERR`,
  `libxml_use_internal_errors()` (state only), `libxml_clear_errors()`,
  `libxml_get_errors()` (always an empty list of `LibXMLError`, php's typed error record), `libxml_disable_entity_loader()`.

`Dom\HTMLDocument` (PHP 8.4's HTML5 DOM) is a separate surface with its own parser; the two do
not share nodes.

## Differences from PHP

- **No `DOMXPath`, `DOMImplementation`, `DOMEntityReference`, schema or DTD validation**, and no
  entity declarations inside a `DOCTYPE`.
- **No warnings**. PHP prints libxml diagnostics (`Tag section invalid in Entity`, ...) for
  malformed HTML and `createElement()` values; elephc prints none, and
  `libxml_get_errors()` is always empty.
- **Properties are real slots**, not engine accessors. `firstChild`, `parentNode`, `nextSibling`
  and the like are relinked on every mutation, and `textContent` / `nodeValue` are refreshed up
  the ancestor chain. Assigning `textContent` or `nodeValue` straight to an *element* is applied
  lazily, at the next `saveXML()` / `saveHTML()` / `cloneNode()`; assigning them (or `data`) to
  character data, an attribute or a processing instruction is immediate, because those names
  share one storage cell.
- **Computed properties** (`firstElementChild`, `lastElementChild`, `childElementCount`,
  `previousElementSibling`, `nextElementSibling`, `DOMCharacterData::$length`,
  `DOMText::$wholeText`) are read through magic accessors, which the compiler resolves only on a
  receiver whose class it knows; read them on a typed variable or after an `instanceof` check,
  not on an untyped `foreach` variable.
- **Lists are snapshots**: the `DOMNodeList` of `getElementsByTagName()` is not live, and
  iterating `childNodes` while removing nodes visits the original list. `$node->childNodes` is
  live for `length`, `item()` and `count()`.
- **Return types**: `appendChild()`, `insertBefore()` and `replaceChild()` return the node in the
  cases where PHP returns `false` (a parent that cannot have children, an empty fragment), and
  `importNode()` of a document throws `DOMException` where PHP warns and returns `false`.
- **Memory**: a node is a plain object of roughly 1.4 KB (2 KB for an element), and a tree is a
  web of references that reference counting alone never frees. `new DOMDocument` runs the cycle
  collector every eighth time so dropped documents are reclaimed; a program that builds very
  large documents should raise `--heap-size`.
