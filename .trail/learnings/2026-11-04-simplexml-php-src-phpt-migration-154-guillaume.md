---
title: "SimpleXML php-src PHPT migration: 154 pass / 2 upstream skips"
author: Guillaume
date: 2026-11-04
branch: feat/simplexml-compliance
cost: { minutes: 37, tokens: 181881 }
touches:
  - ".plans/simplexml.md"
  - crates/elephc-builtin-contract/src/catalog_classes.rs
  - crates/elephc-builtin-contract/src/catalog_constants.rs
  - crates/elephc-xml/src/simplexml/abi.rs
  - crates/elephc-xml/src/simplexml/ffi.rs
  - docs/php/xml.md
  - examples/simplexml/elephc.lock
  - src/builtins/array/internal_pointer.rs
  - src/codegen/lower_inst/array_access_runtime.rs
  - src/codegen/lower_inst/core_builtins/handlers.rs
---

## Decided
- The PHP 8.5.8 ext/simplexml/tests corpus is committed at tests/php_src/simplexml and replayed by scripts/gen_php_src_simplexml_cases.py -> tests/codegen/simplexml_php_src_cases.rs. Each case is an inline Rust #[test]; the runner never parses a .phpt.
- Object handles in EXPECT/EXPECTF are normalized (#N -> #%d); fatal fixtures compare only the fatal message line, because native trace frames are implementation detail.
- Warning producers now append " in FILE on line N" from the published _php_diagnostic_* globals; SimpleXML warnings route through __elephc_builtin_warning so handlers see the message without the location.
- $x[]->prop autovivification selects the appended element after the push (array_key_last for arrays, count-1 for countable objects). Arrays still throw PHP's on-null Error; SimpleXML continues on the new child view.
- libxml2 recipe revision 11 owns the empty-text (bar/ vs bar) and modern DOM encoding fixes.
- Dom\XMLDocument::createFromFile/createFromString is a synthetic class extending the DOM prelude's DOMDocument; it sets UTF-8 so saveXML matches modern DOM.
- Countable subclasses with a string-returning `count()` are routed through `_count_str_wrap_<id>`, which calls the implementation and then `__rt_str_to_int`; the dense `_class_count_ptrs` table accepts the string return for that interface.
- A by-reference foreach over a dynamic object source now raises PHP's catchable `Error: An iterator cannot be used with foreach by reference` instead of the compiler's unsupported-kind fatal.
- An explicit `__construct()` call on an `Object|false` factory view is unboxed to its concrete class and its trailing optional arguments are filled from the class signature. The receiver test uses an object-tag predicate, not truthiness: an empty SimpleXMLElement is falsy in PHP's boolean conversion but is still an object.
- The migrated corpus has a nextest override with five slow-timeout periods because one fixture can compile for almost three minutes.
- Read-context undefined variables now type as `null` and EIR lowering emits PHP 8's `Warning: Undefined variable $x` through `WriteDiagnostic` at the read span, so the shared dispatcher adds the caller location and `@` suppression applies.
- The uncaught SimpleXML initialization Error no longer hangs the native unwind path; `get_prop_address_not_initialized` now exits with the expected fatal instead of blocking.
- A by-reference SimpleXML property binding is resolved at run time: an `instanceof SimpleXMLElement` branch auto-vivifies the child through `__elephc_property_reference_fetch` and seeds an independent managed cell (`store_value_into_temp` + `promote_local_ref_cell`), while ordinary objects keep their shared property cell. Writes through the XML reference replace only that cell; the tree keeps its value.
- A top-level `$GLOBALS` read lowers to the initialized-locals projection (`lower_visible_defined_vars`); function scope projects only initialized `global $name` aliases and spelled superglobals. `array_merge_recursive` now projects colliding raw and boxed-Mixed objects to hashes through `__rt_object_to_hash(object, 1, -1)`, and unwraps boxed arrays, matching php_array_merge_recursive's convert_to_array on duplicate string keys.
- The DOM HTML prelude now owns `DOMDocument::createElement()`, `DOMNode::appendChild()/append()/removeChild()`, and a `__elephc_dom_html` document flag. `append()` on a node bound by `simplexml_import_dom()` appends text through `elephc_xml_sx_html_append`, so the DOM serializer and the SimpleXML view observe the same native tree. XML documents keep the declaration on `saveXML()` even when empty; HTML documents still omit it.
- Undefined-variable reads are decided by physical availability, not by the checker env: `has_local_slot(name)` or an entry-initialized name (`$argc`/`$argv` in main, request superglobals, extern globals) loads storage; anything else warns and yields null. Checking `local_types` alone broke `$argc` (seeded in the checker but not yet slotted) and read uninitialized storage for a name first assigned later in the body.
- User stream-wrapper opens assign the stream context RESOURCE before `stream_open`, as php-src does. A per-class `_class_native_xml` byte table and `__rt_simplexml_wrapper_failure` reproduce SimpleXML's chained TypeError/Error. `file_get_contents()` now probes registered wrappers through `__rt_fopen` on its plain-path fallback, so the same failure is reachable there.
- Covariant return validation now answers ancestry for a class that is still being built through `Checker::pending_class_parents`; a failed class build also removes itself from the `building` set so later siblings do not report a spurious circular-inheritance error. This lets `SimpleXMLIterator::getChildren(): ?SimpleXMLIterator` override `?SimpleXMLElement` and makes `SXETest::getChildren()` compile.
- `RecursiveIteratorIterator` now follows php-src's state machine: only `RS_START` probes the sub-iterator's `valid()`, `RS_TEST` calls `hasChildren()` once, `RS_SELF`/`RS_CHILD` call `getChildren()` without an extra `valid()`, and `RS_NEXT` advances before validating. The public `valid()` re-queries the active sub-iterator, as `spl_recursive_it_valid_ex` does.

## Tried and ruled out
- Keeping SimpleXMLElement::children()/getChildren() overrides for covariant return types: the self-referential return type breaks the prelude checker.
- Changing str_replace's contract to accept arrays: it poisons the DOM prelude's own str_replace calls. The checker accepts only a literal string-array search, and the AST optimizer folds it to scalar calls.
- Treating every EXPECTF #N as an exact handle: php-src reuses handles, so exact identity comparison is not portable.

## Watch out
- Remaining ignored fixtures: only php-src's own `ext/xsl` and `zend_test` environment skips. Every runnable SimpleXML fixture passes.
- Corpus fixtures use a 128 MiB heap (PHP's default memory budget) rather than the 8 MiB fixture default; bug42369 needs it.
- Regenerate cases after editing the corpus or generator; the generated file is checked in.
