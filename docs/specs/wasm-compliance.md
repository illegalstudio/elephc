---
title: "WebAssembly (WASI)"
description: "How to compile PHP to a WebAssembly/WASI module with Elephc, how to run it, and what the wasm32-wasi target supports today."
sidebar:
  order: 13
---

Elephc can compile a PHP program to a WebAssembly module instead of a native
binary. The module is a **WASI Preview 1 command**: it starts at `_start`, reads
its arguments and environment through WASI, writes to the standard streams,
and ends with an exit status. It runs on any WASI Preview 1 host, such as
Wasmtime, Wasmer or Node.js.

The target is **experimental and partial**. It supports a growing part of the
PHP language and runtime. A program that needs something the target cannot
compile yet is **refused at compile time, with the reason**. If a module
compiles, nothing in it is left unimplemented to fail later.

## Quick start

```php
<?php
// hello.php
echo "Hello from ", $argv[0], " with ", $argc, " argument(s)\n";
exit(3);
```

```bash
elephc --target wasm32-wasi hello.php       # writes hello.wasm
wasmtime run hello.wasm first               # or: wasmer run hello.wasm -- first
echo $?                                     # 3
```

`--target` also accepts `wasm32-wasip1`, `wasm32-unknown-wasi` and `wasm`.

## Output kinds

| Command | Produces |
|---|---|
| `elephc --target wasm32-wasi app.php` | `app.wasm`, a WASI command module |
| `elephc --target wasm32-wasi --emit-asm app.php` | `app.wat`, the readable WebAssembly text, and no `.wasm` |
| `elephc --target wasm32-wasi --emit npm app.php` | `app-npm/`, a Node.js 20+ ESM package |

Every module is validated against WebAssembly Core 3.0 in the compiler before it
is written. A module is never published half-written: the `.wat`, `.wasm` and
npm outputs are staged, then renamed into place.

Two independent compilations of the same source produce byte-identical `.wat`,
`.wasm` and npm outputs.

Not available on this target: `--emit cdylib`/`staticlib` (a WASI *reactor*
module), `--web`, native source maps and DWARF, native heap and register
controls, native linker and framework flags, and bridge crates (FFI, SDL, the
image extensions, the PDO drivers). Each of these is rejected with a diagnostic
rather than ignored.

## Running a module

### Wasmtime and Wasmer

```bash
wasmtime run app.wasm first second
wasmer run app.wasm -- first second
```

A module has no filesystem access unless the host grants it a directory
(*preopen*): `wasmtime run --dir . app.wasm`, `wasmer run --dir . app.wasm`.
Every relative path in the program resolves against the first preopened
directory. With no preopen, `fopen`, `file_get_contents` and the other file
functions fail the way PHP fails on a missing file (warning plus `false`).
`STDIN`, `STDOUT`, `STDERR` and `php://stdin`, `php://stdout`, `php://stderr`,
`php://output` need no preopen.

### Node.js

The npm package wraps the module in a loader that runs it under `node:wasi`:

```bash
elephc --target wasm32-wasi --emit npm app.php
node app-npm/index.mjs first second
```

It can also be imported:

```js
import { run } from "./app-npm/index.mjs";

const status = await run({
  args: ["app", "first", "second"],      // $argv, script name first
  env: { APP_ENV: "prod" },              // empty unless passed
  preopens: { "/work": process.cwd() },  // filesystem access
});
```

The package contains `module.wasm`, `index.mjs`, TypeScript declarations
(`index.d.ts`), `package.json` and a `README.md`. The README documents how
`run()` validates its arguments, environment and preopens.

### Programs that catch exceptions

A program that contains a `try` uses WebAssembly's exception-handling proposal
(`try_table` with `exnref`). Current Wasmtime and Wasmer enable it by default.
Some Node.js releases still gate it, Node 24 among them, and refuse the module
with:

```
CompileError: ... Invalid opcode 0x1f (enable with --experimental-wasm-exnref)
```

Run them with the flag:

```bash
node --experimental-wasm-exnref app-npm/index.mjs
```

A program with no `try` does not use the proposal. It runs on any WASI
Preview 1 host, even when it can raise an error: an uncaught error ends the
module with PHP's fatal message and status 255.

### Arguments, environment and exit status

- `$argv` is the host's argument vector, and `$argv[0]` is whatever the host
  passes first. To compare output with php-src, pass the script path first:
  php-src puts it in `$argv[0]` and counts it in `$argc`.
- `getenv()` reads the WASI environment. Wasmtime and Wasmer pass no variables
  unless asked (`--env NAME=value`); the npm loader passes only `env`.
- `exit($n)` ends the module with status `$n`; an uncaught exception or fatal
  error ends it with 255 and PHP's message on stderr.

## What is supported today

### Measured on the example corpus

Of the 239 programs under `examples/` that have a `main.php`, **92 compile to
`wasm32-wasi`, and 85 print exactly what php-src prints**. The other seven are
explained in [Targets: measured parity](../compiling/targets.md#measured-parity-against-the-example-suite).
Three use Elephc-only syntax, and two are answers that differ by design
(`extension_loaded`, `spl-autoload`). `gzip` compresses to a valid stream that
is not zlib's byte for byte, and `ip-conversion` uses IPv6, which this target
does not serve.

52 of the examples that do not compile need something WASI Preview 1 has no
equivalent for, such as a native bridge crate, a socket or a network stream.

### The language

These parts of PHP compile and behave as php-src does:

- scalars, strings, indexed and associative arrays, `list()` and keyed
  destructuring, string interpolation;
- functions with typed, nullable, default and variadic parameters, typed returns,
  `declare(strict_types=1)` (per declaring file), `static` locals;
- classes, abstract and final classes, interfaces, traits, constructor
  promotion, `readonly` properties, typed and static properties, class
  constants, `instanceof`, `clone`, destructors;
- backed and pure enums, `Enum::from()`/`tryFrom()`, methods on enums;
- closures and arrow functions with by-value `use` captures, first-class
  callables of user functions (`format(...)`);
- generators: `yield`, keys, `yield from`, `send()`, `getReturn()`;
- exceptions: `throw`, `try`/`catch`/`finally`, multiple catch types, exception
  chaining;
- `match`, the nullsafe operator `?->`, `??` and `??=`, `<=>`;
- integer arithmetic with PHP's overflow to float, `intdiv`, `%`, integer `**`,
  bitwise operators, PHP's loose and strict comparisons;
- output buffering (`ob_start` with or without a handler, `ob_get_clean`, …).

### The runtime library

The functions below have a WASM implementation. A family whose native version
links a library is written again inside the module, because the module links
nothing:

- **Strings:** `strlen`, `substr`, `strpos`/`strrpos`, `strstr`, `str_contains`,
  `str_starts_with`/`str_ends_with`, `str_replace`, `str_repeat`, `str_pad`
  (without a pad type), `str_split`, `explode`/`implode`, `trim`/`ltrim`/`rtrim`,
  `ucfirst`/`lcfirst`/`ucwords`, `strtolower`/`strtoupper`, `strrev`, `wordwrap`,
  `strcmp`/`strcasecmp`, `sprintf`/`printf`/`fprintf`, `chr`/`ord`, `md5`,
  `sha1`, `crc32`, `base64_encode`/`base64_decode`, `bin2hex`,
  `urlencode`/`rawurlencode` and their decoders, `addslashes`/`stripslashes`,
  `nl2br`, `htmlspecialchars`, `parse_url`.
- **Arrays:** `count`, `array_keys`, `array_values`, `array_map`, `array_reduce`,
  `array_merge`, `array_slice`, `array_splice`, `array_search`, `in_array`,
  `array_key_exists`, `array_key_first`/`array_key_last`, `array_unique`,
  `array_reverse`, `array_pad`, `array_fill`, `array_column`, `array_sum`,
  `array_product`, `array_replace`, `array_unshift`, `array_is_list`,
  `array_diff_assoc`, `array_intersect_assoc`, `range`, and the sorts `sort`,
  `rsort`, `usort`, `asort`, `arsort`, `ksort`, `krsort`.
- **Math:** `abs`, `floor`, `ceil`, `round`, `sqrt`, `intdiv`, `min`, `max`;
  bcmath's `bcadd`, `bcmul`, `bcdivmod`, `bcscale`, computed in decimal digits.
- **Files and streams:** `fopen`, `fread`, `fwrite`, `fgetc`, `fseek`, `ftell`,
  `rewind`, `feof`, `fclose`, `flock`, `tmpfile`, `file_get_contents`,
  `file_put_contents`, `file_exists`, `unlink`, `readfile`, `readline`,
  `fscanf`, `stream_get_contents`, `stream_get_line`, `stream_copy_to_stream`,
  `stream_get_meta_data`, the three stream buffer tuners, and
  `opendir`/`readdir`/`rewinddir`/`closedir`.
- **Compression:** `gzcompress`, `gzuncompress`, `gzdeflate`, `gzinflate`. The
  DEFLATE decoder reads anything zlib writes; the encoder is not zlib's, so its
  output is valid but not byte-identical.
- **Regular expressions:** `preg_match` without `$matches`, over a subset of
  PCRE. The pattern must be a literal: it is compiled when the module is built,
  and a construct outside the subset is a compile error that names it.
- **JSON and output:** `json_encode`, `print_r`, `var_dump` of typed values.
- **Introspection:** `gettype`, `settype`, `get_class`, `get_object_vars`,
  `class_exists`, `interface_exists`, `trait_exists`, `enum_exists`,
  `function_exists`, `class_implements`, `class_parents`, `class_uses`, `is_a`,
  `get_declared_classes`, `spl_classes`, `spl_object_id`, `define`,
  `extension_loaded`, `get_loaded_extensions`, `get_resource_type`.
- **System:** `getenv`, `time`, `microtime` (the real wall clock; `sleep` does not
  suspend), `getprotobyname`/`getprotobynumber`, `ip2long`/`long2ip`, and
  IPv4 `inet_pton`/`inet_ntop`.

[Targets: PHP semantics this target reproduces exactly](../compiling/targets.md#php-semantics-this-target-reproduces-exactly)
details how each of these behaves at its edges, measured against php-src.

### Coverage of the compiler's internal operations

Underneath the language, the compiler lowers a PHP program to an intermediate
representation (EIR). [`wasm-inventory.json`](wasm-inventory.json) classifies
each EIR identity for this target:

| Family | Total | Supported | Not applicable | Not yet supported |
|---|---:|---:|---:|---:|
| Value types | 12 | 11 | 1 | 0 |
| Operations | 278 | 158 | 17 | 103 |
| Runtime functions | 614 | 149 | 29 | 436 |
| Runtime call targets | 18 | 7 | 0 | 11 |
| String helpers | 16 | 12 | 0 | 4 |
| Terminators | 8 | 7 | 0 | 1 |
| **All** | **946** | **344** | **47** | **555** |

"Not applicable" covers raw pointers, buffers and calls into native code, which
a WASI module cannot reach. `cargo run --quiet --example gen_wasm_inventory`
regenerates the file, and CI fails when the committed copy is stale.

## What is refused, and how

Before writing any WebAssembly, the compiler checks every operation in the
program, together with the types it receives and produces, how it owns its
values, and the control flow around it. If anything is not supported, compilation stops and
lists each problem with the function and instruction it comes from:

```
WebAssembly backend error: WASM capability audit found 2 issue(s):
  - functions::main block#14 instruction#549: unsupported runtime function array_filter
  - class_methods::Circle::area block#0 instruction#7: unsupported op fpow
```

The main things refused today:

- **missing functions:** among others `array_filter`, the `mb_*` family, the
  `opcache_*` functions, `str_pad`'s fourth argument, the `$matches` argument of
  `preg_match`, and most of the `date`/`time` family beyond `time()` and
  `microtime()`;
- **float `**`**, reflection, `Closure::bind`, `eval` and fibers;
- **references:** `foreach` by reference, calling a closure that captures by
  reference (`use (&$x)`), a write through an untyped by-reference array
  parameter;
- **callables:** a first-class callable of a builtin (`strlen(...)`), calling a
  closure declared `: void`;
- **late static binding** beyond `static::class`, `static::CONST` and
  `new static()`: `static::method()`;
- **an `int` element read through a nested array that may be missing**
  (`$a["missing"][0]`): php answers `null` with a warning, which the plain integer
  read cannot carry, so it is refused rather than answered `0`;
- **`global $name`** inside a function: the global is `mixed` there, and a
  `mixed` or array global has no storage on this target;
- **dynamic shapes:** a computed dynamic property write (`$obj->$name = …` on a
  `mixed` receiver), `var_dump` of a `mixed` value, `is_countable()`/`is_iterable()`
  on a value whose class is not known, a closure capture whose type the body
  changes;
- **generators** with a `yield` inside a `try` that has a `finally` (php runs that
  `finally` when a suspended generator is destroyed; this target cannot);
- **`fscanf`** formats with two conversions and no space between them (`%d%s`);
- **`json_encode()`/`get_object_vars()`** of an object whose class has a typed
  property with no default that the constructor may leave unset (php skips an
  uninitialized property, and this target cannot tell it from a real `0`);
- **`file_put_contents()` whose result is tested** (php answers `int|false`; the shared
  builtin contract types the result `int`, so a failed write would read as `0`);
  calling it as a statement, or echoing its result, compiles (a failed write then
  prints `0` where php prints nothing);
- **I/O the platform does not have:** sockets, `http://`/`ftp://`/`phar://`
  stream wrappers, process control, native extensions. (`php://memory`,
  `php://temp` and `data:` URIs are implemented.)

## Known limitations

These programs compile and run, but differ from php-src:

- Memory is not reclaimed in a few cases: an exception that escapes a function
  leaks what that function owned; reassigning a by-value parameter leaks the old
  value; `str_pad` in a loop and array literals holding strings grow memory; a
  concatenation passed as an argument to a function that does not return it
  (`$obj->set("a" . $i)`) is never freed; yielding the result of a captured
  closure directly (`yield $cb();`) leaks about 40 bytes per yield (assigning
  it to a variable first does not); an unclosed `php://temp`/memory stream
  keeps its pages; a closed stream keeps a small record (about 80 bytes for a
  memory stream, 40 for any other) so that every copy of its handle stays closed,
  and a stream dropped without `fclose` is not closed at that point.
- A `php://temp` stream that would grow past the memory a WebAssembly module
  can address (for instance after `fseek($h, 2147483648)`) stops with the
  fatal error `Allowed memory size exhausted`, where php succeeds.
- A regex whose backtracking exceeds the pattern engine's budget stops with a
  fatal error naming the limit, where php answers `false`.
- An uncaught error prints a class-agnostic fatal, and diagnostics omit php's
  `in <file> on line <n>` tail. A `TypeError` raised by `count()` is a fatal
  error rather than a catchable exception.
- A `mixed` value stored into a typed `int`, `float`, `bool` or `?int` property,
  static property or parameter is coerced as php does, and raises a catchable
  `TypeError` when php would. Its message is a generic one rather than php's,
  which names the property and the type. A `mixed` value stored into a
  `string` property is cast instead (an array becomes `"Array"`, `null`
  becomes `""`), where php raises a `TypeError`. The same silent casts apply to
  typed closure parameters reached through a callable.
- For those `mixed` coercions, `declare(strict_types=1)` in any file of the
  program makes them strict everywhere, rather than only in that file.
- `inet_pton`/`inet_ntop` serve IPv4 only and answer `false` for IPv6.
- Output an `ob_start()` handler produces while it runs is discarded, as php
  discards it, but php's deprecation notice for it is not printed. A handler
  that throws does not end its buffer the way php does.
- `--emit-ir` shows different EIR for this target than for native ones: the
  WASM pipeline skips the optimizer passes so that every PHP coercion stays
  visible to the compile-time check.

## Standards and tested hosts

| Layer | Reference |
|---|---|
| Module format | WebAssembly Core 3.0 (WebAssembly/spec tag `wg-3.0`, commit `9d36019973201a19f9c9ebb0f10828b2fe2374aa`), plus the exception-handling proposal for programs with a `try` |
| System interface | WASI Preview 1 (WebAssembly/WASI commit `e840fe45e63b4f227a29fa87df94ab3bbe3d5efb`, `wasi_snapshot_preview1.witx`) |
| PHP behavior | php-src: the `--php-version` profiles 8.2 to 8.5 |

CI validates one module with `wasmparser`, `wasm-tools`, Wasmer, Wasmtime and
Node, then runs it under all three hosts. It checks the exact output, the
`$argc`/`$argv` contents, `exit(7)`, partial writes and repeated imports of the
npm package. The pinned versions are Wasmer 7.2.1, Wasmtime 47.0.2, `wasm-tools`
1.254.0, Node.js 26.3.0 and TypeScript 6.0.3. Preview 2 and the component model
are not supported.

The php-src releases that PHP-visible behavior is checked against are pinned
here, with both the annotated tag object and the commit it peels to:

| Profile | php-src tag | Annotated tag object | Peeled commit |
|---|---|---|---|
| `8.2` | `php-8.2.33` | `fa98f62b39a612ae88b7be5d5ea9ff9b794b454b` | `651db3ebfa622cae0c4e6b39766812efbd274ced` |
| `8.3` | `php-8.3.33` | `a7413fbd1dd4dccda419ca473ce475f084edaadd` | `4a55da8cb580ba56887c80a42291ebc676d6fb43` |
| `8.4` | `php-8.4.24` | `3cb6f7231aed24c4ae77a0d3ee5aeeb2b968ad30` | `fb193d3df72d1ca3b5ef58ec9e9b6ef5bdf18bef` |
| `8.5` | `php-8.5.9` | `d6bbf3ed631eea9763a2b790653fc91b69f0af7a` | `dd6e76cce27aaa0ed9f7520648ed1081dfb6af36` |

`scripts/build-pinned-php-src.sh` builds these releases for the differential
tests.
