# PHP `ext/xml` and `ext/xmlwriter` parity

- [x] Add the `elephc-xml` bridge crate: an FFI engine over libxml2's push parser and `xmlwriter`, reached through the Elephc-owned C shim, with a panic-free C ABI (`abi.rs`).
- [x] Add the `libxml2` 2.15.3 catalog package (recipe + `libxml2_shim.c`), splice it into the link whenever `elephc_xml` is planned, and wire the harness/CI/release probe to materialize it.
- [x] Register the bridge (`BRIDGES`, `--with-xml`, workspace members, nextest archive, CI/nightly/release packing, Linux test scripts).
- [x] Add the shared contracts: the 22 `xml_*` + 42 `xmlwriter_*` functions (`catalog_xml.rs`; 54 prelude-provided, 10 registry builtins), the `XMLParser` / `XMLWriter` classes (`Prelude` route), and the 28 `XML_*` constants.
- [x] Build the `xml_prelude` (Rust AST builders transcribed from the PHP form, kept as a parse-parity oracle): `XMLParser`, `XMLWriter`, every procedural function, the `extern "elephc_xml"` block, detection, pipeline + test-harness injection.
- [x] Magician: `xml` eval area whose homes forward to the compiled prelude through the native-function bridge, `extension_loaded()` parity, eval tests.
- [x] Tests: crate unit tests, `tests/codegen/xml/`, error tests, eval parity, symbol/class/constant probes, PHP cross-check corpus.
- [x] Example (`examples/xml/`), generated builtin docs, `docs/php/xml.md`, compatibility catalog, CLI/linking docs, README.

## Scope and authoritative baseline

Everything the compatibility page attributes to the `xml` and `xmlwriter` PHP modules in the
PHP 8.5.10 baseline: 64 functions, `XMLParser` and `XMLWriter`, and the 28 `XML_*` constants.
Behavior is pinned against the local PHP 8.5.10 CLI (libxml2 2.15.3) with the php-src 8.5
sources of `ext/xml` (`xml.c`, `compat.c`) and `ext/xmlwriter` as the reference, on both AOT
and `eval()`.

## Architecture

### Bridge crate

`crates/elephc-xml` is a `staticlib` + `rlib` whose engine is libxml2 2.15.3 itself, exactly
like php-src: the crate never re-implements parsing or writing. libxml2 is not a Cargo
dependency and never a system library; it is the managed native catalog package `libxml2`
(`src/native_deps/recipes/libxml2.rs`, `elephc native add libxml2`), built from the pinned
tarball and archived as `lib/libxml2.a` next to `lib/libelephc_libxml2_shim.a` — the
Elephc-owned C shim (`src/native_deps/recipes/libxml2_shim.c`, compiled by the recipe against
the freshly built headers, like `pcre2_shim.c`) that exposes versioned
`elephc_libxml2_v1_*` entry points over opaque pointers and plain integers, so the Rust side
depends on the shim's ABI rather than on libxml2's struct layouts.

- The parser side drives libxml2's push parser (`xmlCreatePushParserCtxt` / `xmlParseChunk`)
  through a SAX handler table the shim installs, mirroring php-src `ext/xml/compat.c`: the
  shim refuses every external resource load, reports positions, error numbers
  (`xmlParserErrors`, verbatim through `xml_get_error_code()`), entity lookups and the
  in-subset / in-content state PHP's default-handler routing needs.
- The writer side is libxml2's own `xmlwriter` (`xmlNewTextWriter` over an
  `xmlOutputBufferCreateIO` sink that appends to a Rust buffer, exactly how php-src's memory
  writer is built, plus the `xmlTextWriter*` family), so indentation, namespace declarations,
  DTD sections and escaping are libxml2's bytes; output encodings are whatever libxml2 (with
  iconv) knows.
- `abi.rs`: `elephc_xml_*` entry points over id-keyed registries. Strings cross as NUL-terminated
  C strings, with an explicit length for the parser input (PHP data may contain NUL bytes,
  which must surface as libxml2's "invalid character" error). Returned strings live in
  `thread_local!` cells (FFI buffer hygiene rule). Case folding and
  target-encoding conversion are applied in the ABI layer from the parser's options;
  `XML_OPTION_SKIP_TAGSTART` and `XML_OPTION_SKIP_WHITE` are stored there and applied by
  the prelude.
- `build.rs` reads `ELEPHC_XML_LIBXML2_LIB_DIR` (an artifact's `lib/`): when set it links the
  shim and libxml2 statically (plus `-liconv` on Apple targets) and enables
  `cfg(elephc_xml_native)`, which compiles the libxml2-calling unit tests in; without it the
  staticlib still builds — its libxml2 symbols stay unresolved until a PHP program links —
  and the tests take a skip path.
- Pay-for-use, like curl: `src/pipeline/backend.rs` adds `NativeRequirement::package("libxml2")`
  whenever `elephc_xml` is planned (auto-detected xml use or `--with-xml`), so the final link
  resolves `libelephc_libxml2_shim.a -> libxml2.a` from the project's lock with no system
  `-lxml2` fallback; the bridge table marks `iconv` as an Apple-only system library. The codegen
  harness (`tests/codegen/support/xml_native.rs`) discovers the same artifact structurally,
  skips fixtures without it, and fails them under `ELEPHC_TEST_REQUIRE_XML_NATIVE=1` (every CI
  shard, after `elephc native install --locked --manifest-path examples/xml/elephc.toml`).

### Compiler surface

The PHP surface is an injected prelude (`src/xml_prelude/`), built from Rust AST builders that
were transcribed from a PHP form kept under `cfg(test)` as a node-by-node oracle:

- `XMLParser` (final; the constructor throws PHP's "Cannot directly construct XMLParser" error)
  holds the bridge handle plus the ten handler slots, the `xml_set_object()` target, the
  `parse_into_struct` accumulation state and the "is parsing" flag; `__destruct` frees the
  bridge parser. `xml_parse()` feeds the chunk and drains events, dispatching each to the
  handlers with the php-src rules (default handler fallbacks, entity handling, `is_final`).
- `XMLWriter` holds the bridge writer handle and, for `openUri()` / `toStream()`, the PHP
  stream it flushes into. Memory mode returns strings, stream mode returns byte counts.
- Every procedural function is a thin wrapper over the class methods, so the AOT path and the
  eval path execute the same code. Ten of them are registry builtins rather than prelude
  declarations because they need the checker: `xml_parse_into_struct()` writes two
  by-reference outputs that may name undefined variables, and the nine `xml_set_*_handler()`
  setters type an unannotated handler closure's parameters from the SAX event it receives
  (a statically named function or `[$object, 'method']` handler is specialized the same
  way). Their EIR lowering is one call to the prelude's `__elephc_xml_*` twin.
- Calls into the bridge are `extern "elephc_xml"` declarations inside the prelude; declaring
  them is what links `libelephc_xml.a`, and `--with-xml` forces the injection.

### Eval

Magician never re-implements the surface. Prelude functions are registered into the eval
context as ordinary host functions, and the compiled `XMLParser` / `XMLWriter` classes are
reached through the native class bridge (`new_object` / `method_call`), so `eval()` code sees
real objects and the same behavior. The `xml` eval area exists so the parity audit counts the
64 names: each home forwards to the host function (positional, named and by-reference
arguments through `eval_native_function`) and reports PHP's undefined-function error when the
host program did not link the bridge. The ten registry builtins have no host function, so
eval calls the `XMLParser` object's `__elephc_*` methods through the native class bridge and
writes `xml_parse_into_struct()`'s outputs back through the captured caller lvalues.
Handlers installed from eval must be callables the compiled parser can invoke (compiled
functions and methods); an interpreted closure is not.

## Verification

Crate unit tests replay the PHP probe corpus (events with positions, error codes, writer
output). Codegen tests cover every function on AOT and inside `eval()`, error tests cover
arity/type diagnostics, and the builtin parity, symbol catalog, backend support and generated
docs gates must stay green.
