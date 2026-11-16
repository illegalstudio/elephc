---
title: "Dom\\HTML prelude moves to synthetic AST"
author: Guillaume
date: 2026-11-08
branch: feat/dom-prelude-synthetic-ast
touches:
  - src/dom_html_prelude.rs
  - src/dom_html_prelude/generated.rs
  - src/synthetic_class.rs
  - src/synthetic_class/transcribe.rs
  - src/synthetic_class/print.rs
---

## Decided
- The Dom\HTML prelude is built as synthetic AST like SimpleXML (#2269). The five PHP
  fragments are transcribed with `transcribe_split`; entities/attributes/node_cache/text stay
  as their own builder modules (preambles preserved), and the surface moves to `generated.rs`.
  `surface.rs` is deleted; no PHP text is parsed at compile time.
- Every top-level declaration gets `Span::synthetic()`. `class_is_user_declared` compares the
  declaration span with `Span::dummy()`, so a dummy span silently disables PHP's weak scalar
  coercion (`new DOMDocument(1, 9000000000)`).
- `DOMDocument::loadXML()` reaches the optional SimpleXML loader through `("fn")(...)`, which is
  `ExprKind::ExprCall`. The builder/transcriber/printer and `reads_of` need it; printing the
  grouped callee is what reparses to the same node.
- The prelude's own HTML scanner helper is `__elephc_dom_token_stream`, so the issue's
  `rg "tokenize|parse_internal|SOURCE" src/dom_html_prelude*` check is literal. Tests use the
  test-only `parser::parse_test_source` helper instead of tokenizing inline.
## Watch out
- `transcribe_split` emits neither `use` lines nor module preambles; add both by hand.
- The AST printer ignored `PropertyHooks`; hook bodies are stored as `__propget_*`/`__propset_*`
  methods. Printing the plain property plus those methods reparses to a class with NO hook
  flags. The class printer now emits the `{ get … set … }` block and skips the accessors.
- `walk_expr` skips unknown expression nodes; without an `ExprCall` arm, `loadXML`'s
  `$source`/`$options` look unread and the built body drifts.
- Rebasing onto #2269 brought new DOM surface PHP (`appendChild`/`append`/`removeChild`,
  `createElement`, `load`, `__elephc_dom_html`, XML declaration prefixes) and the hand-built
  `Dom\XMLDocument` interop class. Re-transcribe the updated upstream surface rather than
  patching the old generated file; `@file_get_contents` also needed `ErrorSuppress` support.
- `Dom\XMLDocument` is a hand-built class whose fully-qualified name has no top-level PHP
  spelling, so the DOM print round-trip excludes it and a structural test covers it instead.

## Verified
- `printing_round_trips`, `parser_agreement`, the DOM prelude unit tests, all 125 `dom_html`
  codegen tests, and the 155-case SimpleXML php-src corpus pass.
