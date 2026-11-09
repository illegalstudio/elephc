# DOM and libxml php-src compliance

Goal: `ext/dom` and `ext/libxml` behave exactly as PHP 8.5.8 (libxml2 2.15.3, Lexbor 2.7.0).
The php-src tests are the authority. Beads epic `elephc-8q7`; branch
`feat/dom-php-src-compliance`, started from `origin/main` 145426b7fe.

## Rules

- No PHP source text is parsed outside tests: the DOM surface is built as `synthetic_class`
  AST, like SimpleXML (#2269).
- The php-src tests are Elephc tests: `tests/codegen/dom_php_src/<topic>.rs` and
  `tests/codegen/libxml_php_src/cases.rs`, one Rust test per php-src test, program and
  expected output kept verbatim. Fixture assets live in `tests/fixtures/{dom,libxml}_php_src/`.
  There is no `.phpt` copy and no committed generator: the tests are maintained by hand.
- A test Elephc does not pass carries `#[ignore = "<missing behaviour>"]`; removing the ignore
  is how progress is recorded. php-src's own environment skips keep their `SKIPIF` message.
- Fixed divergences get a focused regression test besides the php-src test.

## Done

- [x] Pinned oracle rebuilt at `~/PhpstormProjects/oss/php-dom-oracle` (PHP 8.5.8 + static
  libxml2 2.15.3, `php -n` reports `8.5.8|2.15.3|21503`).
- [x] 868 DOM + 32 libxml tests migrated; shared byte-exact runner in
  `tests/codegen/php_src_corpus.rs` (EXPECTF on raw bytes, encoding fixtures included).
- [x] Lexer: case-insensitive `<?php`, `<?=`, short `<?`, `?>` as `;` swallowing one newline,
  inline HTML output, `//`/`#` comments ending before `?>`, CLI `#!` line (`elephc-8q7.7`).
- [x] Resolver: `include dirname(__FILE__) . '...'` folds when the call can only reach the
  global `dirname()` (namespace and `use function` guard) (`elephc-8q7.8`).

Baseline on main after these fixes: 14 pass, 866 ignored with a reason, 20 php-src skips.

## Architecture of the port

The previous attempt (`feat/php-dom-compliance`, 994ee1b5cd) exposed DOM as native compiler
classes with a 12k-line C engine; it is 4.8k commits behind and is a behavioural reference
only. The port follows SimpleXML instead:

1. Engine: `crates/elephc-xml/src/dom/`, Rust over libxml2 through the existing opaque shim
   (`src/native_deps/recipes/libxml2_shim.c` owns every struct field access). Documents are
   `Rc`-owned; nodes are pinned while a PHP wrapper references them.
2. Surface: a DOM prelude built as AST over an `extern "elephc_xml"` block, replacing
   `src/dom_html_prelude` (whose AST conversion is pending on `feat/dom-prelude-synthetic-ast`).
   PHP node identity (`$a->firstChild === $a->firstChild`) needs a node-handle to wrapper
   cache.
3. The modern `Dom\` namespace (HTML5 via Lexbor, CSS selectors, TokenList) comes after the
   legacy API.

### Legacy engine design (bead `elephc-8q7.10`)

- `crates/elephc-xml/src/dom/`: a thread-local registry maps each libxml2 node to ONE stable
  `i64` handle (so the prelude can cache one wrapper per handle and `===` holds). A document
  is `Rc`-owned by every handle into it; nodes unlinked by `removeChild()` and never re-linked
  are freed with their document.
- Entry points `elephc_xml_dom_*`: create/load (`loadXML`, `load`) and save (`saveXML`) with
  php-src's diagnostics, node accessors (type, name, value, text content, parent, children,
  siblings, owner document, document element), attributes, `create*`, `appendChild`,
  `insertBefore`, `removeChild`, `replaceChild`, `cloneNode`, `getElementsByTagName(NS)`.
  Rust calls libxml2's public API directly; the shim only grows for struct-field reads.
- Prelude: the `DOM*` classes as AST (PHP form under `cfg(test)` as the transcription oracle),
  properties as `get` hooks over the handle, `DOMNodeList`/`DOMNamedNodeMap` live over a
  (handle, query) pair, `DOMException` with php-src's codes and messages. It replaces the
  pure-PHP termwind prelude in `src/dom_html_prelude`, whose 125 `dom_html*` tests must stay
  green (`loadHTML` then goes through libxml2's HTML parser, as in php-src).

## Remaining, by blocker volume (ignore reasons)

- [ ] Legacy core: `DOMDocument` load/save/create*, node tree properties and mutation,
  `DOMElement`/`DOMAttr`/character data, `DOMException`, `libxml_*` errors.
- [ ] `DOMXPath` (query, evaluate, namespaces, PHP callbacks).
- [ ] HTML (`loadHTML`, `saveHTML`), validation (DTD, XSD, RelaxNG), XInclude, C14N.
- [ ] `Dom\XMLDocument`, `Dom\HTMLDocument` (149 tests name it first), `Dom\ParentNode`,
  CSS selectors, token lists.
- [ ] Generic gaps surfaced by the tests: `array_intersect_key()` on a mixed first argument
  (20), property access on untyped results (30), `assert()` (6).
- [ ] The five `server.inc` tests need php-src's test HTTP server fixture.
