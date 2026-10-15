---
title: "Targets and cross-compilation"
description: "The supported target matrix, how to select a target with --target, and the accepted target spellings."
sidebar:
  order: 4
---

elephc compiles to native machine code for a fixed set of first-class targets.
All native targets are equal: a feature is not considered done until it works on
every one of them. Elephc can also target experimental platforms outside this
matrix. `wasm32-wasi` is one such target: it compiles to a WebAssembly module
rather than native machine code and is documented separately below.

## Supported target matrix

| Target | Platform | Architecture |
|---|---|---|
| `macos-aarch64` | macOS | ARM64 (Apple Silicon) |
| `ios-arm64` | iOS device | ARM64 |
| `ios-sim-arm64` | iOS Simulator | ARM64 |
| `linux-aarch64` | Linux | ARM64 |
| `linux-x86_64` | Linux | x86-64 |

By default the compiler targets the **host** it runs on, detected automatically.
The native macOS/Linux targets are the first-class supported matrix.

## Experimental targets

| Target | Platform | Architecture | Status |
|---|---|---|---|
| `wasm32-wasi` | WebAssembly / WASI Preview 1 | wasm32 | Experimental; incomplete PHP/EIR parity and portability gates |

Experimental target availability is not a first-class support guarantee. See
[WebAssembly partial parity](#webassembly-partial-parity) for the currently
tested surface and open gates.

## Selecting a target

```bash
elephc --target linux-aarch64 hello.php
elephc --target linux-x86_64 hello.php
elephc --target=macos-aarch64 hello.php
elephc --target ios-arm64 --emit staticlib module.php
elephc --target ios-sim-arm64 --emit staticlib module.php
```

Both the spaced (`--target VALUE`) and inline (`--target=VALUE`) forms work.

## Accepted spellings

Each target accepts several spellings, including the LLVM-style triple, so build
scripts written for other toolchains keep working:

| Canonical | Also accepted |
|---|---|
| `macos-aarch64` | `macos-arm64`, `aarch64-apple-darwin` |
| `ios-arm64` | `ios-aarch64`, `aarch64-apple-ios` |
| `ios-sim-arm64` | `ios-simulator-arm64`, `aarch64-apple-ios-simulator` |
| `linux-aarch64` | `linux-arm64`, `aarch64-unknown-linux-gnu` |
| `linux-x86_64` | `x86_64-unknown-linux-gnu` |
| `wasm32-wasi` | `wasm32-wasip1`, `wasm32-unknown-wasi`, `wasm` |

## WebAssembly partial parity

The `wasm32-wasi` target is a non-native target: instead of emitting native
assembly and invoking the system assembler and linker, it emits a WebAssembly
module (`.wat`/`.wasm`) through the dedicated `src/codegen_wasm` backend, which
consumes the same EIR the native backends use. The target is experimental.
Production artifact generation performs in-process Core validation with
`wasmparser`. The dedicated host-portability job is green on Elephc
`0505b837ad`; it checksum-pins Wasmer 7.2.1, Wasmtime 47.0.2, `wasm-tools`
1.254.0, Node 26.3.0, and TypeScript 6.0.3. `--emit npm` packages the resulting
command module for Node.js 20 or newer.

Unlike the native macOS/Linux targets, `wasm32-wasi` is **not yet at full
parity**. It supports a growing subset of the language, and an EIR operation
that the WebAssembly backend does not yet implement aborts compilation of the
whole module rather than degrading a single function. The pre-emission
capability audit classifies every operation, runtime identity, and terminator,
then checks cross-cutting operand, result, immediate, representation,
ownership, callable, object/property, iterator, and control-flow shapes. Static
acceptance is followed by one exact in-memory planning pass; every fallible
lowering and identifier-consistency check must succeed before an accepted plan
exists. Artifact publication consumes that private plan without re-running
lowering, so a module accepted by the complete gate cannot later fail with a
backend `Unsupported`. Rejected source remains a target-capability gap even
though no artifact is published. How to compile and run a module, and what the
target supports today, is described in
[WebAssembly (WASI)](../specs/wasm-compliance.md).

The durable tested inventory currently includes:

- production in-process WAT assembly and Core 3.0 validation;
- focused artifact publication, shape-complete capability, and target-capability rejection
  tests;
- focused typed-transfer, `$argc`, void/Mixed result, block-argument, loop, and
  deterministic-identifier regressions;
- one shared artifact validated with `wasmparser`, `wasm-tools`, Wasmer,
  Wasmtime, and Node, then executed under all three hosts with exact output and
  `exit(7)`;
- independent-process WAT/WASM/npm/archive reproducibility, partial `fd_write`,
  repeated Node imports, npm contents, and strict TypeScript NodeNext checks;
- refcount balance measured as wasm page growth over 20000 iterations, each
  against a control loop that deliberately retains its children — a flat page
  count means nothing unless the control grows.

It does not yet include a full php-src differential corpus or exhaustive EIR
shape, ownership, argument/environment/preopen, and process-status coverage.

### Measured parity against the example suite

Parity is tracked against the repository's own examples rather than a prose
claim. Of the 239 examples under `examples/` that carry a `main.php`, **92
compile to `wasm32-wasi`**, and 85 of those reproduce php-src's output byte for
byte. Before main's newer EIR was merged in, 106 of the then 219 compiled; the
examples that no longer do reach a shape this target now refuses by name rather
than lowers — a reference-cell operation, a write through an untyped by-reference
array parameter, a computed dynamic-property write, a store of an `int|null` read
into a fresh array, foreach by reference, or an opcache runtime function — and
none of them miscompiles.

The seven that do not reproduce php-src are not seven defects. `ifdef`,
`union-types` and `generics` use Elephc-only syntax php-src cannot parse at all —
they are checked against the NATIVE backend instead, and agree with it.
`ip-conversion` is the one gap: main taught its IPv6 half to the native runtime,
and this target's `inet_pton`/`inet_ntop` serve IPv4 only, answering `false` for
the sixteen-byte form (see `src/codegen_wasm/inet.rs`).

`extension_loaded` is the fifth, and it cannot match php-src on ANY elephc
backend: the answer is a property of the compiled binary, not of the host's php.
Native reports ten extensions where the comparison host reports sixty-six, and
answers `curl: no` where the host answers yes, because the program did not link
it. This target applies that same rule to a module that links nothing at all —
see the loaded-extension entry in the semantics section below.

`gzip` is the sixth, and it is the one mismatch that is a genuine
implementation difference rather than a rule. It compresses and decompresses
correctly and round-trips its own output; it prints 39 and 45 bytes where php
prints 40 and 46, because this target's compressor is not zlib. The entry in
the semantics section below gives the reason and the measurements.

`spl-autoload` is the seventh, and it is the only one whose reference is the
NATIVE binary rather than php-src. The example exists to demonstrate compile-time
autoloading: the compiler reads `composer.json`'s PSR-4 map and the body of the
closure passed to `spl_autoload_register`, and resolves every class before the
program runs. php-src has no such map — it fatals on the first `class_alias`,
having never found the file — so there is no php-src output to match. Against
the native binary this target agrees line for line but one: `get_declared_classes()`
answers 105 names where native answers 106, because `Pcntl\QosClass` is injected
only for macOS targets, which is php-src's own platform guard. So every example
this target compiles, it also runs correctly.

### Shapes refused rather than answered wrongly

Comparing this backend against php-src found programs it accepted and answered
wrongly. Most were fixed; three are refused by name instead, because a correct
answer needs more than a local change:

- a generator with a `yield` inside a `try` that has a `finally`: php runs that
  `finally` when a suspended generator is destroyed, and a frame dropped without
  resuming cannot;
- `fscanf` conversions that must stop inside a token, such as `%d%s` over
  `"12abc"`, and every numeric conversion (`%d`, `%f`, `%e`, `%g`): php returns
  an int or a float for those, and the checker types every `fscanf` field as a
  string, so the call is refused by name rather than answered with the wrong
  type;
- a regex whose backtracking exhausts the pattern VM's budget now stops with a
  fatal naming the limit, where it used to answer a silent `0` (php answers the
  match, or `false` with `PREG_BACKTRACK_LIMIT_ERROR`).

An unclosed `php://memory` stream is still never released when its last value is
dropped: a resource stored in an array is re-boxed into a fresh cell on every
read, so freeing on a cell's release would close a stream the array still holds.
It needs a per-stream reference count.

### What the host has to support

A module whose program contains a `try` uses WebAssembly's exception-handling
proposal, and will not COMPILE on a host that does not implement it. The
encoding emitted is the final one, `try_table` with an `exnref` catch clause,
not the withdrawn `try`/`catch` form.

The trigger is the `try`, not the throw. A program that can raise a catchable
PHP error but never catches one — `intdiv()` by zero in a file with no `try`
anywhere — emits no exception instruction at all: the guard exits with the
fatal directly, so such a module still runs on a host without the proposal.

That proposal is enabled by default in current engines, and gated in slightly
older ones. On a Node that rejects the module with

```
CompileError: Invalid opcode 0x1f (enable with --experimental-wasm-exnref)
```

pass `--experimental-wasm-exnref`; `0x1f` is `try_table`, and the flag's name is
the tell that the engine has the right proposal and has simply not stabilized it
yet. A host missing exception handling ALTOGETHER cannot run such a module at
all.

The measurement below does not depend on which of those a developer happens to
have installed. The codegen suite and the parity sweep both PROBE once — an
engine with the proposal on by default still accepts the flag as a no-op, so
acceptance is the question asked, not need — and pass it when it is understood.
Before that probe existed the same corpus scored nine fewer matches on a Node
that gated the proposal, with no change to the compiler.

Every other module — one whose program contains no `try` — has no such
requirement and runs on any WASI Preview 1 host.

When comparing against php-src yourself, pass the script path as the module's
first WASI argument. php-src puts it in `$argv[0]` and counts it in `$argc`; a
host that starts the module with an empty argument vector makes both differ for
reasons that have nothing to do with the backend.

**52 of the 115 remaining examples depend on something WASI Preview 1 has no
equivalent for.** That is the count of examples citing at least one refusal
whose text names `extern_call` (28 examples: native bridge crates for FFI, SDL,
the image extensions, the PDO drivers, `eval`, `xml`), a socket (10), or a
stream wrapper (24, of which 14 cite nothing else from the first two). Some of
those bridges could in principle be ported to WebAssembly one day, so treat the
number as the shape of the remaining corpus rather than a hard ceiling — but
none of them is a WASM backend gap in the sense the rest of this section counts.

**Seven more never reach the backend at all.** `pdo-cubrid`, `pdo-oci`,
`pdo-sqlsrv`, `strict-php`, `web-session`, `web-session-trans-sid` and
`web-session-upload` fail in the front end — an undefined function, an undefined
class constant, a parameter type — and would fail the same way on every target.
They are counted as not compiling because they do not, but nothing in this
document would change if they did.

That leaves 56 reachable by work on this backend.

Of the rest, the work is a long tail rather than a few large levers. Counting
the distinct refusals a single `elephc --target wasm32-wasi` reports per example:
of the 115 that do not compile, 7 have one distinct blocker, 9 have two, 8
have three, 7 have four, and the tail runs to 135 for the PDO-driver examples.
(Distinct REASONS, not refusal lines: ranking by lines answers "how often is
this hit", which is a different question from "what would unblock an example".)

The seven reachable by a single blocker are `constructor-promotion`,
`dir-wrapper`, `fsockopen`, `ftp-stream`, `https-stream`, `phar-writer` and
`socket-pair`. Every one of them is a dead end rather than work: `fsockopen`,
`socket-pair` and `https-stream` are among the fifty-two; `dir-wrapper`,
`ftp-stream` and `phar-writer` need stream wrappers this target refuses on
purpose; and php-src itself fatals on `constructor-promotion`, so compiling it
would match nothing.

That the list is nearly all dead ends is the measurement's real finding, and it
survives a set-cover reading: closing the THIRTY most-cited remaining gaps
together unblocks no example at all, because the median reachable example needs
about seven of them. There is no lever — but there is a shape. The cheap tier is
one self-contained BUILTIN FAMILY per example — `get_object_vars`, the
`opendir`/`readdir`/`rewinddir`/`closedir` cursor, the loaded-extension set, the
introspection four (`get_declared_classes`, `spl_classes`, `spl_object_id`,
`is_a`), the carried protocol registry, the three stream tuners, the
`fprintf`/`fscanf` round trip, bcmath's decimal arithmetic and RFC 1951 — each
finished by comparing its example to php-src byte for byte, or to the rule
that makes the comparison impossible.

The last two settle what a LIBRARY rewrite costs here. Since this target
assembles one self-contained WAT text and links nothing, a family whose native
implementation is a linked crate has to be written again in the module:
roughly 600 lines of WAT for bcmath's decimal add, multiply, long division,
parsing and rendering, and roughly 700 for DEFLATE's bit reader and writer,
canonical Huffman decoder, three block types and LZ77 matcher. Both are
tractable; they differ in what "done" means. bcmath's answers are the
format's, so it matches php-src on every case measured. DEFLATE's
decompressor is likewise exact — it reads what zlib writes — but its
COMPRESSOR is an implementation among several valid ones, and matching php
byte for byte would mean reproducing zlib's buffering rather than the
standard. That is the line worth drawing: a rewrite can reach parity on a
format, and cannot on a heuristic.

All three are now written. PCRE was the last and the least like the others:
bcmath and DEFLATE have a specification to be right against, while a regex
engine is a whole language, and no amount of it would ever be "finished". What
made a PARTIAL one shippable is that the pattern is a LITERAL — so it is
compiled when the module is built, and every construct outside the subset is a
compile error naming itself rather than a wrong answer later. That is the
shape any further library work here should take. Since this
target assembles one self-contained WAT text and links nothing, a family whose
native implementation is a library — bcmath, zlib, PCRE — is a rewrite in WAT
rather than a port, which is a different order of commitment.

The most WIDELY shared refusals are not the most useful ones to fix, and it is
worth being precise about why. The SPL iterator hierarchy — `IteratorIterator`,
`FilterIterator`, `AppendIterator`, `ParentIterator` and `SplFixedArray` —
contributes refusals to fifteen examples from its prelude classes alone, with no
counterpart in the user code of any of them: a typed property that may be
uninitialized, and a virtual method with no concrete candidate. That reads like
a lever, and is not one. Every one of those fifteen carries between 21 and 135
distinct blockers of its own — `extern_call`, `time`, `date`, the PDO drivers —
so completing the hierarchy would unblock none of them. Counting how many
examples MENTION a gap measures how much prelude they pull in, not what would
compile if it were closed.

What the measurement does support is that progress runs at roughly one example
per fix, which makes the example counter a poor guide to correctness work.
Running a differential corpus against php-src has been finding more, and more
serious, defects than making the counter move: admitting the inline `int|null`
pair moved the counter by one and uncovered a `finally` that ran for only the
first exit out of a protected body — on BOTH backends, and on main. The
string-boxing widening found alongside `"123" + 3` is the same pattern. The
defect is always already there; something else refusing first is what hides it.

Note also that the example suite is not a pure PHP corpus, so "matches php-src"
is not a question that can be asked of all of it: some examples do not parse
under php-src at all, because they use Elephc-only syntax, and more reach a
fatal for an Elephc-only builtin, an extension the comparison machine lacks, or
a service that is not running.

### What a differential fuzz has covered

The example corpus answers "does this program compile and match", which stops
being informative once the reachable examples are gone. A differential fuzz
answers a different question — "does this OPERATION match" — by generating
calls with literal arguments, running them under this target and under php-src,
and diffing. It has found more than the corpus has:

| surface | cases | disagreements |
| --- | --- | --- |
| Pattern matching (`preg_match`, supported grammar) | 1500 | 0, after a fix |
| Arithmetic, comparison, casts, bitwise, rounding | 2000 | 0 |
| Array builtins | 1200 | 0, after a fix |
| String builtins | 600 | 0, apart from diagnostic framing |

Two defects came out of it, neither of which the corpus could have shown:

- A loop whose body can match nothing made the pattern VM spin to its step
  budget and answer 0, taking the alternatives that would have matched with it
  (15 of 400 patterns).
- `json_encode` rendered every keyed array as a JSON object, where php decides
  by whether the keys are `0..n-1` in order (14 of 500 array calls).

The remaining string-builtin difference is the diagnostic framing in the next
section, which is not a string builtin and not this target's alone.

Each generator keeps operand types CONSISTENT per call — an int list gets an
int needle — because the capability audit requires a statically typed operand
and refuses the mixed shapes. A mixed one measures the audit rather than the
runtime.

### Diagnostic divergences both backends share

#### Warnings reach the wrong stream, unframed

Found by a differential fuzz over the string builtins — 600 calls with literal
arguments, wasm against php-src — which disagreed on 15 of them, all the same
way. Recorded here rather than fixed because the fix is not this target's to
make: the NATIVE backend does the same thing.

php's CLI writes a warning or a deprecation to **stdout**, framed:

```
\nDeprecated: ord(): ... instead in /path/to/script.php on line 2\n
```

Both elephc backends write it to **stderr**, unframed:

```
Deprecated: ord(): ... instead\n
```

Three differences, measured on php-src 8.5.10 against both backends:

1. **The stream.** php's `display_errors` defaults to `STDOUT` for the CLI, so
   a program that triggers a notice produces different STDOUT under elephc than
   under php. This is the one that matters: stdout is what a caller pipes, and
   what this page's parity numbers compare.
2. **A leading newline**, which php emits unconditionally — measured even when
   the notice is the very first output of the script.
3. **The location**, ` in <file> on line <line>`. Both backends have the file
   (`Throwable::getFile()` already reports it here) and neither threads the
   line to the diagnostic.

The first two are a data change. The third is a design question: elephc's
diagnostics are emitted from RUNTIME HELPERS, which have no source line, so
carrying one means either a global the lowering sets at every site that can
reach a diagnosing helper — a transitive question — or a parameter on all 21
helpers. Whichever it is, it should be answered for both backends at once,
which is why a wasm-only change was reverted rather than committed: it would
have left the two backends disagreeing with each other on every warning, and
rewritten sixteen tests toward an end state that still would not match php.

No example in the corpus triggers a diagnostic on a matching path, which is why
the parity numbers above do not show this.

#### A closure names itself by its internal symbol

A declared return type that a closure violates quotes the EIR's own name for
it. Measured on php-src 8.5.10 and both backends, for
`$f = function (mixed $a): int { return $a; }; $f("nope");`:

| | text |
|---|---|
| php | `{closure:/path/to/script.php:2}(): Return value must be of type int, string returned` |
| elephc, both backends | `__eir_closure_main_0(): Return value must be of type int, string returned` |

The class, the message and the exit status all match; only the name does not.
It is the same shape as the framing divergence above — the two backends agree
with each other and disagree with php — so it is recorded rather than patched
on one side. php's own spelling carries the file and the LINE, which is the
part elephc's diagnostics cannot yet reach from a runtime helper.

Until this work a closure with a declared `int` return did not compile here at
all: the planner laid out a function's name for that diagnostic by walking the
free functions and the methods, and a closure is neither, so
`$f = function (int $a, int $b): int { return $a + $b; };` failed the whole
build with `string property default "__eir_closure_main_0" has no data
segment`. Checked addition widens on the chance of an overflow, which is what
sends an ordinary integer return through the boundary.

### PHP semantics this target reproduces exactly

Behaviour that a naive lowering gets wrong, and that this backend implements
against measured php-src 8.5.6 output rather than by analogy:

- **File I/O, against a preopened directory.** `fopen`, `fread`, `fwrite`,
  `fclose`, `file_get_contents`, `file_put_contents`, `file_exists` and `unlink`
  reach the real filesystem. WASI Preview 1 is capability-based, so a module has
  no filesystem at all unless the host preopens a directory for it — `node`'s
  `preopens`, `wasmer --dir .` — and every path resolves against the first
  preopen. Without one, each answers PHP's failure value, which is also the right
  answer for a host that grants nothing. A stream handle is a boxed value
  carrying a per-open record of the WASI fd, so it lives in locals and arrays like any other. The mode
  string is PHP's: `r`/`w`/`a`/`x`/`c` with an optional `+`, and `b`/`t` read and
  ignored. `STDOUT`, `STDERR`, `STDIN` and the `php://stdout`, `php://stderr`,
  `php://stdin`, `php://output` names reach the process fds directly and need no
  preopen; `fclose` on one of them answers true without closing the fd, which is
  what php-src does for a `php://stdout` handle. Any OTHER `scheme://` path is a
  stream wrapper — `http://`, `ftp://`, `phar://` — and a literal
  one is refused at compile time rather than opened as a filename, which would
  answer `false` where PHP answers a handle.
- **`$mixed[$key]` on a receiver whose type is only known at runtime.** Reading
  one element out of another (`$deep["db"]["host"]`) makes the inner read answer
  `mixed`, so the outer one dispatches on the cell's tag. An array or hash reads
  normally and warns `Undefined array key` on a miss, quoting a string key and
  not an integer one; a STRING is indexed by byte, counting a negative offset
  from the end and warning `Uninitialized string offset` past either end before
  answering `""`; and an int, float, bool or null warns before answering null.
  That last wording is version-profiled the same way the null receiver already
  was: before 8.3 PHP names the type for all of them, and from 8.3 it names the
  type for int and float but the VALUE for a boolean, so `on true` and
  `on false` are distinct messages there. The string and scalar receivers are
  two places the native backend answers a silent null and this one does not.
  A string indexed by a string key, and an object receiver, still answer null
  here as they do natively, rather than php-src's `TypeError` and `Error`.
- **`(float) $string`.** PHP parses the LEADING numeric prefix and answers
  `0.0` when there is none, silently. `"12abc"` is `12.0`, `"  .25e2xyz"` is
  `25.0`, `"0x1A"` is `0.0` because PHP reads no hex here, `"INF"` and `"NAN"`
  as text are `0.0`, an underscore stops the parse so `"1_000"` is `1.0`, and
  `"1e400"` answers `INF` rather than saturating. This is the same parser
  `(int) $string` routes float-form prefixes through, so the two spellings
  cannot disagree about what a string holds.
- **Coercion at a declared `int`, `float` or `bool` return.** This is not the
  `(int)` cast: returning `null` from a function declared `int` is a
  `TypeError`, and returning `5.7` deprecates before truncating. A non-numeric
  string, an out-of-range or non-finite float, a container, and an object each
  raise, with the object naming its class. `declare(strict_types=1)` performs no
  coercion at all and is refused rather than answered with the weak-mode result.
- **`__toString`.** `echo $obj`, `"x" . $obj` and `(string) $obj` call the
  method when the class is statically known, and raise php-src's
  `Error: Object of class X could not be converted to string` when the class
  provably has none. A subclass that overrides `__toString` leaves the site
  undecidable and is refused.
- **`count()` of a boxed value.** Raises php-src's own `TypeError`, which names
  a boolean by VALUE (`true given`, not `bool given`).
- **Relational comparison against a boxed value.** PHP compares without
  converting, so `"abc" <= 1` is false and `[1] <= 1` is false. A cast-then-
  compare lowering answers both incorrectly. The NaN rule is where relational
  comparison parts company with `<=>`: php answers **1** for `NAN <=> $x` in
  both directions, while `NAN < 1`, `NAN > 1`, `NAN <= 1` and `NAN >= 1` are all
  false. A bool or null on either side outranks that rule — both sides become
  booleans first, and NaN is truthy.
- **A float reaching a string.** `(string) $f`, `"$f"` and `echo $f` share one
  renderer, so all three agree: `-0` for negative zero, `1.0E+20` and `1.0E-7`
  in exponent form, `100` rather than `100.0` for an integral value.
- **`array_keys()` and `array_values()` of an associative array.** Both project
  in php-src's INSERTION order — `["z" => 26, "a" => 1]` answers `z`, `a` —
  never sorted, and never the bucket table's order.
- **`json_encode()` decides list-versus-object by the KEYS.** A keyed array
  whose keys are `0..n-1` IN THAT ORDER encodes as a JSON array, and anything
  else as an object: `[0 => "a", 1 => "b"]` is `["a","b"]`, `[0 => "a", 2 =>
  "b"]` is `{"0":"a","2":"b"}`, and `[1 => "a", 0 => "b"]` is
  `{"1":"a","0":"b"}` — the order counts, not just the key set. This target
  stores a keyed array as a hash whatever its keys are, so the question is
  asked at runtime rather than read off the storage. Found by a differential
  fuzz over the array builtins: `array_unique()` on an array with no
  duplicates leaves its keys `0..n-1`, and the encoder was answering
  `{"0":100,...}` where php answers `[100,20,3]`.
- **`array_unique()` keeps its keys.** `array_unique([1, 2, 2, 3, 3, 4])` is
  `[0 => 1, 1 => 2, 3 => 3, 5 => 4]`, so the answer has holes and
  `array_is_list()` on it is false. Uniqueness is php's default `SORT_STRING`.
- **`readline()` answers `string|false`.** `false` is EOF with nothing read, and
  it is distinct from the two cases that look like it: an empty LINE is `""`,
  and a line ended by EOF rather than by a newline is still that line.
- **The loaded extensions are this BINARY's, not the host php's.** `extension_loaded()`
  and `get_loaded_extensions()` report what the compiled program can actually
  do, which is already the native backend's rule — it answers `curl: no` where a
  host php built with curl answers yes. A wasm module links no native bridge at
  all, so what it reports is what it implements: `Core`, `standard`, `SPL`,
  `json`, `bcmath`, `zlib` and `pcre`. That is not a smaller lie but the same
  rule applied — reporting `date` loaded would make
  `if (extension_loaded('date')) { date(...); }` take the branch and then fail
  to COMPILE, where answering false lets the program build and take its
  fallback.
  The last three are SUBSETS, and are reported anyway. A construct outside the
  subset is refused at compile time naming itself — `bcsub`, `gzopen`, a
  backreference — so a guarded branch that reaches one fails loudly and
  locally, where declaring the whole extension absent would send every user of
  it down a fallback it did not need. The test that pins this list against the
  compiler is what put `pcre` in it: it failed the moment `preg_match` started
  compiling.
  The name must be a literal — the set is settled when the module is built, so
  a runtime name has nothing to resolve against and is refused rather than
  guessed at.
- **Introspection answers THIS MODULE's world.** `get_declared_classes()` lists
  the classes the program declares, in declaration order, which under AOT is the
  whole world there is — so it folds to a constant list rather than consulting a
  runtime registry. On a non-macOS target that list is one shorter than the
  native binary's, because `Pcntl\QosClass` is injected only for macOS, which is
  php-src's own platform guard rather than anything this backend decides.
  `spl_classes()` answers the catalogue the shared contract attributes to
  `ext/spl`, the same list native answers. It is a DIRECTORY of the extension and
  not a claim about what can be instantiated, which is the same reading php gives
  it; `extension_loaded('SPL')` already answers true here, and a program that goes
  on to `new` an entry this backend does not serve is refused at compile time —
  a better answer than a catalogue that hides half of ext/spl.
- **`spl_object_id()` reproduces php's handle numbering, reuse included.**
  Handles are dense integers from 1 assigned at ALLOCATION — `$a = new A();
  $b = new A(); spl_object_id($b)` is 2, which a pool filled on first query would
  get wrong — and a freed handle comes back most-recently-freed-first, so freeing
  `$b` then `$c` hands the next two objects `$c`'s handle and then `$b`'s. The
  handle is kept in a side table rather than in the object header: the payload's
  first word is the class id, read as a whole `i64` by instanceof, method
  dispatch, the release walk and the cycle collector, and sharing it would mean
  changing all of them in step for a value only one builtin reads. The table
  exists only in a module that calls `spl_object_id()`.
- **`is_a()` asks the object, not the call site.** It resolves the name through
  the same runtime table a dynamic `instanceof` uses and answers from the class
  the object actually carries, so `is_a($x, "Derived")` is true for a `Derived`
  reached through a parameter typed `Base`. php's third argument, `$allow_string`,
  makes the FIRST argument a class name rather than an instance — a different
  question with a different answer — and a call that passes it is refused at
  compile time rather than answered with this question's answer.
- **The wall clock is real, and sleeping is not.** `time()` and `microtime()`
  read WASI's `clock_time_get`, which answers nanoseconds since the Unix epoch.
  Nothing about them needed reimplementing — the import had simply never been
  declared, which is the whole reason `time()` was the most cited blocker in
  the example corpus at nineteen. A host that refuses the clock makes both
  answer the epoch, the same shape the file helpers take when no directory is
  preopened. `microtime()`'s two forms are different TYPES rather than two
  renderings: `true` answers a float, and the default answers
  `"0.17535200 1789824612"` — the fraction to eight digits zero-padded, a
  space, then whole seconds, so a clock landing exactly on a second still shows
  `0.00000000`. The flag must be a literal, because it picks the return type.
  `sleep()` and `usleep()` do NOT follow: they need `poll_oneoff` and its
  subscription records, a different import, and are refused rather than turned
  into a busy wait that would burn a host's CPU imitating a function whose
  purpose is not to.
- **`preg_match()` is a SUBSET of PCRE, compiled when the module is built.**
  There is no PCRE to link, so the engine is written here: the pattern is
  parsed and compiled to bytecode at compile time, and a backtracking VM in the
  module runs it. That the pattern must be a LITERAL is what makes a partial
  engine safe — every construct the VM cannot execute is a compile error naming
  it, never a wrong answer at runtime.
  Served: literals and escapes, `.`, character classes with ranges and
  negation, the `\d \w \s` shorthands and their negations, `^ $ \A \z \b \B`,
  alternation, capturing and non-capturing groups, and the greedy and lazy
  forms of `* + ? {n} {n,} {n,m}`, with the `i`, `m` and `s` flags.
  Refused by name: backreferences, lookahead and lookbehind, named,
  conditional and atomic groups, inline flags, possessive quantifiers, unicode
  properties, `\K` and the other PCRE extensions, and the `u` and `x` flags.
  `$matches` is refused too, and deliberately: every other unsupported
  construct announces itself in the pattern, while an array that silently never
  fills would not.
  The engine matches BYTES, which is what PCRE does without `u`. Agreement with
  php-src is measured rather than claimed — a table of 71 cases covering each
  rule, plus a differential fuzz of 1500 random patterns from the supported
  grammar, all matching php exactly. Two budgets are finite: a pattern needing
  more than 4096 backtrack frames or a million VM steps answers `0`, where
  php's own `pcre.backtrack_limit` answers `false`. Both are falsy.
- **DEFLATE is written in the module; the decompressor is exact and the
  compressor is not.** `gzdeflate`, `gzinflate`, `gzcompress` and
  `gzuncompress` are RFC 1951 implemented here, because there is no zlib to
  link. The DECOMPRESSOR handles every block type the format defines — stored,
  fixed Huffman and dynamic Huffman, with its code-length alphabet and repeat
  codes — and is checked against streams zlib itself produced, since the bytes
  a program is handed may well have come from zlib. The zlib wrapper's header
  is validated before inflating, so a string that is not a zlib stream answers
  `false` rather than being decoded as if it were.
  The COMPRESSOR emits one final fixed-Huffman block with lazy LZ77 matching.
  That is valid DEFLATE which any decompressor reads, and it is not
  byte-identical to zlib's: on `examples/gzip`'s input it answers 39 bytes
  where php answers 40, because zlib's windowed buffering splits the long
  match one byte earlier than an unbuffered pass does. On prose it is roughly
  a third larger than zlib, because zlib switches to dynamic Huffman there and
  this does not. Lazy matching is measured rather than assumed: against this
  compressor's own greedy form it is 5.3% smaller on this page, 6.1% on a Rust
  source file, 4.5% on repetitive JSON, and identical on a strictly periodic
  input. A program that stores a compressed blob and compares it against a
  freshly compressed one will therefore differ; one that round-trips will not.
  One more difference has no value attached: php prints
  `Warning: gzinflate(): data error` on STDOUT for a corrupt stream before
  answering `false`, and this target answers `false` silently.
- **Decimal arithmetic is written in digits, not linked.** `bcscale()`,
  `bcadd()`, `bcmul()` and `bcdivmod()` are a reimplementation on this target:
  the native backend links `elephc-bcmath`, and a wasm module assembles one
  self-contained WAT text and links nothing, so every digit of every answer
  comes out of the module itself. A number is one decimal digit per byte and
  every operation is schoolbook, which is slow and exactly right — bc promises
  DECIMAL arithmetic, so a binary representation would have to round somewhere
  php does not. A 20-digit product times a 20-digit product is exact at 40
  digits. Results truncate rather than round (`bcadd("0.005", "0.004")` is
  `"0.00"` at scale 2) and render at the full scale (`bcadd("1", "2")` is
  `"3.00"`). `bcdivmod` answers the quotient at scale 0 and the remainder at
  the call's scale, with the quotient truncating toward zero and the remainder
  taking the DIVIDEND's sign — so `bcdivmod("7", "-2")` is `["-3", "1.00"]`. A
  zero divisor raises php's `DivisionByZeroError` and an operand that is not a
  well-formed decimal raises its `ValueError`, both catchable; php's message
  names the function and the argument position, and this target's is generic.
  Only those four are served: `bcsub`, `bcdiv`, `bcmod`, `bcpow`, `bcsqrt`,
  `bccomp` and the rest have no implementation here and are refused at compile
  time rather than approximated.
- **`fscanf()` serves `%s` fields only.**
  This one is a front-end limitation rather than a backend choice. elephc's
  checker types `fscanf` — and `sscanf` — as `array<string>`, while php returns
  TYPED fields: `%d` gives an int and `%f` a float. No lowering can put an int
  in a string slot, so every numeric conversion is refused by name at compile
  time instead of answering a string php would not have produced. `%s` fields
  are strings in php too and are served. Two stand-ins remain, both for answers
  the type cannot express: a field the line does not supply is the empty
  string, where php answers `null` (so `count($row)` still equals the number of
  conversions), and end of input is an EMPTY array, where php answers `false`,
  which is falsy in the `while (($row = fscanf(...)))` loop php's manual
  writes but differs under `=== false`. The format must be a LITERAL, as
  `sprintf`'s must; a width or a literal separator changes where a token ends,
  which the whitespace tokenizer cannot see, so both are refused rather than
  approximated.
- **The stream tuners answer, they do not tune.** `stream_set_chunk_size()`,
  `stream_set_read_buffer()` and `stream_set_write_buffer()` cannot change how
  this target reads or writes: reads and writes go straight to `fd_read` /
  `fd_write` or to an in-memory buffer, with no layer to size. What php promises
  is not that the size takes effect but what the CALL answers, and that is
  reproduced exactly. `stream_set_chunk_size()` records a real per-stream value
  and answers the PREVIOUS one, defaulting to 8192, so the save/change/restore
  pattern php documents it for works; a closed fd's entry is cleared, because
  WASI recycles fd numbers and php gives a reopened file the default again. A
  non-positive size raises php's `ValueError` and leaves the recorded size
  unchanged. `stream_set_read_buffer()` answers `0` and
  `stream_set_write_buffer()` answers **-1** — measured on php-src 8.5.10 across
  `php://memory`, `php://temp`, a file opened for reading, a file opened for
  writing and `php://stdout`, all five the same. -1 is php's "this stream has no
  such option", which is exactly true here. The NATIVE backend answers `0` for
  the write buffer, so on this line the wasm target matches php-src and native
  does not.
- **The protocol registry is CARRIED, not read.** `getprotobyname()` and
  `getprotobynumber()` answer from a 139-entry table built into the module. The
  native backend opens `/etc/protocols`; a WASI module reaches only what the
  host preopens and cannot open an absolute path outside one at all, so reading
  would mean answering `false` for everything on every normal host. What that
  file holds is the IANA protocol-number registry — a specification, not a
  property of the machine — which is the same reasoning that makes `readdir()`
  report `.` and `..` whatever the host enumerates, and the same grounds on
  which elephc already carries the IANA timezone list. Every one of the table's
  280 spellings and every number from -2 to 257 was compared against php-src
  8.5.10 and agreed. Lookup by name is byte-exact and matches the canonical name
  or an alias: `tcp` and `TCP` are both 6, `Tcp` is `false`. Lookup by number
  answers the canonical name, never the alias. `getprotobyname("ip")` is `0`,
  which is not `false`, so the result stays boxed — a bare int would lose the
  distinction `=== false` asks about. An IP protocol number is an 8-BIT field, and macOS's own
  `/etc/protocols` carries a BSD-local `divert 258` line that no registry
  assigns and Linux does not ship; carrying it would make this module answer
  258 everywhere, including on hosts whose php answers `false`. So `divert` is
  the one entry where this target and php-on-macOS disagree, and it disagrees
  on purpose. More generally, a host whose file spells an entry differently
  will differ for that entry: the module's answer is the registry's.
- **`readdir()` reports `.` and `..`, whatever the host enumerates.** Whether
  WASI's `fd_readdir` yields the two directory links is the HOST's choice and not
  the specification's — wasmtime returns them, node's WASI does not — so a module
  that forwarded what it was handed would list a different directory depending on
  who ran it. php's contract is the POSIX one, both present, so this target
  reports them first and skips them wherever the host produces them too. The
  order of the REST is the filesystem's, exactly as it is under php.
- **`Throwable::getFile()` and `getLine()` report the CONSTRUCTION site.** An
  exception built on line 2 and thrown on line 5 answers 2, as php-src does, and
  the file is the compiled script's own path — the same string `__FILE__`
  yields, and the only one the compiler has, since EIR spans carry a line and a
  column but no filename. A throwable the compiler synthesizes rather than a
  user `new` — a `DivisionByZeroError` from a guard — answers `0`, because there
  is no construction site to report. The line is stamped past everything the
  class declares, so the offset belongs to the RUNTIME class: a subclass with
  properties of its own keeps its line further along, and the accessor reads it
  back through the same class-indexed layout metadata the release walk uses.
  `getTrace()` and `getTraceAsString()` stay an empty array and an empty string
  on both backends, because elephc keeps no call stack to render.
- **A generator suspends by RETURNING, because this target cannot switch
  stacks.** The native backend runs a generator body on a fiber and switches
  stacks at each `yield`; wasm has no such primitive. What it has instead is that
  every function here is already emitted as a state machine — `(local $__state
  i32)` and a `br_table` dispatch loop — so suspending is "write the state and the
  locals to a heap frame and return", and resuming is "read them back and re-enter
  the loop". `g(...)` becomes a wrapper that allocates the frame and answers it as
  the `Generator` without running a line of the body, which is what php does too;
  the body becomes `<g>__resume(frame) -> (has_value, value)`, reached through an
  if-ladder over a dense generator id rather than a `call_indirect`, the way every
  other such dispatch here is resolved.
  The frame's layout is the one `codegen_support::runtime::generators::frame`
  already wrote down — an 80-byte header whose `state_idx` and `resume_fn_ptr`
  were reserved for exactly a state-machine target and had no other user — with
  the function's locals packed one 8-byte word each after it.
  Two things follow that reading would not have suggested. The dispatch loop has
  to SPLIT: it mapped one arm per EIR block, and every yield sits mid-block with
  code after it, so a block index cannot name the point to resume at — blocks now
  become one segment per yield plus a tail, and a function that never yields gets
  exactly one segment per block, so its WAT is unchanged. And the frame needs its
  own heap KIND: the values it owns live in its header and its local area, while
  the object release walk only knows declared property slots, so kind 7 releases
  the header cells always and the spilled locals only while still SUSPENDED —
  `FLAG_TERMINATED` records which of the two happened, and doing both would be a
  double free.
  Keys, `getReturn()` and the hand-called protocol followed. php's key for a plain
  `yield` is not a yield count but `largest_used_integer_key + 1`, so an explicit
  `yield 5 => $v` MOVES what the next plain one answers while a string key leaves it
  alone — that rule is the whole content of the frame's `auto_key_counter`.
  `getReturn()` needs no method body, because a `Generator`'s methods are not
  bodies: they read the frame, and a generator's `return` is what this one reads
  rather than what the call answered.
  `send($v)` is the reason `current()`, `key()`, `valid()` and `next()` are here
  too: calling the protocol by hand means nothing has primed the generator, and php
  primes on the first call of any of them — `FLAG_REWOUND` is that record. What
  `send` passes in is what the suspended `yield` EVALUATES TO, which is decided by
  the resume and not by the suspension, so the read sits at the top of the segment
  the yield resumes into.
  Two mistakes worth naming. Terminating stored `FLAG_TERMINATED` over the flags
  word instead of OR-ing it, clearing `FLAG_REWOUND` — so the next call decided the
  generator had never started, primed it again, and re-entered a stale resume
  point, printing the body's tail twice. And a yield spills every local whatever its
  state, so a value the body had already RELEASED was released a second time when
  the generator was dropped mid-suspension; that cycles the allocator's free list,
  which is a HANG rather than a crash — `$g->next(); $g->next();` answered correctly
  once and never finished a thousand times. Releasing inside a generator now zeroes
  the local.
  `yield from` is the one construct whose resume point is ITSELF: while the inner
  generator has values the outer comes back to the same instruction, forwards the
  inner's key and value, and suspends again; when the inner runs out the outer does
  NOT suspend but falls through into the rest of its body in that same resume, and
  the expression evaluates to the inner's `return`. Two things make that fit. The
  delegation gets a segment of its own — re-entering a segment that also held the
  `inner()` call would build a fresh generator on every resume — and a segment that
  does not end its block already falls into the next one, so nothing has to name
  where to continue. A value sent to the outer while it delegates belongs to the
  inner's suspended yield, so the frame word is MOVED down before the inner is
  resumed; php forwards it the same way.
  What that exposed is bigger than delegation. `yield from <array>` is desugared
  into an ordinary `foreach` before the backend sees it, which makes it the same
  construct as a `foreach` WRITTEN inside a generator body — and an iterator's
  source and cursor live in locals this backend makes up rather than in
  `function.locals`, so the spill walked straight past them and the cursor reloaded
  as whatever the previous resume left. `foreach ([1, 2, 3] as $v) { yield $v; }`
  yielded one real value and then counted forever, which is ordinary PHP that no
  probe had covered. The frame carries three words per iterator now — source, cursor
  and runtime kind — reserved in the order the iterator prepass walks them, because
  two derivations of one layout is how a spill and a reload come to disagree. The
  delegated yields that desugaring emits also forward their keys verbatim: php
  leaves the outer's `auto_key_counter` alone through a delegation, so a plain
  `yield 30` after `yield from [10, 20]` answers key 0, not 2.
  Two ownership rules were wrong in the slices before this one, and both surfaced
  three generators later rather than where they were made. A yield frees its operand
  to close the leak of a boxed copy, but `str_concat` and `load_local` are both
  `maybe_owned` — so `$a = "aa" . "bb"; yield $a;` freed the SLOT's storage, the
  release walk freed it again, and the free list came apart. The rule is now
  structural: free only what the value's defining instruction ALLOCATED, and only
  when this yield is its only use. And a yield whose result nothing reads still has
  to take the sent value out of the frame, because leaving it parked leaked a cell
  per forwarded element — 3 pages became 26 over 20000 delegations.
  Exceptions across a suspension came last, and all three shapes came together
  because they share one word: the armed handler. `$__handler` and its per-`try`
  save slots are locals this backend makes up — the same kind of state as an
  iterator's cursor — so a `try` around a yield had nothing armed on the far side of
  it. The frame carries them now, and the resume must NOT then re-arm nothing: the
  dispatch loop's `$__handler = -1` is the wrapper's job on the first entry, and
  running it again on a resume threw away the `try` the body was still inside.
  Which is also why the sentinel is -1 rather than 0 in the frame: segment 0 is a
  legitimate handler.
  `throw()` raises INSIDE the body at the point it is waiting, and there is exactly
  one place on this target that can be — the top of the segment the yield resumes
  into, which sits inside the landing pad. The exception is parked in `sent_value`,
  since nothing can be sent and thrown at once, and parked as the OBJECT rather than
  boxed: a boxed cell was thrown as though its tag word were a class id, so no
  `catch` clause matched and the body rethrew to its consumer.
  An exception that LEAVES a generator also finishes it — php answers false from
  `valid()` afterwards — and nothing else records that, because the return
  terminator is what normally sets the flag and unwinding skips it. So a generator
  body is guarded even when it catches nothing of its own.
  `rewind()` refuses on "has it been ADVANCED", which is not "has it started": a
  rewind of an untouched generator primes it, one at the first yield is a no-op, and
  one after `next()` raises php's `Exception`. `FLAG_REWOUND` cannot answer that,
  because priming sets it and priming is what the first case does.
  KNOWN GAP, and not a generator one: an exception that escapes a frame strands
  everything that frame owned. Measured on ordinary functions — a string, an array
  and an object each leak when a `throw` unwinds past them — so a generator dropped
  by an escaping exception leaks its whole frame. The releases live on the return
  path only.
  Still refused, by name: a by-REFERENCE local in a generator body, whose ref cell
  is storage that has to outlive the call rather than a value to copy.
- **An array stored into a declared `array` property is a COPY, not a pointer
  write.** `private array $items = [];` followed by `$this->items = [1, 2, 3];` is
  as ordinary as PHP gets, and it was refused: a declared `array` property is
  `array<mixed>`, which this target lays out as boxed cells, while the literal is
  `array<int>`, eight-byte slots. Two layouts, so the assignment converts — through
  the same `__rt_array_widen_to_mixed` the call boundary already used for the pair.
  The conversion answers a FRESH array, which is why the store must not retain it
  the way the ordinary container path does: it is already the one reference the slot
  takes, and increfing it left one array behind per assignment. php's by-value array
  semantics fall out of the copy for free.
  Still refused by name: a HASH assigned into a declared `array` slot. php has one
  array type and this target has two layouts, so `["k" => 1]` into an `array<mixed>`
  slot would have to carry its kind into every later read of that property.
- **A property read through a `mixed` receiver is a ladder over the class id.**
  A boxed receiver names no class until run time, so `$o->n` where `$o` is
  `mixed` is decided the way a mixed METHOD call already was: unbox, read the
  class id the object header carries, and compare it against every class in the
  module that has the property. Each arm reads one class's slot and boxes it,
  because the instruction answers one Mixed cell whatever storage was behind it —
  and the load is BORROWED, because `__rt_mixed_from_value` copies a string and
  increfs a container, so an owned load would leave the object's own slot with
  nothing to give back.
  `stdClass` is why the ladder has two kinds of arm. It is in every module's
  prelude and answers from its dynamic-property hash rather than a slot, so a
  ladder that could only read slots would have refused every mixed property read
  there is.
  php's two diagnostics are both here: a receiver that is not an object warns and
  answers null, naming the type — and for a bool the VALUE, `on true`, which is
  why those two words cannot come from the type table every other tag reads —
  while an object whose class no arm claims gets `Undefined property: C::$n` from
  the runtime class-name table.
  The WRITE is still refused by name. It needs php's `Error` for assigning to a
  property of a non-object, which is a different diagnostic from the warning a
  read gives.
- **`$obj->$name` goes to the property hash, and only where nothing else
  could answer.** The name is a run-time string, so nothing at compile time can
  say which property it means. Two conditions make the hash the right
  destination: the class must carry one, and it must declare no property slots at
  all — with a slot present the name might name THAT, and deciding which needs a
  chain of string compares this backend does not emit. `stdClass` is the shape
  this is for, and every property of it is dynamic. The literal spelling
  `$obj->name` and the computed one reach the same hash through the same key
  helper, which is the point: they differ in where the name's bytes come from and
  in nothing else. A miss answers null and says so —
  `Warning: Undefined property: Bag::$k1`, rendered from the receiver's own class
  id rather than a per-class literal, so a program with fifty classes lays out no
  extra bytes. Except inside `isset`, `empty` and `??`: those ASK whether the
  property is there, php answers silently, and a null test on the read's own
  value is the shape only those three produce.
- **`unset($obj->typed)` cannot remove the slot, so it MARKS it.** php removes
  the property; a fixed 16-byte slot has nowhere to go. The slot's SECOND word
  becomes the marker instead, carrying the native backend's own uninitialized
  sentinel so the two targets cannot drift — and that word is exactly the one no
  live value can collide with, because every property shape puts a tag, a string
  LENGTH or a container tag there, none of which can equal `PHP_INT_MAX - 2`.
  Three sites then have to agree about it: the unset writes it, `isset()` reads
  it instead of folding to a constant, and a READ of the slot raises php's
  `Error: Typed property C::$p must not be accessed before initialization`, which
  is catchable exactly as php's is. All three ask one function whether an `unset`
  in the module can reach the slot, so a program that never unsets a property
  keeps the folded probe and the unguarded read it had. The previous value is
  released first — a string's bytes and a container's reference belong to the
  slot, and marking over them leaks one per unset, which is four of the five slot
  shapes. An UNTYPED declared property stays refused, as it is on native and for
  the same reason: php's removed-then-read answers `null` with a warning rather
  than raising, and a slot whose declared type has no null cannot hold that
  answer. A tagged slot's store had to widen from four bytes to eight, or half a
  marker would survive a write for `isset()` to find.
- **`isset()` over a property is two questions, and the receiver decides which.**
  `isset($obj->p)` on a concrete receiver folds to a constant, because the gate
  admits it only where the property's write is proved. A `?Node` receiver is
  where it stops folding: `isset($maybeNull->n)` reaches the probe with nothing
  having tested the receiver, and the EIR reads the probe ITSELF as the null test
  — its false arm is the `n`. So a boxed receiver is answered at run time from
  the cell's own tag, which also covers the `?->` chain's hops for free. A STATIC
  property is the opposite difficulty: php distinguishes a typed static before
  its first write, where `isset()` is false and reading it fatals, from one
  holding `0`, where it is true — and a static slot here carries no sentinel to
  tell those apart, because the layout writes bytes for every slot. The native
  backend keeps a marker word and compares it against an uninitialized sentinel;
  carrying that here would change the width of every static slot shape, so the
  DECLARATION is asked instead. A default proves the write, an untyped
  declaration gets php's implicit `null` which is also a written value, and a
  typed static with no default is refused rather than answered. The null-VALUE
  cases are not the probe's at all: `public static ?int $n = null;` answers
  false because the READ that follows says so.
- **An int reaching a `bool` slot is php's truthiness, not a bit copy.** The
  transfer `isset()` needed, and it was the reason the construct was refused two
  layers below the opcode anyone would have looked at: the EIR types the
  construct's result `int` and the slot the ternary around it writes `bool`.
  `isset` only ever yields 0 or 1, for which normalizing changes nothing, and the
  wider rule is the safe one — anything else reaching a bool slot is a value php
  reads as `true`. A one-word slot holding the NULL sentinel is the one collision
  it inherits, and it is the encoding's own.
- **`$a?->b?->c` turns on the hops PAST the first.** The first hop has a
  concrete receiver, so the `isset` probe each hop lowers to folds to a constant:
  the gate admits it only where the property's write is proved, and every
  instance of that class has the slot. The second hop's receiver is `$a->b`,
  typed `Node|null` and stored as ONE boxed cell — and a boxed receiver is a
  layout this target had no way to name, which is what kept the whole construct
  refused. The layout is the union's single class, read through the same
  function `->` and `?->` already read it through, so the three cannot drift
  apart on which class they mean. NULL is the separate question, and the answer
  is not to test for it: the chain's own branch has already tested each link and
  carries the read on the false arm, so the gate REQUIRES that branch — a boxed
  receiver no branch has cleared is refused rather than answered `true` where
  php answers false. A union naming two classes is refused by the checker before
  the backend sees it.
- **`??` hands over the slot it spilled into, and takes over the string it
  boxes.** Two leaks, both in the same join and both reachable from any `??`
  over a boxed value. The operator spills its left operand into a hidden slot,
  reads it back at the join and clears the slot — and clearing it gives up the
  slot's reference, so the read has to TAKE OVER rather than retain. Retaining
  leaked one Mixed cell per evaluation, 40 bytes, measured as
  `if (($o->tag ?? 1) > 0) {}` growing 3 → 51 pages over 1k → 80k iterations
  while the same read without the `??` stayed flat. The second is the one shape
  a string left operand adds: boxing a string here COPIES its bytes, where a
  refcounted target would take a reference and the EIR therefore emits no
  release, so the copy the box replaced had no owner left. The box now frees it,
  and only where the operand is a copy of its own — an `Op::Acquire`, nothing
  else releases it, and this box is its only use. The same shape still leaks 48
  bytes an evaluation on the NATIVE backend, where the box increfs instead of
  copying and the missing release is the EIR's.
- **`Enum::from()` raises php's own `ValueError`, message included.** The hit
  answers the case SINGLETON, so `Suit::from("H") === Suit::Hearts`. The miss is
  the part that kept `from()` refused here for a while: php names the value that
  did not match, quoted for a string backing and bare for an int one, so the
  message is a concatenation rather than a constant and the fixed-message raise
  path had nowhere to put it. It is a real PHP exception, so a
  `catch (ValueError)` receives it and a `catch (Error)` ahead of that one wins,
  exactly as php orders them. What it inherits is this target's existing limit on
  an UNCAUGHT throw: the fatal reads `PHP Fatal error: Uncaught exception`
  without the class, the message or a trace — which is what a hand-written
  `throw` already prints here, so `from()` is no worse than the `throw` a program
  would have written instead.
- **Argument unpacking is three conversions, and the proof is not the loop's.**
  `f(...$args)` reads each element as `int|null`, because an element read might
  miss, and hands it to a declared `int` parameter that has one word for it: the
  pair narrows to its payload and the tag is dropped. That is the native
  contract rather than a shortcut — native keeps a tagged value's payload in the
  integer result register and its tag in a separate one, so a concrete parameter
  never looks at the tag, and a genuinely null element at a non-nullable
  parameter is a `TypeError` the checker raises before either backend sees it. A
  FLOAT element is read at its own width or not at all: every bit pattern is a
  valid float, so the getter's NaN-for-a-miss is a value a real element could
  equal, and only a read the audit can PROVE in bounds is admitted. The spread's
  proof is its own shape — it tests `count >= k` once and then reads constant
  indices, where a `foreach` tests `i < count` per iteration — so the audit
  carries both and neither subsumes the other.
- **A nullable INSTANCE property is the static slot at a different base.** The
  same two shapes — a tagged `(payload, tag)` pair for a nullable scalar, one
  cell pointer for a nullable container — laid into the object's 16-byte slot,
  and the same trap: `public ?int $v = null;` must write tag 8, because the
  zeroed slot reads back as the integer `0`. A tagged slot also gets its own
  byte in the class's release descriptor rather than sharing the boxed one: its
  first word is the PAYLOAD, so a walk that released it would hand an integer
  to the refcount dispatcher and free a block for any payload that happened to
  land inside the live heap window.
- **A nullable parameter takes two words, and the call site builds them.**
  `function f(?int $n)` receives the `(payload, tag)` pair, and an argument
  arrives as one value: `f(5)` hands over a bare long and `f(null)` a Void. The
  conversion is the transfer table's, the same one an `array<T>` reaching an
  `array<mixed>` parameter goes through, and the call boundary now asks for it
  instead of requiring the argument to have arrived in the parameter's shape.
  An int, a float and a bool each take their own tag, and a float's payload is
  its BITS. A STRING is refused there rather than guessed: its `(pointer,
  length)` does not fit one payload word. A nullable INSTANCE property is a
  separate gap and still refused — `private ?int $v = null;` has no object-slot
  shape yet, the way a static one had none before this.
- **A nullable static property is one of two 16-byte shapes.** `?int`, and
  every other nullable SCALAR, is the `int|null` pair written flat: the payload
  in the first word and the Mixed tag in the second. Its `= null` default is
  therefore not sixteen zero bytes — tag 0 is the INTEGER tag, so a zeroed
  tagged slot reads back as `0` where php reads null, which is the one way this
  layout goes wrong without saying so. `?array`, `?SomeClass` and `mixed` hold
  one CELL pointer instead, the shape a function's `static $x` already uses,
  and there the null pointer IS php's null — the same value an unwritten slot
  carries. A boxed static whose default is anything other than null is refused:
  it would need a heap cell built before `main`, and the zero that would be
  laid down instead reads back as null. An assignment does not have to arrive
  in the slot's shape — `C::$maybe = 5;` hands over a bare long — because the
  store asks the same conversion table every other transfer asks.
- **`clone` copies the values and shares only what can be shared.** The copy's
  layout is read from the same per-class descriptor the RELEASE walk reads, so
  "which slots hold a reference" is stated once and the two cannot drift: a
  clone that retained a different set from the one `__rt_decref_object`
  releases would either leak or free a child both objects still point at. An
  object property is deliberately SHARED — `$copy->inner->x = 42` shows on the
  original, which is what shallow means — and an array property is
  copy-on-write, so appending to one leaves the other at its length. A STRING
  property is the exception: a string block on this target is copy-on-acquire
  rather than refcounted, freed outright by its release, so the copy takes its
  own bytes. The dynamic-property table is copied rather than shared too, or
  writing an undeclared property on the copy would appear on the original.
  `__clone()` runs afterwards, on the copy, as its own call.
- **`<=>` is the ordering the relational operators are predicates over.** Two
  longs compare AS LONGS, so `PHP_INT_MAX <=> PHP_INT_MAX - 1` is 1 where
  converting both to doubles first would answer 0. A float on either side makes
  it a double comparison, and php answers **1 for a NaN in both directions** —
  `$nan <=> 1.0` and `1.0 <=> $nan` are each 1 — where a plain `gt - lt` would
  answer 0. A boxed operand carries its own tag and is not cast first: a
  boolean or null on either side makes BOTH sides booleans, an array outranks
  any scalar, and a non-numeric string against a long renders the long as a
  string and compares bytes. `cmp(int, cell)` is `-cmp(cell, int)`, which is
  how a plain long on the left reaches the same helper. The `<=>` on two
  STRINGS is refused before the backend sees it: elephc's checker requires
  numeric operands there, on both targets.
- **`isset($a[$i])` asks two questions and answers one.** An offset that is not
  there and an offset holding null are both false, which is why this is not a
  bounds test alone. It is a bounds test for most storages, though: only a
  Mixed-cell array can hold a null in bounds, so an `array<int>`, an
  `array<string>`, an `array<float>` and an array of nested arrays are settled
  by the range check, and no cell is allocated to ask. A negative index is
  false, as php says, and so is any index into an empty array.
  `array_key_exists($i, $a)` is the other half — it asks only whether the
  offset is there, so a stored null answers TRUE where `isset` answers false —
  and over an indexed array that is the range check by itself, because the keys
  of one are `0..count-1` with no holes. A non-integer key to
  `array_key_exists` on an indexed array is refused at compile time rather than
  answered: php coerces a numeric string there and this target does not.
- **A function's `static $x` is one cell, keyed by the function.** The
  initializer runs on the FIRST call and never again; every later call finds
  what the last one left. The guard is a separate flag word rather than a test
  of the value, because `static $n = 0;` and `static $on = false;` leave bytes
  indistinguishable from an untouched cell — a value-based guard would reset the
  variable on every call, which is the one thing a static local exists not to
  do. The cell is keyed by `(function, variable)`, so `a()` and `b()` each
  declaring `static $n` get two counters, and two methods of one class do too;
  an instance method's static is shared by every instance, as php's is. The
  widths are the static property slot's — int, bool, float, string, array,
  and a boxed cell for the Mixed case that ordinary `$n = $n + 1` produces,
  since checked arithmetic widens on the chance of an overflow. A static
  holding an ARRAY survives being appended to: `__rt_array_grow` frees the old
  block and answers a new pointer, so the cell takes it; the native backend
  does not do this yet and answers only the last element written. Closures
  carry statics too.
- **A typed property with no default.** Reading one before it is written is
  `Error: Typed property C::$p must not be accessed before initialization`, and
  this backend has no sentinel for it: the allocator zeroes the slot and zero is
  a legitimate `int`. Such a read is refused unless the constructor writes the
  property unconditionally, which removes the question rather than answering it.
  The same reasoning retires PHP's implicit `= null` on an untyped property: once
  the constructor always overwrites it, no reader can observe the null, which is
  what makes `class Node { public $value; public $next; }` constructible here.
  Inside the constructor the store must be shown to come FIRST, which is a
  question about the individual read rather than about the class:
  `__construct(string $n) { $this->name = $n; echo $this->name; }` is admitted,
  and a read placed before the store is not.
  An ABSTRACT declaration cannot be instantiated, so what decides there is what
  its CONCRETE descendants do: with every one of them initializing the slot, no
  instance exists whose read could raise. One descendant that does not brings the
  refusal back for the whole hierarchy.
- **Reading an untyped property.** The read is an ownership question, not an
  opcode one. `ir_lower` stabilizes a borrowed result with an `Op::Acquire` and
  skips that when the result is already owned — which is what an untyped property
  gives — so borrowing unconditionally frees the cell the object still points at,
  and retaining unconditionally leaks one reference per read of a declared
  `string` or `array` property. The load asks the value which it is.
- **Writing a container after iterating it.** `foreach ($h as ...) {}` followed
  by `$h["c"] = 3;` is accepted: the iterator is dead by then. Only mutations the
  loop itself can reach are refused, since those have no PHP snapshot to write
  against. An iterator's live range ends where its own `IterStart` runs again, so
  a `foreach` nested in a `while` does not make the whole outer body untouchable.
  Refusal covers the mutations that are not written at the loop: every in-place
  builtin (`sort`, `shuffle`, `array_splice`, …), a call that RECEIVES the
  container — whether its parameter is declared `&` is not visible at the call
  site — and any reference bound to it, since an alias has its own slot and would
  otherwise pass every slot-keyed check.
- **Rendering an `array<int>` element.** A read of one answers an int-or-null,
  and PHP renders the null arm as the empty string, so `echo $a[0] . "|" . $a[1]`
  is exact — including the miss, which warns and contributes nothing.
- **`gettype()`.** php-src's historical spellings, not the type names PHP 8
  prints elsewhere: an int is `integer`, a float `double`, a bool `boolean`, and
  null `NULL` in capitals. A settled type answers at compile time; a boxed one
  reads the cell tag. A RESOURCE answers `resource` or `resource (closed)` from
  the stream's own closed state (the box's stamp, the per-open record, the
  memory-stream descriptor or the standard-stream mark), and `is_resource()`
  answers false for a closed one. The check reads the DECLARED type rather than
  its codegen representation, since a resource represents as an integer and
  would otherwise answer `integer`.
- **`Foo::class` and `self::class`.** Resolved at compile time to a data-segment
  address, since the EIR already carries the class. `static::class` is refused:
  late static binding resolves it from the CALLED class, which this target does
  not forward, and answering the defining class instead would be wrong for every
  subclass.
- **`round($value, $places)`.** Not `round($value)` with a default argument.
  Scaling is inexact — `0.285 * 1e10` is `2849999999.9999995` — so php-src
  extracts the integral part and then repairs the extraction, which is why
  `round(1.005, 2)` is `1.01` and `round(9.995, 2)` is `10`. Scale-round-unscale
  gets both wrong. Transcribed from php-src 8.5's `_php_math_round` and validated
  at 1420/1420 over the halfway values, the classic traps, the 1e15 and 1e-15
  boundaries, both signed zeroes and 1200 random values across 24 orders of
  magnitude. A precision outside php_intpow10's exact 0..22 table is refused
  rather than answered nearly-right.

Known shared defect, measured and not yet fixed: PHP's `null` is dropped when a
possibly-missing array element crosses a **function return**. A direct read keeps
it — `$v = $a[5]; $v === null` is true — but `function pick($i) { return
$a[$i]; }` is typed `int`, so `pick(5) === null` answers false on BOTH backends
where php-src answers true. The checker infers the read as `int` while the EIR
produces `int|null`, and the signature is what the call site believes. Anything
reading that type inherits the wrong answer, `gettype()` included; patching it in
one consumer would hide it from the others.

**A Mixed value rendered as a string through a `??` merge** is admitted. Such a
cast is accepted when every consumer is a place PHP renders a string — echo,
concat, interpolation, `strlen` — and `echo $x ?? "d"` looked like it had no
consumer at all: the merge parks the value in a hidden slot and reads it back in
the merge block, so the cast's only direct uses were the `acquire`/`release` pair
the check rightly ignores, and "no string consumer" was indistinguishable from
"no consumer". The walk now follows an `acquire` to its result and a `store_local`
to every LOAD of that slot. Following every load is what keeps it sound: one load
reaching a non-string context still refuses the whole cast. Unblocks
`examples/union-types`.

**`foreach ($a as &$x) { $x += n; }`** writes back through the cell. `$x + 5`
types Mixed because the add can overflow into a float, while the cell it writes
through is the array's own `int`. The emitter already handled that narrowing; the
capability gate was the only thing refusing a store the backend could perform.
The native backend answers this shape correctly, so it was a WASM-only gap. What
it inherits is the EIR's widening gap rather than a new one: on a REAL overflow
the value is a float and narrowing it into an `int` cell is wrong, which the
native backend does there too — refusing the whole shape to avoid that would cost
every ordinary by-reference accumulate. Unblocks `examples/foreach-ref`.

**A NULLABLE SCALAR parameter** takes both the `null` literal and a concrete value.
A `?int` is an inline two-word `{payload, tag}` slot, so the literal `null` —
which arrives as `Void`/`I64` — is a conversion rather than a copy, and had no
classification at all. Null is tag 8 with an unread payload; a concrete value
becomes the payload and names its own tag.

What made that correct rather than merely compilable is the second half:
`is_null` required the operand's php type to literally be `TaggedScalar`, but a
`?int` PARAMETER carries its nullability in the declaration instead, so the test
fell through to a `statically non-null` fallback and answered false over a value
whose tag said 8 — `describe(null)` printed `NULL:` where php-src prints
`NULL:null`. The tag is the truth for any two-word scalar whatever the static
type calls it, which is the same correction the pointer arm needed. Unblocks
`examples/functions`.

**`Enum::cases()` and `Enum::tryFrom()`** are open-coded against the case
singletons. The refusal was `missing method body Color::cases`, which was true
and beside the point: PHP synthesizes both for every enum, so there is no
function to find on either backend. They are emitted directly instead — the same
treatment the `Throwable` accessors get — and audited against what the emitter
produces rather than against a body that will never exist.

`cases()` materializes every case in DECLARATION order into a pointer-slot array
under `value_type` 4, which is an ordinary `array<Object>`: `count()`, `foreach`
and an indexed read all reach it with no special case. `tryFrom()` walks the
cases as an equality ladder over the BACKING value and boxes the winner under
Mixed tag 6, a miss boxing null under tag 8 — which is what makes `?? Default`,
`is_null` and `===` answer the way php-src does. For a string-backed enum the
length is compared first and separately, since the byte comparison reads the
case's length from the needle and would otherwise run past a shorter one.

`from()` stays refused: it must raise php-src's `ValueError` naming the enum and
the offending value when nothing matches, and answering it without that raise
would turn a fatal into a wrong value. Unblocks `examples/enums`.

**An element that is itself an ARRAY or an OBJECT** can be read out of an indexed
array. This was refused because of the MISS rather than the hit: every accepted
element type had somewhere to put PHP's missing-key null — an int became a tagged
scalar, a bool/string/mixed became a Mixed cell — and a raw container pointer
appeared to have nowhere. It does: pointer 0, the same null the native backend
produces, and one no live array or object can collide with since both are
allocated.

Three things had to hold together. The getters had to survive a null SOURCE, now
that a read chained onto a miss passes one — they were loading the length straight
off the pointer, which for 0 reads address 0, valid linear memory in wasm, so it
would have answered from whatever was there rather than trapping. PHP's two
diagnostics had to be told apart, which is decided by the source and not the
result, since both give null: a missing key off a real array is `Undefined array
key N`, and any offset off a null is `Trying to access array offset on null`. And
`is_null` had to stop answering from the static type, whose `statically non-null`
fallback the EIR cannot back — a missing element of `array<array<int>>` is typed
`array<int>`, the null having been dropped at that boundary, so a pointer that IS
0 called itself non-null.

The last of them separated reading memory from reading FREED memory. The getter
hands its pointer back BORROWED while the EIR types the result `own=owned` and
emits a matching `release`, so without a reference of its own that release dropped
the PARENT's: `$g[1][0][1]` freed a child its parent still pointed at, where
`$x = $g[1]; count($x)` did not, the chained form having no `acquire` to balance
it. Verified balanced by measurement, not by reading: 20000 build-then-read
iterations hold at 3 wasm pages, exactly like the same loop without the read,
while a loop that deliberately retains every child grows to 32.

WRITING one back works the same way, with one addition the read does not need:
the slot's previous occupant is a refcounted child, so overwriting it has to
RELEASE it. That release happens AFTER the store, which is what makes
`$a[0] = $a[0]` safe — the incoming pointer is already increfed, so a
self-assignment nets to no change instead of freeing the value mid-write.
Measured the same way: 20000 overwrites of one slot hold at 3 pages against the
retaining loop's 32. Together these unblock `examples/nested-arrays` and
`examples/cow`, the latter matching php-src's copy-on-write output byte for byte.

Two CONSUMERS of that null needed the same treatment. Writing through the null was
the worse one, because it answered rather than stopped: php-src AUTOVIVIFIES
silently, building a fresh array, where this backend exhausted memory treating
address 0 as an array header. The setters now build the array php-src builds.
Calling a method on the null terminated either way but named the wrong thing —
`Invalid callable dispatch`, the dispatch ladder's fallthrough trap, instead of
php-src's `Call to a member function hi() on null`. A raw object pointer used to
be non-zero by construction, so nothing checked; the native backend already
answered this correctly from the same EIR, so the guard is parity rather than
caution.

A second audit round found two more, both in the round-one fixes. The
null-receiver check had been fused to the dispatch load, which misses the
open-coded `Throwable` accessor — that path resolves and RETURNS before
dispatching, so `$e = $a[9]; $e->getMessage()` read address 0 plus the property
offset, printed an empty message and carried on. The check is now separate from
the load and runs before every path. And a container write with a NEGATIVE index
leaked: the setters reject one, because php-src stores a negative KEY there and a
dense array has no slot for it, but the caller increfs the child before the call,
so returning without storing stranded it — 24 wasm pages over 20000 such writes,
now 3.

A third and fourth round pushed on the same seam, which is where the null that
the EIR drops meets consumers that trust the type. `count()` of the missed
element answered `4295050542` off address 0 and carried on, where php-src raises
`TypeError: count(): Argument #1 ($value) must be of type Countable|array, null
given` and terminates; it now raises exactly that. And `tryFrom`'s needle width
was read off the case list rather than the enum's declared backing type, so
`enum E: string {}` — a backed enum with no cases, which is legal PHP — took the
int path and left half a string on the operand stack.

Reading a PROPERTY through the missed element was the last of that seam. It used
to answer a bare `1` off address 0 with no diagnostic at all; it now emits
php-src's own `Attempt to read property "age" on null`, in php-src's order, and
substitutes a defined value instead of whatever the data segments start with. The
VALUE is still not php-src's — the read evaluates to null there, and the EIR types
the result a non-nullable `int` — so the substitute is the same null sentinel the
NATIVE backend leaves in that slot, which puts the two backends in exact agreement
on a null neither can type. A string property answers the empty string and a
container property a null pointer, both of which ARE php-src's answers.

The warning helper lives in the command runtime, so a reactor/library module —
which has no `main`, no stderr contract and no `__rt_warn_*` at all — emits the
plain load exactly as before. That is the same line the warning-producing indexed
read already draws.

Writing PAST the end is a known shared gap that neither of these caused. PHP
treats `$a[3]` on a one-element array as a SPARSE key, so `count()` is 2; a dense
representation with no occupancy bit fills the gap with nulls and answers 4.
Measured identically on the scalar setter that predates this work, so the
container setter matches it rather than adding a second wrong answer.

**A method call on an INTERFACE-typed receiver** dispatches on the runtime class.
This was refused as an `unknown receiver class`, which named the wrong problem: an
interface is not an unknown class, it is a class-less one. It declares no storage
and owns no body, so there is nothing to call into — but the object that arrives
carries its real class id in its own header, which is exactly what PHP dispatches
on. The callee is therefore the closed set of concrete implementors, and that set
is enumerable at compile time, so these calls now go through the same class-id
if-ladder that already serves virtual calls, with the implementors as its arms.

Membership is walked in both directions, because PHP hands a class its parents'
interfaces and an interface its parents' methods: `class C extends B` where `B
implements J` and `interface J extends I` makes a `C` a legitimate `I`. Reading
only a class's own `implements` list would miss every implementor that inherits
the obligation. Enum cases and anonymous classes are ordinary implementors here.

Two shapes stay refused, both because the implementors cannot share one stub, and
both by the AUDIT rather than by the stub emitter — the emitter skips an interface
it cannot serve, so an audit that accepted either would leave the module calling a
function that was never defined. An interface with no concrete implementor has no
arm to select. And an interface method with no declared return type lets each
implementor pick its own, so a `void` body and an `int` body can differ in return
ARITY and unbalance the wasm stack; that is caught even when the call discards the
result. Unblocks `examples/intersection-types`, `examples/anonymous-classes` and
`examples/enum-methods`.

**A subclass argument** may bind to a parameter that declares one of its
ancestors. The capability audit compares an argument's representation against the
parameter's, and two object types with different class names read as two
different representations — so `look(Base $x)` called with `new Kid()` was
refused, though PHP's whole inheritance story is that the call is legal. The
refusal made a representation claim that is not true: an object is ONE pointer to
a header naming its own runtime class, which is exactly why `instanceof` and
virtual dispatch both answer off the value rather than the static type. Two object
pointers are therefore copy-compatible at the physical layer, and whether a given
class may stand in for another is decided where the hierarchy is in scope — in
the audit, which walks `parent` links and so admits a descendant and nothing
else. An interface-typed parameter is a different question, answered by the
interface dispatch described above rather than by this rule. Verified byte-identical to
php-src 8.5.6 for inherited field offsets under a subclass that appends fields of
other representations, virtual dispatch of an overridden method through an
ancestor-typed parameter, a write through such a parameter landing on the base
field, three-level hierarchies, and one callee body serving both classes across
repeated calls. Unblocks `examples/instanceof`.

**By-reference container parameters** are supported. A by-ref argument arrives as
a ref-cell pointer, so the callee loads it with `LoadRefCell` rather than
`LoadLocal`; the writeback recognised only the latter, so a callee that MOVED the
array — `$a[] = 41` growing it past its capacity — had nowhere to put the new
pointer and dropped it. That answered `106808` where php-src answers `2`, and the
`$a[0] = 41` form answered `0` for `1`. Writing the pointer THROUGH the cell when
the slot is ref-bound repairs appends, offset writes, associative keys, wholesale
reassignment, several by-ref parameters at once, nested by-ref calls, repeated
calls and by-ref callees that also return a value — all verified byte-identical
to php-src 8.5.6.

One narrower shape stays refused, and it is a shared defect rather than a WASM
one: a callee that **replaces the container's representation**. `$a[] = $i` where
`$i` came from `$i++` appends a `mixed`, because the increment can overflow into
a float; EIR then widens the whole array with `ArrayToMixed` and stores the wider
array back through the cell. The caller receives the new pointer but keeps the
`array<int>` element type it passed in, and reads 24-byte Mixed cells as a dense
i64 buffer. `count()` stays right — the length field IS shared — so nothing
announces the mismatch. The native backend prints the same raw heap addresses
from the same EIR, so the gap is upstream: the callee's post-condition never
reaches the call site's type facts. This target refuses the call rather than
answering garbage.

A shared front-end bug surfaced while measuring this and is fixed: an
`if`/`elseif`/`else` chain whose FIRST condition folded to false propagated the
`else` branch's constants, ignoring the elseifs entirely. Both backends then
answered from the wrong branch — silently, since the branch itself was lowered
correctly and only the propagated fact was wrong.

**A dynamic dispatch is no longer vetoed by an unrelated namesake.** A `mixed`
receiver names no class, so the ladder's arms come from every class declaring the
method — and the audit then demands that every one of them lower, which let a
bystander refuse a whole program. Two filters now narrow that list before the
audit sees it. The receiver's STATIC type is the first: an `int|Money` receiver
admits `Money` and its subtree, so a prelude `DateInterval::format(string)` is not
an arm of `$money->format()`; a narrowing that empties the list is treated as the
type telling us nothing and the unnarrowed list stands. The call's ARITY is the
second, and it is what saves a plain `mixed`, whose type narrows nothing: a class
php-src would not ENTER with this many arguments cannot be the right arm.

That filter runs one way only. Measured on php-src 8.5.6, a user method accepts
SURPLUS arguments silently — `C::m(int $a)` called with two runs, and
`func_num_args()` sees both — so passing too many disqualifies nothing. An upper
bound here was a miscompile, not a missed refusal: it dropped a class php-src
dispatches to, and the ladder answered `Call to undefined method Money::show()`
for a call that should have printed.

Dropping a class does not mean forgetting it. Each one keeps an arm raising PHP's
own `ArgumentCountError`, with php-src's count and wording — `exactly N` when
every declared parameter is required, `at least N` when a default makes the
counts differ, and (measured) `exactly` still when the tail is VARIADIC. Left to
the ladder's fallthrough it would have reported `Call to undefined method` for a
method that plainly exists; the native backend does worse and says `Call to a
member function show() on null`. Only the location tail and stack trace are
missing, the same convention the other composed fatals follow.

Fixed alongside it: a ladder with two or more `void` arms stored its result from
an empty stack. When every candidate returns `void` the checker types the call
expression `I64 php=null` rather than boxing it, and the arm has to supply the
null the callee never pushed — which the direct and interface paths already did.
The module failed WebAssembly validation outright rather than miscompiling.

**A WIDENED integer arithmetic result counts as an integer** on the far side of a
comparison or a `%`. `$i * $i` on two ints is typed Mixed only because an overflow would
promote it to a float, and narrowing it back is already admitted as exact for every value
that did not overflow — but the predicates deciding the OTHER operand rejected it as
"another conversion of a box", which contradicted that. `$i * $i <= $n` and
`$n % ($i + 2)` were refused for having a perfectly good integer opposite them. Unblocks
`examples/primes`.

**Truthiness of a BOXED value or a float** is lowered. The per-tag ANSWERS were always
exact — `"0.0"` is TRUE because only the single character `"0"` is false, and `-0.0` is
FALSE like `+0.0` — so the refusal was only ever about the one diagnostic PHP raises
here: a NaN is TRUE, and warns `unexpected NAN value was coerced to bool` on the way. A
bare `f64.ne 0.0` would have answered FALSE for it and said nothing, which is why the
float path tests the BITS rather than comparing.

That warning goes through WASI, so a REACTOR module keeps the refusal: answering
silently where php-src speaks is a divergence, not a partial implementation, so the rule
follows the module kind rather than the value.

**A boxed operand of `%`** is coerced under PHP's ARITHMETIC contract, which is a THIRD
one — distinct from both the declared-parameter and declared-return coercions, and the
reason it needs its own path. Measured on php-src 8.5.6 with `$mixed % 3`: `null` is
SILENTLY 0, where a parameter deprecates and a return raises; a non-numeric string is
`Unsupported operand types: string % int`, naming the operand types and the operator
rather than `must be of type int`; and `INF` does not raise at all — it warns that it is
not representable and yields 0. The numeric middle IS shared, so a lost fraction
deprecates identically from a float and from a float-shaped string.

Only `%` is admitted so far, and only with the box on the LEFT and a concrete integer on
the right — the shape `$n % 2` takes. Two boxed operands need php-src's full cross-type
table, which is a different problem.

**A boxed value reaching a builtin's declared `int` parameter** is coerced PHP's way,
which completes the pair started by the `string` one. It arrives differently: `substr($s,
$mixed)` reaches the call with the Mixed operand INTACT — the frontend materialises no
cast for it — so the coercion is emitted where the argument is pushed rather than where
a cast would have been.

The conversion is the one a declared `int` RETURN performs, and the runtime SHARES a
core with it rather than carrying a second copy, because measured on php-src 8.5.6 the
two differ in exactly two places: `null` does not raise at a parameter — it becomes 0
after a `Deprecated` naming the parameter — and the failure says `Argument #N ($p)`.
Every numeric answer in between is identical, both precision deprecations included:
`2.7` truncates to 2 with a notice, `-2.7` toward zero to -2, `"2.0"` is silent because
its VALUE is integral while `"2.7"` gets the float-STRING wording, and `INF` has no
conversion at all and is a `TypeError` naming `float`.

**`php://memory` and `php://temp`** are streams with no host file behind them, and
`feof`, `ftell` and `rewind` come with them. Every other stream this target opens is a
WASI fd, and WASI is capability-based — without a preopened directory there is no
filesystem at all — but an in-memory stream needs none of that, so it is opened before
the preopen probe and works under a host that granted nothing. The descriptor's ADDRESS
is the handle with a high bit set, which keeps it disjoint from the fd space without a
side table, and the bytes live in a separate block so a write can grow the stream
without invalidating the handle the script holds.

Two behaviours were measured rather than assumed, and both would have been wrong by
intuition. `feof` is set by a read that ASKED for more than was there, not by one that
merely finished at the end: requesting 5 of 5 leaves it false, requesting 100 of 6 sets
it true even though 6 bytes came back. And a mid-stream write OVERWRITES rather than
inserting, so `"abcdef"` rewound and written `"XY"` reads back `"XYcdef"`.

An in-memory stream honours its open mode as php does: `a`/`a+` append (the bytes go to the
end, the position still advances from where it was), `w`, `w+`, `r+` and friends read and
write, and `r`, `x` or `c` alone are read-only, so `fwrite` answers `false`. A `data:` URI is a
read-only stream too, and `file_get_contents`/`readfile` decode one (plain, `urldecode`d, or
`;base64`) without touching the filesystem. `fseek` refuses a whence outside 0/1/2 and leaves
the position alone.

A closed handle is php's `TypeError` for every stream and directory builtin
(`fread(): Argument #1 ($stream) must be an open stream resource`, and likewise `readdir`,
`rewinddir`, `closedir`), through whichever copy of the handle reaches it; uncaught it is the
usual fatal line and exit status 255. `fread()` with a length below 1, `stream_get_line()` with
a negative one and `stream_get_contents()` below -1 raise `ValueError`. The per-fd state (eof
flag, mode/uri record, chunk size, directory cursor) covers every descriptor up to
1023; a host descriptor past that is closed again and the open answers `false`, which is what
php does under the usual `ulimit -n 1024`. Closing a memory stream keeps its 64-byte descriptor
(about 80 bytes per stream) so every other copy of the handle still answers "closed". Every other
stream (`fopen`, `tmpfile`, `opendir`, and the `php://stdin|stdout|stderr|output` spellings) carries
a per-open 8-byte record naming its host descriptor, closed exactly once at `fclose`/`closedir` and
kept as the closed tombstone (about 40 bytes per open/close pair in a loop, measured), so a stale
copy of a closed handle stays closed after the host hands its descriptor number to a new stream,
and closing one `php://output` stream does not close another. `fclose(STDERR)` (likewise
`STDOUT`/`STDIN`) marks the constant closed for every copy, as php does, but keeps the host
descriptor open so `echo` and error reporting keep working, where php loses its output.

`php://temp/maxmemory:N` is accepted under the same arm: that suffix only chooses when
php-src spills the buffer to a real file, which this implementation never does. The
other `scheme://` wrappers stay refused.

**`class_exists`, `interface_exists`, `trait_exists` and `enum_exists`** answer from
the module's own declarations. They are closed-world questions: the checker already
requires a literal name in AOT mode, and this module IS the whole program, so each
folds to a constant with no runtime table consulted. PHP's `$autoload` argument
cannot change that — a name this module never declared has nothing to load.

The four namespaces are distinct, which is the half worth measuring rather than
assuming: php-src answers `class_exists("Shape")` FALSE for an interface and
`interface_exists("Circle")` FALSE for a class. The one crossover is an ENUM —
`class_exists("Suit")` is TRUE — because an enum IS a class in PHP. Names match PHP's
own way: case-insensitively, with a leading `\` on either side not part of the name.

**`array_keys` and `array_values` over an indexed array of ANY element type** are
admitted. The gate demanded `array<int>`, which was stricter than the code it guards:
`__rt_array_index_keys` builds `[0..n-1]` from the LENGTH alone and never reads an
element, and `__rt_array_clone_shallow` reads the array's own `elem_size` from the
header and then re-persists or increfs each child by its value_type — Mixed cells
included. What the result may be follows from which projection it is: `array_keys`
answers `array<int>` whatever the elements were, while `array_values` has to preserve
the element type unchanged. `ArrayIterator::__construct` calls `array_keys` on an
`array<mixed>`, so the old gate refused the whole SPL iterator family.

**`foreach` over a BOXED value** is lowered. The iterator picked indexed-versus-hash
at compile time from the source's EIR type, so a `mixed` source — which names no
storage until the cell is read — was refused outright. The cursor seed, the advance,
the key and the value now all dispatch on the runtime tag, through one set of helpers
so a dynamic iterator and a statically typed one advance identically.

The non-iterable arm is the one worth measuring rather than assuming: php-src 8.5.6
does NOT raise there. It WARNS naming the type that arrived and runs the body zero
times, so the loop is still entered and left cleanly — which is why the dispatch
carries a third kind rather than a fatal. Both the key and the value come back boxed,
which is what the EIR already types them, so no element contract is needed.

**A dispatch ladder ignores a subclass this module could never construct.** A
virtual call collects every concrete class in the receiver's subtree, and the audit
demands a body for each — so one subclass whose body was never emitted refused the
call and took the base class's own method down with it. The SPL prelude does exactly
that: `__ElephcAppendIteratorArrayIterator` extends `ArrayIterator` and declares
`append` with no body here, which refused `$this->append(...)` inside
`ArrayIterator::offsetSet` and blocked 7 examples.

Dropping it is licensed by the same audit that would refuse creating it: a class
DECLARING `__construct` with no body cannot be instantiated, so no instance exists to
dispatch to. A class declaring no constructor at all needs no body and stays. A
THROWABLE is exempt for a different reason — the runtime raises `ValueError` and its
siblings directly, never through `new`, and their accessors are open-coded against
bodyless classes on purpose.

**`===` between two BOXED values** is answered, deep array identity included. Either
cell can hold an array, and PHP's array identity is a DEEP, ORDER-SENSITIVE
element-wise comparison rather than the tag-plus-payload test a cell against a
concrete value needs — so the pair was refused outright. Measured on php-src 8.5.6,
three cases are what make the walk non-trivial: `["a" => 1, "b" => 2] === ["b" => 2,
"a" => 1]` is FALSE, so comparing key SETS answers the wrong thing; `[0 => 1, 1 => 2]
=== [1 => 2, 0 => 1]` is FALSE for the same reason with integer keys; and `[[1], [2]]
=== [[1], [3]]` is FALSE, so it has to recurse.

One logical cursor serves both storages, which makes an indexed array comparable
against a hash for free: an indexed array's keys are `0..count-1` and its cursor is
the index, while a hash walks its insertion-order list. Nothing is boxed on the way —
a hash entry already stores its value as `(tag, lo, hi)` and an indexed element is
read in place — so the comparison allocates nothing, verified as a flat page count
against a varying number of comparisons. The NATIVE backend answers `false` for two
structurally equal arrays here, comparing them by heap pointer, so this target is the
one that matches php-src.

**A constructor's builtin call no longer ends the initialization proof.** A typed
property with no default may be read only once something proves the slot was
written, and a constructor that writes it unconditionally is that proof — but the
walk treated ANY call as a point past which `$this` might have escaped. That is
right for a closure, which binds `$this` with no operand naming it anywhere, and
for a call that receives `$this`; it is wrong for `array_keys($array)`. The object
is FRESH from `new`, so nothing outside holds a reference to it yet, and user code
can only reach it through the call's own arguments — a callback that captured it
would be a closure above, and an array or global holding it would be a store above,
both of which end the walk first. So a typed runtime call whose operands do not name
`$this` is now transparent. `ArrayIterator::__construct` calls `array_keys` before
its first property write, and treating that as an escape refused every
`$this->position`, `$this->keys` and `$this->values` read in the SPL iterator
family — 9, 7 and 7 examples respectively.

**`===` and `!==` against a nullable int** are lowered. This target stores a `?int`
as an inline `{payload, tag}` PAIR rather than one word, and the pair was refused
outright — which turned away every `$x === null`, the single most common thing
anyone writes with one and the largest strict-comparison gap in the suite. Its tag
is 0 or 8 and nothing else (`codegen_repr` folds only `int|null` to this
representation), which is what makes each case decidable rather than approximate:
against a string, a bool or a float the answer is a compile-time FALSE, because a
`?int` holds none of those and `===` compares the type first. Against a concrete
int the TAG is checked before the payload, so `$x === 0` does not answer true for a
null whose payload word also happens to be zero.

What the pair may not meet is a runtime-tagged `Mixed` cell — that cell's own tag is
dynamic too, and the existing mixed/concrete path assumes exactly one side is —
so that pair stays refused. Producing the slot needed two more pieces: an untyped
runtime call WIDENING a concrete scalar into it (a widening cannot lose information
or raise, unlike the narrowing in the other direction, which stays refused), and
`const_null` writing both components instead of the one-word sentinel. The latter
was a latent invalid-module bug, reachable only once the widening was admitted.

**A boxed object reaches `__toString`** instead of being refused outright. The
object arm of every string conversion raised `Error: Object of class C could not
be converted to string` for EVERY class, which is php-src's answer only for a
class that does not define the method — measured, `(string)$tag` prints `<em>`
there and fatally raised here, a wrong ANSWER in shipped code rather than a
refusal. The conversion now goes through a runtime class-id dispatch, so a class
defining `__toString` converts and one that does not still raises. All three
ownership shapes are covered: a literal returns a data-segment pointer that must
not be released, a property read returns storage the object still owns, and a
concat returns a fresh heap string.

That dispatch only carries classes whose `__toString` has a BODY in the module,
which is what excludes most of the prelude: `Exception`, `SplFileInfo` and the
`Reflection*` family all declare the method with no body this target can call, so
they still reach the fatal. That is unchanged from before rather than new, and it
is the same set the `Throwable` accessors already approximate.

**A boxed value reaching a builtin's declared `string` parameter** is coerced
PHP's way. This is a THIRD implicit `Str` conversion, distinct from both the
explicit `(string)` cast and the one an echo performs, and the difference is not
cosmetic. Measured on php-src 8.5.6 for `strtoupper($mixed)`: a string, int,
float or bool converts exactly as `(string)` does; `null` converts to `""` but
raises `Deprecated: strtoupper(): Passing null to parameter #1 ($string) of type
string is deprecated`; and an array — which `(string)` would have turned into
`"Array"` with a warning — does not convert at all, it is `TypeError:
strtoupper(): Argument #1 ($string) must be of type string, array given`. An
object contributes its CLASS name to that message, and a closure the word
`Closure`.

Both names in those messages come from the builtin registry, which the EIR
reaches through the call's runtime target. A target several PHP names share is
refused rather than guessed — `count` and `sizeof` reach the same one, and
php-src reports the name AS WRITTEN, which the target cannot recover. Unblocks
`examples/pipe-operator`.

**A boxed value reaching a slot declared as a class** is narrowed back to an
object. A `?Node` property is stored boxed, so `return $this->node;` from a method
declared `: Node` moves a `Heap(Mixed)` into an object slot through a call that
carries no runtime function id at all — the frontend leaves the conversion
implicit, and the audit refused it as `missing typed runtime target`. The lowering
unboxes the cell and takes its payload, with a tag guard the native backend does
without: a cell holding anything but an object yields a null pointer rather than a
scalar reinterpreted as an address. Unblocks `examples/type-narrowing`.

Two shared defects found the same way remain OPEN, both in the checker rather
than in this backend:

- An untyped property narrowed from its null default to a concrete class drops
  the null it still holds, so `$a->next === null` answers false for a slot that
  is null, and `$cur = $head; while ($cur !== null) { $cur = $cur->next; }` does
  not terminate on the native backend. A declared `?N $next = null` is correct in
  both backends, so the narrowing rather than the storage is at fault. Giving the
  slot union storage fixes the answer but stops a self-referential cycle from
  being collected, and a leaked cycle is not an improvement on a wrong answer, so
  the change was reverted rather than shipped half-right. This target refuses the
  shape.
- The checker analyses a loop condition once, with the environment from before
  the loop, so a local reassigned in the body is typed from its pre-loop value in
  the condition. The straight-line form of the same walk is correct.

Known divergence: an uncaught error prints a class-agnostic fatal, and
diagnostics omit php-src's ` in <file> on line <n>` tail. A `TypeError` raised
by a coercion or by `count()` is a deterministic fatal rather than a catchable
throw, because a message composed at runtime cannot be resolved from the static
error table.

To select it:

```bash
elephc --target wasm32-wasi hello.php
elephc --target wasm32-wasi --emit npm hello.php
node hello-npm/index.mjs
```

iOS targets are ARM64-only. `ios-x86_64`, `ios-sim-x86_64`, and their
x86_64 Apple-triple spellings are rejected during target parsing with an
arm64-only diagnostic.

The NPM form writes `hello-npm/` with `module.wasm`, an ESM loader
(`index.mjs`), TypeScript declarations, package metadata, and a README. The
loader can also be imported:

```js
import { run } from "./hello-npm/index.mjs";

const exitCode = await run({
  args: ["hello", "first-argument"],
  env: process.env,
  preopens: { "/work": process.cwd() },
});
```

WASM output is currently a WASI command (`_start`); `--emit cdylib` reactors are
rejected with a focused diagnostic.

Native-only compiler options are also rejected with focused diagnostics instead
of being ignored: web-server mode, native source maps/DWARF, native heap and
register-allocation controls, native linker/framework flags, and bridge-crate
linking are not yet available on `wasm32-wasi`.

The parser also recognizes `macos-x86_64` / `x86_64-apple-darwin` and
`windows-x86_64` / `x86_64-pc-windows-msvc` /
`x86_64-pc-windows-gnu`. These spellings are groundwork for future backends,
not supported targets: compilation stops with an explicit unsupported-backend
diagnostic.

## Cross-compilation notes

Selecting a native target different from the host produces assembly and an
object file for that target. Producing a final linked binary still depends on
having a linker and any target libraries available for that platform; the
elephc test suite uses the Docker scripts under `scripts/` to build and run the
Linux targets from a macOS host. `wasm32-wasi` instead follows the artifact and
host workflow described above.

iOS output is a native library consumed by an application host, not a complete
signed `.app` bundle. Select `--emit staticlib` (the usual Xcode delivery form)
or `--emit cdylib`; standalone `--emit executable` output is rejected for both
iOS device and Simulator targets. A linked iOS library requires macOS with the
matching Xcode SDK. `--emit-asm` can stop before assembling, but it must still be
paired with a library emit kind so the assembly contains the public library ABI.

For the target-aware ABI and runtime details behind each platform, see
[Architecture](../internals/architecture.md) and
[The Code Generator](../internals/the-codegen.md).
