---
title: "libxml error list, LIBXML_NOERROR routing, trim view leak"
author: Guillaume
date: 2026-11-09
branch: feat/dom-php-src-compliance
touches:
  - crates/elephc-xml/src/libxml_errors.rs
  - src/native_deps/recipes/libxml2_simplexml.c
  - src/xml_prelude/build/libxml.rs
  - src/ir_lower/expr/function_calls.rs
  - crates/elephc-magician/src/interpreter/builtins/xml/dispatch.rs
---

## Decided
- PHP's libxml error state (php-src ext/libxml/libxml.c) lives in crates/elephc-xml/src/libxml_errors.rs: the list exists only while libxml_use_internal_errors(true); disabling destroys it; libxml_get_last_error() reads the list's last entry, else libxml2's last error; libxml_clear_errors() resets both. LibXMLError properties follow the stub order (level, code, column, message, file, line); message keeps libxml2's trailing newline.
- The shim (libxml2_simplexml.c) records every xmlError as "level\x1fcode\x1fcolumn\x1fline\x1ffile\x1fmessage\x1e" in a thread buffer drained by simplexml diagnostics::capture; with internal errors on, the operation's warning text is dropped.
- libxml2 recipe revision 12 (r11 shim + error recorder); catalog, recipe.rs dispatcher and example locks move together.
- trim/ltrim/rtrim/chop/substr results are persisted (StrPersist) whenever argument 0 is any owning temporary, not only eval pins / borrowed / mixed casts: MayAliasArguments suppresses that operand's release, so an acquired property read, array element or call result leaked once per call.

## Tried and ruled out
- Filtering LIBXML_NOERROR/NOWARNING errors out of the list: php 8.5.8 still lists them. libxml2 skips the parser's own handler for them and raises them on the thread's global structured handler, so the shim installs a global recorder before each parse.
- A `__elephc_libxml_*` helper function in the xml prelude: xml_prelude's oracle requires every `__elephc_*` function to be a registry-builtin twin; build the shared statements with a Rust helper instead.

## Watch out
- Every new shared contract needs a Magician eval binding (one-line eval_builtin! file under crates/elephc-magician/src/interpreter/builtins/xml/) or EVERY eval-linked program panics at startup ("requires an eval registry binding") — 338 tests failed that way.
- eval_xml_builtin_name routes by prefix (xml_, xmlwriter_, simplexml_, libxml_); a new prefix needs adding there.
- gen_php_comparison compares against a Homebrew PHP linked to libxml2 2.9.14: LIBXML_VERSION is a recorded divergence (elephc reports the 2.15.3 it links).
- An undefined constant inside an XML-injected program compiled silently to 0 instead of php's Error (bead elephc-8q7.15).
