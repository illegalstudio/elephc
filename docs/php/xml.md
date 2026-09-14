---
title: "XML"
description: "The ext/xml SAX parser (XMLParser, xml_* functions, XML_* constants) and ext/xmlwriter (XMLWriter, xmlwriter_* functions)."
sidebar:
  order: 24
---

elephc implements PHP's `ext/xml` (the expat-style SAX parser: `XMLParser`,
the 22 `xml_*` functions and the 28 `XML_*` constants) and `ext/xmlwriter`
(`XMLWriter` and the 42 `xmlwriter_*` functions) through the `elephc-xml`
bridge, whose parser and writer are libxml2 itself — the same library PHP
uses — pinned at 2.15.3 and supplied by the managed native catalog as the
`libxml2` package. The package is linked statically, so compiled programs are
still standalone binaries: the target machine needs neither PHP nor a system
libxml2. Both surfaces behave the same inside `eval()`, which runs the very
same compiled `XMLParser` / `XMLWriter` declarations as native code.

## Enabling xml

xml needs a native package, so it is opt-in at the project level even though
usage detection is automatic. Declare it once:

```bash
elephc native add libxml2
```

`libxml2` has no catalog dependencies of its own: the one `native add` builds
the pinned 2.15.3 sources plus the Elephc-owned C shim the bridge calls, and
archives both (`libelephc_libxml2_shim.a`, `libxml2.a`) in the cache. See
[Native dependencies](../compiling/native-dependencies.md) for the full `elephc
native` workflow (lock file, cache, `elephc native install`).

After that, an ordinary compile is enough — using any function or class of the
surface links the bridge and, with it, the managed package:

```bash
elephc feed.php
```

Use `--with-xml` when the surface is only reached at runtime, for example
through opaque dynamic `eval()`; it force-links the whole `elephc_xml` archive
and force-injects the `XMLParser` / `XMLWriter` prelude even when the compiler
sees no xml usage:

```bash
elephc --with-xml feed.php
```

Either way the project must declare `libxml2`: a compile that plans the bridge
without it fails closed with the `elephc native add libxml2` recovery, and there
is no fallback to a system `-lxml2`. On Apple targets the link also adds the
SDK's `libiconv` for libxml2's encoding handlers; on Linux, glibc provides
iconv from libc. On Linux a program that links the bridge also needs the zlib
and bzip2 development libraries at link time (`-lz -lbz2`, the runtime's
stream layer), exactly like any program that uses `fopen()`.

`extension_loaded('xml')` and `extension_loaded('xmlwriter')` report `true`
whenever the bridge is linked, natively and inside `eval()`. `function_exists()`
answers `true` for the ten registry builtins — `xml_parse_into_struct()` and
the nine `xml_set_*_handler()` setters — even in a program that never links
the bridge, and `false` for the prelude functions; both backends agree.

## Parsing with the SAX API

`xml_parser_create()` (or `xml_parser_create_ns()` for namespace-aware
parsing) returns a final `XMLParser` object; `new XMLParser()` throws PHP's
`Error`. Handlers are ordinary callables — closures, first-class callables,
`[$object, 'method']` arrays, or function names — and receive the parser as
their first argument, exactly as in PHP:

```php
$parser = xml_parser_create();
xml_parser_set_option($parser, XML_OPTION_CASE_FOLDING, false);

xml_set_element_handler(
    $parser,
    function (XMLParser $parser, string $name, array $attributes): void {
        echo "<{$name}> at line ", xml_get_current_line_number($parser), "\n";
    },
    function (XMLParser $parser, string $name): void {
        echo "</{$name}>\n";
    },
);
xml_set_character_data_handler($parser, function (XMLParser $parser, string $data): void {
    echo "text: ", trim($data), "\n";
});

$chunks = ["<doc><item id=\"1\">hel", "lo</item></doc>"];
foreach ($chunks as $i => $chunk) {
    if (xml_parse($parser, $chunk, $i === count($chunks) - 1) !== 1) {
        $code = xml_get_error_code($parser);
        printf("%s at %d:%d\n", xml_error_string($code),
            xml_get_current_line_number($parser), xml_get_current_column_number($parser));
    }
}
```

Every handler PHP offers is available: element, character data, processing
instruction, default, notation declaration, unparsed entity declaration,
external entity reference, and namespace declaration. `xml_set_object()` binds
an object so that string handler names resolve to its methods. The default
handler receives exactly what PHP's libxml-backed parser hands it: comments,
processing instructions and tags nobody else claimed, and unexpanded entity
references when a character-data handler is not installed.

A closure written directly in a `xml_set_*_handler()` call may leave its
parameters unannotated: elephc types them from the event the handler receives,
and a bare `array` annotation on the attribute parameter is accepted. Declaring
a variadic parameter is a compile error. A named function passed by its literal
name gets the same treatment, so an unannotated
`function on_element($parser, $name, $attributes)` also compiles. A handler that
is chosen at run time (a method name resolved through `xml_set_object()`, a
name held in a variable, a name passed from `eval()`) should declare its
parameter types, because elephc infers the parameters of a function that is
only ever invoked by name from its call sites, and such a callback has none.
Declare the attribute map as `mixed` in that case: a bare `array` declaration on
a handler that is invoked dynamically does not yet receive the map correctly
(see [Runtime limits](#runtime-limits) for what each choice costs).

Incremental parsing works chunk by chunk with `xml_parse($parser, $chunk,
false)`. Events are dispatched as soon as the buffered input completes an item,
which is also when `xml_get_current_line_number()`,
`xml_get_current_column_number()` and `xml_get_current_byte_index()` update.

### Error reporting

`xml_get_error_code()` returns the libxml2 error number PHP itself reports
(`76` for a mismatched tag, `5` for content after the root element, and so on),
and `xml_error_string()` produces PHP's message table for it. The classic
expat-numbered `XML_ERROR_*` constants are defined with PHP's values.

### Options

| Option | Behavior |
|---|---|
| `XML_OPTION_CASE_FOLDING` | Uppercase element and attribute names (on by default) |
| `XML_OPTION_TARGET_ENCODING` | `UTF-8`, `ISO-8859-1` or `US-ASCII`; unrepresentable characters become `?` |
| `XML_OPTION_SKIP_TAGSTART` | Drop that many leading characters from tag names |
| `XML_OPTION_SKIP_WHITE` | Skip whitespace-only text in `xml_parse_into_struct()` |
| `XML_OPTION_PARSE_HUGE` | Lift libxml2's 10 MB text-node limit |

Input decoding is libxml2's: a document may declare any encoding libxml2 (with
iconv) understands, and one without a declaration is read as UTF-8, exactly as
in PHP. The `$encoding` argument of `xml_parser_create()` selects the *target*
encoding handed to handlers, which PHP restricts to the three values above.

### `xml_parse_into_struct()`

```php
$parser = xml_parser_create();
xml_parse_into_struct($parser, $xml, $values, $index);
foreach ($values as $entry) {
    echo str_repeat(' ', $entry['level']), $entry['tag'], ' ', $entry['type'], "\n";
}
```

`$values` and `$index` need not exist before the call. Both come back as
`mixed` arrays with PHP's exact layout: one entry per open/complete/close/cdata
event with `tag`, `type`, `level`, optional `attributes` and `value`, and an
index of positions per tag name.

## Writing with XMLWriter

`XMLWriter` is an ordinary, extensible class; `new XMLWriter()`,
`XMLWriter::toMemory()`, `XMLWriter::toUri()` and `XMLWriter::toStream()` all
work, as do the `xmlwriter_open_memory()` / `xmlwriter_open_uri()` functions.
Output is byte-identical to PHP, including indentation, namespace
declarations, DTD sections and escaping:

```php
$w = new XMLWriter();
$w->openMemory();
$w->setIndent(true);
$w->startDocument('1.0', 'UTF-8');
$w->startElementNs('p', 'root', 'urn:example');
$w->writeAttribute('id', '1');
$w->writeElement('title', 'Fish & Chips');
$w->startElement('note');
$w->writeCdata('<raw>');
$w->endElement();
$w->endElement();
$w->endDocument();
echo $w->outputMemory();
```

`openUri()` accepts plain file paths and the `php://output`, `php://stdout`
and `php://stderr` stream URIs. URI writers buffer their output and write it
on `flush()`, `endDocument()` and destruction; `flush()` returns the byte count
for them and the buffered string for memory writers, exactly like PHP. Invalid
element, attribute, PI and DTD names raise PHP's `ValueError`, and an unopened
writer raises `Error: Invalid or uninitialized XMLWriter object`.

## Runtime limits

The limits below are properties of the compiled runtime that SAX parsing — a
callback per event — brings to the surface; none of them is a libxml2 cost, and
the writer is not affected. The numbers are measurements on this release
(`--gc-stats` prints allocation and free counts at exit, `--heap-debug` the
leaked block and byte totals), not guarantees.

- **Every handler invocation leaks a small heap block.** The compiled runtime
  does not release one small block per string argument it passes to a closure
  or to a callable resolved at run time (`$f("x")`, `call_user_func($name,
  "x")`, `[$object, 'method']`), nor per argument bound to a `mixed` or
  untyped parameter; integer, float, bool and object arguments bound to typed
  parameters, and direct calls to named functions, do not leak. This is a
  runtime-wide defect
  that xml does not introduce, but a SAX handler receives a name or data string
  on every event, so each element-start, element-end, character-data and
  default event costs one block of 32–40 bytes, whether the handler is a
  closure literal, a function named in the call, or a callable chosen at run
  time. In a probe with a start and an end element handler declared
  `array $attributes`, a document of 85,000 elements (170,000 handler calls)
  completes on the default 8 MB heap and one of 90,000 stops with
  `Fatal error: heap memory exhausted` (exit status 1). Compile with
  [`--heap-size=BYTES`](../compiling/linking-and-conditional-compilation.md#heap-size)
  to raise the ceiling: the same probe built with `--heap-size=67108864`
  (64 MB) parses 300,000 elements. Parsing with no handler installed does not
  leak at all.
- **A `mixed` attribute parameter also leaks the attribute map.** When the
  start-element handler declares its third parameter `mixed` — the declaration
  a dynamically resolved handler needs, see the next item — every
  start-element event additionally leaks the boxed attribute array: about
  1.3 KB and six blocks per element with one attribute in the same probe, so
  the default heap is exhausted between 6,000 and 7,000 elements. A closure
  literal or a function named literally in the `xml_set_*_handler()` call can
  keep `array $attributes` and pays only the per-event block above.
- **A bare `array` parameter on a dynamically resolved handler receives
  garbage.** A handler chosen at run time — a name held in a variable, a method
  bound through `xml_set_object()`, a name supplied from `eval()` — whose
  attribute parameter is declared `array` is handed a packed array of stray
  integers instead of the map (`json_encode()` prints `[10,7]` where PHP prints
  `{"K":"v","X":"y"}`), because the runtime invoker unboxes the associative
  payload as a packed array. Declare that parameter `mixed`. A closure literal,
  or a function named literally in the call, is typed by the compiler and
  receives the map correctly with either declaration.
- **`xml_parse_into_struct()` holds the whole document on the heap.** The
  `$values` entries, and the per-tag `$index` lists when the caller passes
  `$index`, are ordinary PHP arrays that live until the call returns: in a probe
  of identical empty elements about 1.6 KB per element, so a document of 4,000
  elements completes on the default 8 MB heap, one of 5,000 stops with
  `Fatal error: heap memory exhausted`, and `--heap-size=67108864` (64 MB) takes
  30,000. This is the document's own footprint (PHP holds the same arrays),
  not a leak; the SAX handlers are not involved and cost nothing here.
- **Linux link inputs.** A program that links the bridge needs the zlib and
  bzip2 development libraries at link time (`-lz -lbz2`, the runtime's stream
  layer), as described under [Enabling xml](#enabling-xml).

## Differences from PHP

- The parser is libxml2 2.15.3 itself, driven through the same push-parser
  entry points php-src's `ext/xml` uses, so event boundaries, positions, error
  codes and character-data chunking are libxml2's own. Only the internal DTD
  subset is read: the bridge refuses every external resource load, so external
  subsets and external parsed entities are never fetched, whatever the document
  declares and whatever the `php://` or filesystem environment offers.
- The deprecations PHP 8.4 and 8.5 attach to `xml_set_object()`,
  `xml_parser_free()` and non-callable string handlers are not emitted; the
  functions behave as PHP's do.
- `xml_parse_into_struct()` throws `Error` instead of warning and returning
  `false` when called from inside a handler, matching `xml_parse()`. Its
  "Maximum depth exceeded" warning past 255 nested levels is not printed,
  although the result is truncated like PHP's.
- Only public methods can be bound as handlers: `[$this, 'privateMethod']`, a
  protected method pair, and private or protected method names bound through
  `xml_set_object()` are all rejected; PHP accepts them.
- A handler declared with more required parameters than its event supplies is
  a compile error for a closure literal. A dynamically named handler throws a
  catchable `ArgumentCountError` like PHP, but its message names
  `call_user_func_array()` instead of the handler and argument counts.
- `xml_parser_set_option()` does not emit PHP's `E_WARNING` for a value of the
  wrong type or an out-of-range `XML_OPTION_SKIP_TAGSTART`.
- `XMLWriter::openUri()` reports failures through the stream layer's own
  warning rather than PHP's `Unable to resolve file path` text, and supports
  plain paths plus `php://output`, `php://stdout` and `php://stderr` — not
  `file://` URIs or `php://memory`. `XMLWriter::toStream()` does not detect a
  closed resource.
- The `xml_set_object()` swap error names a bound method as the caller spelled
  it; PHP prints the method's declared spelling. `xml_parser_free()` called from
  inside a handler returns `false` like PHP but without PHP's warning.
- `xml_parse_into_struct()`'s `$values` and `$index` are typed `mixed`, so a
  variable holding them can still be iterated and indexed like any array. Both
  outputs must be variables (locals, statics, globals or by-reference
  captures), not properties or array elements, and they are left untouched
  when a handler throws, where PHP hands back the partial arrays.
- Inside `eval()`, a handler must be a callable the compiled program can
  invoke: a compiled function name, a compiled `[$object, 'method']` pair, or
  a method name bound through `xml_set_object()`. A closure declared inside the
  `eval()` fragment is rejected with a catchable `Error` naming the limitation,
  because the compiled parser cannot run interpreted code.
- `xmlwriter_open_uri()` throws `ValueError` when the target cannot be opened
  instead of returning `false`, because its return type is `XMLWriter`.

- `XMLParser::__clone()` and `XMLWriter::__clone()` are declared `final`, so a
  subclass that overrides `__clone()` is a compile error (`Cannot override final
  method XMLWriter::__clone`). PHP compiles the override and still refuses the
  clone at run time; either way `clone` throws `Error: Trying to clone an
  uncloneable object of class ...` and the original object keeps its handle.

- A handler that arrives through a run-time unpack (`xml_set_element_handler($p,
  ...$handlers)`) is rejected at compile time with the builtins' general
  `takes exactly N arguments` diagnostic for an unpack of unknown length, where
  PHP binds it at run time. Name the handlers in the call instead
  (`...$args, start_handler: ..., end_handler: ...` works, with the parser
  unpacked from a list or a string-keyed array).

See the generated [`xml_parse()` reference](./builtins/xml/xml_parse.md) and
the neighboring XML builtin pages for individual signatures and backend support.

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

### xml

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`xml_error_string()`](./builtins/xml/xml_error_string.md) | `(int $error_code): ?string` | `?string` | ✓ | ✓ |
| [`xml_get_current_byte_index()`](./builtins/xml/xml_get_current_byte_index.md) | `(mixed $parser): int` | `int` | ✓ | ✓ |
| [`xml_get_current_column_number()`](./builtins/xml/xml_get_current_column_number.md) | `(mixed $parser): int` | `int` | ✓ | ✓ |
| [`xml_get_current_line_number()`](./builtins/xml/xml_get_current_line_number.md) | `(mixed $parser): int` | `int` | ✓ | ✓ |
| [`xml_get_error_code()`](./builtins/xml/xml_get_error_code.md) | `(mixed $parser): int` | `int` | ✓ | ✓ |
| [`xml_parse()`](./builtins/xml/xml_parse.md) | `(mixed $parser, string $data, bool $is_final = false): int` | `int` | ✓ | ✓ |
| [`xml_parse_into_struct()`](./builtins/xml/xml_parse_into_struct.md) | `(mixed $parser, string $data, mixed $values, mixed $index = null): int` | `int` | ✓ | ✓ |
| [`xml_parser_create()`](./builtins/xml/xml_parser_create.md) | `(?string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`xml_parser_create_ns()`](./builtins/xml/xml_parser_create_ns.md) | `(?string $encoding = null, string $separator = ':'): mixed` | `mixed` | ✓ | ✓ |
| [`xml_parser_free()`](./builtins/xml/xml_parser_free.md) | `(mixed $parser): bool` | `bool` | ✓ | ✓ |
| [`xml_parser_get_option()`](./builtins/xml/xml_parser_get_option.md) | `(mixed $parser, int $option): mixed` | `mixed` | ✓ | ✓ |
| [`xml_parser_set_option()`](./builtins/xml/xml_parser_set_option.md) | `(mixed $parser, int $option, mixed $value): bool` | `bool` | ✓ | ✓ |
| [`xml_set_character_data_handler()`](./builtins/xml/xml_set_character_data_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_default_handler()`](./builtins/xml/xml_set_default_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_element_handler()`](./builtins/xml/xml_set_element_handler.md) | `(mixed $parser, mixed $start_handler, mixed $end_handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_end_namespace_decl_handler()`](./builtins/xml/xml_set_end_namespace_decl_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_external_entity_ref_handler()`](./builtins/xml/xml_set_external_entity_ref_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_notation_decl_handler()`](./builtins/xml/xml_set_notation_decl_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_object()`](./builtins/xml/xml_set_object.md) | `(mixed $parser, mixed $object): bool` | `bool` | ✓ | ✓ |
| [`xml_set_processing_instruction_handler()`](./builtins/xml/xml_set_processing_instruction_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_start_namespace_decl_handler()`](./builtins/xml/xml_set_start_namespace_decl_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_unparsed_entity_decl_handler()`](./builtins/xml/xml_set_unparsed_entity_decl_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |

Classes: `XMLParser`.

Constants: `XML_ERROR_ASYNC_ENTITY`, `XML_ERROR_ATTRIBUTE_EXTERNAL_ENTITY_REF`, `XML_ERROR_BAD_CHAR_REF`, `XML_ERROR_BINARY_ENTITY_REF`, `XML_ERROR_DUPLICATE_ATTRIBUTE`, `XML_ERROR_EXTERNAL_ENTITY_HANDLING`, `XML_ERROR_INCORRECT_ENCODING`, `XML_ERROR_INVALID_TOKEN`, `XML_ERROR_JUNK_AFTER_DOC_ELEMENT`, `XML_ERROR_MISPLACED_XML_PI`, `XML_ERROR_NONE`, `XML_ERROR_NO_ELEMENTS`, `XML_ERROR_NO_MEMORY`, `XML_ERROR_PARAM_ENTITY_REF`, `XML_ERROR_PARTIAL_CHAR`, `XML_ERROR_RECURSIVE_ENTITY_REF`, `XML_ERROR_SYNTAX`, `XML_ERROR_TAG_MISMATCH`, `XML_ERROR_UNCLOSED_CDATA_SECTION`, `XML_ERROR_UNCLOSED_TOKEN`, `XML_ERROR_UNDEFINED_ENTITY`, `XML_ERROR_UNKNOWN_ENCODING`, `XML_OPTION_CASE_FOLDING`, `XML_OPTION_PARSE_HUGE`, `XML_OPTION_SKIP_TAGSTART`, `XML_OPTION_SKIP_WHITE`, `XML_OPTION_TARGET_ENCODING`, `XML_SAX_IMPL`.

### xmlwriter

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`xmlwriter_end_attribute()`](./builtins/xml/xmlwriter_end_attribute.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_cdata()`](./builtins/xml/xmlwriter_end_cdata.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_comment()`](./builtins/xml/xmlwriter_end_comment.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_document()`](./builtins/xml/xmlwriter_end_document.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_dtd()`](./builtins/xml/xmlwriter_end_dtd.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_dtd_attlist()`](./builtins/xml/xmlwriter_end_dtd_attlist.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_dtd_element()`](./builtins/xml/xmlwriter_end_dtd_element.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_dtd_entity()`](./builtins/xml/xmlwriter_end_dtd_entity.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_element()`](./builtins/xml/xmlwriter_end_element.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_pi()`](./builtins/xml/xmlwriter_end_pi.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_flush()`](./builtins/xml/xmlwriter_flush.md) | `(mixed $writer, bool $empty = true): mixed` | `mixed` | ✓ | ✓ |
| [`xmlwriter_full_end_element()`](./builtins/xml/xmlwriter_full_end_element.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_open_memory()`](./builtins/xml/xmlwriter_open_memory.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`xmlwriter_open_uri()`](./builtins/xml/xmlwriter_open_uri.md) | `(string $uri): mixed` | `mixed` | ✓ | ✓ |
| [`xmlwriter_output_memory()`](./builtins/xml/xmlwriter_output_memory.md) | `(mixed $writer, bool $flush = true): string` | `string` | ✓ | ✓ |
| [`xmlwriter_set_indent()`](./builtins/xml/xmlwriter_set_indent.md) | `(mixed $writer, bool $enable): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_set_indent_string()`](./builtins/xml/xmlwriter_set_indent_string.md) | `(mixed $writer, string $indentation): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_attribute()`](./builtins/xml/xmlwriter_start_attribute.md) | `(mixed $writer, string $name): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_attribute_ns()`](./builtins/xml/xmlwriter_start_attribute_ns.md) | `(mixed $writer, ?string $prefix, string $name, ?string $namespace): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_cdata()`](./builtins/xml/xmlwriter_start_cdata.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_comment()`](./builtins/xml/xmlwriter_start_comment.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_document()`](./builtins/xml/xmlwriter_start_document.md) | `(mixed $writer, ?string $version = '1.0', ?string $encoding = null, ?string $standalone = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_dtd()`](./builtins/xml/xmlwriter_start_dtd.md) | `(mixed $writer, string $qualifiedName, ?string $publicId = null, ?string $systemId = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_dtd_attlist()`](./builtins/xml/xmlwriter_start_dtd_attlist.md) | `(mixed $writer, string $name): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_dtd_element()`](./builtins/xml/xmlwriter_start_dtd_element.md) | `(mixed $writer, string $qualifiedName): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_dtd_entity()`](./builtins/xml/xmlwriter_start_dtd_entity.md) | `(mixed $writer, string $name, bool $isParam): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_element()`](./builtins/xml/xmlwriter_start_element.md) | `(mixed $writer, string $name): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_element_ns()`](./builtins/xml/xmlwriter_start_element_ns.md) | `(mixed $writer, ?string $prefix, string $name, ?string $namespace): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_pi()`](./builtins/xml/xmlwriter_start_pi.md) | `(mixed $writer, string $target): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_text()`](./builtins/xml/xmlwriter_text.md) | `(mixed $writer, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_attribute()`](./builtins/xml/xmlwriter_write_attribute.md) | `(mixed $writer, string $name, string $value): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_attribute_ns()`](./builtins/xml/xmlwriter_write_attribute_ns.md) | `(mixed $writer, ?string $prefix, string $name, ?string $namespace, string $value): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_cdata()`](./builtins/xml/xmlwriter_write_cdata.md) | `(mixed $writer, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_comment()`](./builtins/xml/xmlwriter_write_comment.md) | `(mixed $writer, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_dtd()`](./builtins/xml/xmlwriter_write_dtd.md) | `(mixed $writer, string $name, ?string $publicId = null, ?string $systemId = null, ?string $content = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_dtd_attlist()`](./builtins/xml/xmlwriter_write_dtd_attlist.md) | `(mixed $writer, string $name, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_dtd_element()`](./builtins/xml/xmlwriter_write_dtd_element.md) | `(mixed $writer, string $name, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_dtd_entity()`](./builtins/xml/xmlwriter_write_dtd_entity.md) | `(mixed $writer, string $name, string $content, bool $isParam = false, ?string $publicId = null, ?string $systemId = null, ?string $notationData = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_element()`](./builtins/xml/xmlwriter_write_element.md) | `(mixed $writer, string $name, ?string $content = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_element_ns()`](./builtins/xml/xmlwriter_write_element_ns.md) | `(mixed $writer, ?string $prefix, string $name, ?string $namespace, ?string $content = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_pi()`](./builtins/xml/xmlwriter_write_pi.md) | `(mixed $writer, string $target, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_raw()`](./builtins/xml/xmlwriter_write_raw.md) | `(mixed $writer, string $content): bool` | `bool` | ✓ | ✓ |

Classes: `XMLWriter`.

<!-- elephc:generated:symbols:end -->
