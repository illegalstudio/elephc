---
title: "DOM after the merge: main's libxml2-bound design is the base"
author: Guillaume
date: 2026-11-18
branch: merge-main
cost: { minutes: 112, tokens: 328607 }
touches:
  - src/dom_html_prelude.rs
  - src/dom_html_prelude/extend.rs
  - src/dom_html_prelude/detect.rs
  - src/dom_html_prelude/generated.rs
---

## Decided
- User decision: keep main's DOM (synthetic-AST src/dom_html_prelude, nodes bound to a native libxml2 tree shared with
  SimpleXML, node identity cache) and port the branch's classic DOM (~200 methods; already AST in the branch's
  src/dom_prelude/build.rs, PHP model only as a test oracle) onto it in packages P1–P9 (plan:
  .elephc-work/merge-batches/dom-ast-plan.md, decisions dom-decisions.md).
- P1 extension API (src/dom_html_prelude/extend.rs): each package owns one file; `members` merges onto main's generated
  classes and panics on a duplicate member; parts Walker/Html/Libxml/XPath gate injection so loadXML-only programs do
  not pay for the HTML parser.
- loadHTML keeps php's libxml 2.9.13 HTML4 semantics through an AST port of the branch parser (eager tree): the bundled
  native libxml2 2.15.3 differs (no <p> wrap of text, CDATA becomes a comment).

## Tried and ruled out
- Native htmlReadMemory for loadHTML: verified with xmllint 2.15.3 vs php -n, outputs differ.
- Materializing eager slots for bound nodes: breaks simplexml_import_dom sharing; mutations go through native bridge
  helpers (crates/elephc-xml/src/simplexml/dom_tree.rs, dom_write.rs) instead.

## Watch out
- Never rename `__elephc_dom_*` members or the node cache: SimpleXML (php-src corpus) depends on them.
- Free-standing parent/child trees need weak back-links (parentNode/ownerDocument as WeakReference) or they leak.
