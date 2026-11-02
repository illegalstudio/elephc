# Threat model

## What this project does and where untrusted input enters

elephc is an ahead-of-time compiler, written in Rust, that turns PHP source into native assembly and links it into a
standalone binary (or a C-ABI library). Compiled programs carry no interpreter: their runtime is assembly emitted by
the compiler (the `__rt_*` helpers in `src/codegen_support/runtime/`) plus Rust "bridge" staticlibs under
`crates/elephc-*` that are linked on demand. Supported targets are macos-aarch64, ios-arm64, ios-sim-arm64,
linux-aarch64 and linux-x86_64; this image builds and runs linux-x86_64.

The PHP source is written by the developer and is trusted. The data a compiled program processes at run time is not:
assume that every value a PHP program reads (request bodies, files, sockets, environment, command-line arguments) is
attacker-controlled. The promise that matters is that a PHP program that is memory-safe under the PHP interpreter stays
memory-safe once elephc compiles it.

Untrusted input reaches, in particular:

- The runtime helpers for strings, arrays and hashes, `Mixed` values, objects, references, copy-on-write, refcounting
  and the cycle collector, whenever they are fed attacker-sized or attacker-shaped data (offsets, lengths, keys,
  nesting depth).
- Parsing and decoding builtins: `unserialize`, `json_decode`, `preg_*` (managed PCRE2) and `mb_ereg*` (managed
  Oniguruma), the `sprintf`/`printf` family, `parse_url`, `base64_decode`/`hex2bin`/`urldecode`, and date and time
  parsing (`strtotime`, `date_parse`).
- Bridges: `elephc-web` (the `--web` prefork HTTP server, built on hyper; request lines, headers, multipart bodies,
  sessions), `elephc-xml` (libxml2 2.15.3 through `src/native_deps/recipes/libxml2_shim.c`), `elephc-curl` (libcurl
  responses), `elephc-phar` (PHAR archives), `elephc-image` (GD, Exif, Imagick, Gmagick, Cairo inputs), `elephc-mbstring`
  and `elephc-iconv` (charset conversion), `elephc-tls`, `elephc-crypto`, `elephc-bcmath`, `elephc-pdo` (database
  results), `elephc-tz`.
- `eval()`: `elephc-magician` interprets PHP source at run time. When a program evaluates a string built from input,
  the interpreter, and every native function it can reach, are exposed to that input.
- `elephc native add/install`: downloads pinned source archives for managed C packages, verifies them against
  checksums in the catalog and in `elephc.lock`, extracts and builds them into a cache. A third-party project's
  `elephc.toml`/`elephc.lock` should not be able to escape the cache, bypass checksum verification, or substitute an
  unpinned archive.

## Components that matter most / least

- Most: the generated runtime and codegen (`src/codegen/`, `src/codegen_support/`, `src/ir_lower/`, `src/ir_passes/`),
  because a miscompilation or a runtime helper bug that corrupts memory affects every compiled program; then the
  `--web` bridge (remote, unauthenticated input), the parsing builtins and bridges above, `eval()`, and the native
  package verification and extraction code in `src/native_deps/`.
- Also in scope: PHP extensions and C-ABI libraries emitted with the cdylib/staticlib modes, where a host process
  passes values in (`docs/beyond-php/cdylib.md`, `docs/beyond-php/zval-bridge.md`).
- Least: the compiler front end on malformed or hostile PHP source (lexer, parser, type checker). The source is
  trusted, so a compiler panic, crash or blow-up on bad source is an ordinary bug, not a vulnerability. The exception
  is valid PHP that compiles without error into a binary that corrupts memory at run time: that is a miscompilation
  and in scope.
- Out of scope: `examples/`, `benchmark/`, `showcases/`, `docs/`, `scripts/`, `tests/` and `.github/`, except as
  drivers.

## How to exercise it

- Compile and run a program: `./target/debug/elephc prog.php && ./prog` (the binary is written next to the source).
  `--emit-asm` keeps the generated assembly, `--emit-ir` prints the intermediate representation.
- Build any compiled program with `--heap-debug` to turn on the runtime's heap verification (double free, bad
  refcount, free-list corruption). The runtime uses its own heap inside a fixed region (`--heap-size`, 8 MB by
  default), so AddressSanitizer does not see the generated code; `--heap-debug`, gdb and the program's exit status
  are the tools. Rust bridges can be built and tested with the usual Rust tooling.
- HTTP server: `./target/debug/elephc --web app.php && ./app --listen 127.0.0.1:8080` (see `docs/beyond-php/web.md`
  for the `--web-isolation` models).
- Feature documentation lives in `docs/php/` (one page per area: strings, arrays, regex, xml, curl, eval, phar, image,
  sessions, streams) and `docs/beyond-php/`; runtime internals in `docs/internals/the-runtime.md`,
  `docs/internals/memory-model.md` and `docs/internals/eval-runtime.md`.
- About 250 small programs in `examples/*/main.php` cover most features and are convenient starting drivers.
- Tests: `cargo nextest run --test codegen_tests <filter>` runs end-to-end tests that compile PHP, run the binary and
  check its output (`tests/codegen/`, ownership and GC cases in `tests/codegen/runtime_gc/`). Bridge crates have
  their own `cargo nextest run -p elephc-<name>` tests.
- Everything is prebuilt offline: the compiler and bridge staticlibs in `target/debug/`, the managed native packages
  in `$ELEPHC_NATIVE_CACHE` (`/opt/elephc-native`), and every crate in the cargo registry. Changing a bridge's sources
  makes the compiler rebuild that bridge with cargo, offline, on the next compile.
- A program that uses regex, multibyte regex, curl or XML links a managed native package, and the compiler only
  finds it through an `elephc.toml` and `elephc.lock` in the program's directory or one of its parents. Without them
  the link fails with "requires managed native package". Copy both files from the example whose lock pins the package
  next to the new program: `examples/hello-preg` (PCRE2), `examples/date-json-regex` (PCRE2 and Oniguruma),
  `examples/curl-get` (curl, libssh2, nghttp2, OpenSSL, zlib) or `examples/xml` (libxml2). For example:
  `mkdir /tmp/t && cp examples/xml/elephc.toml examples/xml/elephc.lock /tmp/t/ && ./target/debug/elephc /tmp/t/prog.php`.

## How you rate severity

- Critical: memory corruption with attacker control over what is written or executed, reachable from data a compiled
  program processes, especially through `--web` request handling.
- High: any other out-of-bounds read or write, use-after-free, double free or refcount underflow in the runtime or a
  bridge that untrusted data can trigger; a miscompilation that produces such a bug from valid PHP; checksum or
  path-containment bypass in `elephc native`; a cryptographic or comparison builtin (`hash_equals`, `hash_hmac`,
  `openssl_encrypt`, `random_int`, TLS certificate checks) that is weaker than PHP's.
- Medium: denial of service reachable from untrusted data (a crash, an abort or unbounded memory or CPU in a `--web`
  worker or a parser), and information leaks of uninitialized memory.
- Low: crashes that need a hostile PHP source file or hostile compiler flags; divergences from PHP semantics without
  a memory-safety or security consequence (these are ordinary bugs, please file them as issues).

## Anything to leave alone

- The compiler extensions that hand raw memory to the program on purpose: `ptr`, `ptr_*` builtins, `extern`,
  `buffer<T>`, `packed class` and FFI calls (`docs/beyond-php/`). A program that misuses them can corrupt memory by
  design; report only a bug where the compiler or runtime breaks its documented contract for them.
- Benign linker warnings on Linux when compiling a program (`gethostbyname` in statically linked applications).
- `#[ignore]` tests that need SDL2, live databases or the network.
