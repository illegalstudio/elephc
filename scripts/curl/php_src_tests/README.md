# php-src curl compliance audit

Developer tools for measuring elephc's `ext/curl` implementation against
php-src's own test suite. These are **not** part of `cargo test` and are not run
in CI: they need a PHP CLI binary, a local checkout of php-src's
`ext/curl/tests`, and the managed native `curl` package.

## Prerequisites

1. A PHP CLI binary with the `curl` extension (used only to evaluate
   `--SKIPIF--` blocks and to start the local dev server the tests talk to).
2. php-src's `ext/curl/tests` directory. From a checkout:

   ```bash
   git clone --depth 1 --branch PHP-8.5 https://github.com/php/php-src
   ```

   or extract just the directory from the release tarball.
3. The managed native packages the fixtures need (`curl` plus `pcre2` for the
   regex-using fixtures) and a project manifest that declares them. The runner
   copies `elephc.toml` / `elephc.lock` from its own `audit-project/` directory
   by default; pass `--manifest-source` to use another project.

## Usage

```bash
python3 run_php_src_tests.py /path/to/php-src/ext/curl/tests \
    --elephc ../../target/debug/elephc \
    --jobs 8 \
    --json results.json
```

Then summarize the failures:

```bash
python3 categorize_results.py results.json
```

## What the runner does

- Parses each `.phpt` into its `--NAME--` sections.
- Evaluates `--SKIPIF--` with the host `php` binary.
- Starts one `php -S` dev server and shadows `server.inc` with a shim that
  returns the pre-started `localhost:PORT`, so tests exercise elephc's curl.
- Rewrites inline-HTML `?>` segments to equivalent `echo` statements, because
  elephc's frontend does not tokenize closing tags.
- Compiles the `--FILE--` section with elephc, runs the binary with stderr
  merged into stdout, and compares using php-src `run-tests.php` semantics.
- Emulates PHP CLI's `display_errors` leading newline before diagnostics, so the
  stderr-vs-stdout rendering difference does not mask semantic differences.

## Managed-native cache fingerprint caveat

The native resolver keys artifacts by a toolchain fingerprint that includes
`PATH` and `TMPDIR`, so artifacts built in another shell/session are not found
and the compile fails with "native artifact is missing or corrupt". The archives
themselves are SHA-256-verified, so a cached artifact can be reused under the
current fingerprint by copying its directory and rewriting only the receipt's
`toolchain_fingerprint`:

```bash
python3 bootstrap_native_cache.py ../../target/debug/elephc \
    audit-project/elephc.lock
```

**This mutates the global native cache** (`~/.cache/elephc/native`). It copies
pinned, already-built archives under a new key; it never downloads or rebuilds.
Run it once per session before the audit when the compile reports a missing
artifact. Set `ELEPHC_NATIVE_CACHE` to redirect it elsewhere.

Alternatively, build the packages for the current fingerprint with
`elephc native install --locked` inside a project that declares `curl`; that is
slower (libcurl + OpenSSL from source) but needs no cache mutation.
