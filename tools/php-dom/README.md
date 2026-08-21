# PHP DOM snapshot tooling

These tools freeze and validate the PHP 8.5.8 DOM/libxml/SimpleXML reference used
by `docs/specs/php-dom-compliance.md`.

The reference inputs are intentionally external to the Elephc checkout:

- an official php-src `php-8.5.8` checkout at the peeled commit recorded in
  `source-lock.json`;
- an exact PHP 8.5.8 CLI built against libxml2 2.15.3 and PHP's bundled
  Lexbor 2.7.0.

Generate the Reflection surface:

```bash
/path/to/php-8.5.8/sapi/cli/php \
  tools/php-dom/snapshot_surface.php \
  tests/php_dom/surface/php-8.5.8.json \
  /path/to/php-src-8.5.8
```

The second input is mandatory because PHP Reflection intentionally does not
report handler-backed virtual `@readonly` properties as language-level
readonly. The generator verifies all three pinned stub hashes and records a
separate semantic `writable` bit without changing Reflection's `readonly` bit.

Generate upstream PHPT ledgers:

```bash
python3 tools/php-dom/generate_ledgers.py \
  /path/to/php-src-8.5.8 \
  tests/php_dom/upstream
```

Run the frozen CLI PHPT corpus against both the exact PHP oracle and Elephc:

```bash
python3 tools/php-dom/phpt_runner.py \
  --php-src /path/to/php-src-8.5.8 \
  --oracle /path/to/php-8.5.8/sapi/cli/php \
  --elephc target/debug/elephc \
  --component simplexml \
  --filter 'ext/simplexml/tests/(000|bug79971_1)\.phpt$' \
  --report-json /tmp/simplexml-phpt-report.json
```

The runner fails closed unless `--php-src` is a Git checkout at the commit and
extension-tree hashes in `source-lock.json`. The selected test directory and
every external fixture must also be clean relative to that commit; unrelated
in-tree build products are allowed. It also checks that the oracle is PHP 8.5.8
with libxml2 2.15.3 and the DOM, libxml, and SimpleXML extensions loaded. A
non-default oracle build can receive repeatable arguments such as
`--oracle-arg=-d --oracle-arg=extension=/path/to/simplexml.so`.

Each test receives separate oracle and Elephc sandboxes containing the complete
component fixture directory. The known cross-extension XSL fixture used by
`gh17153.phpt` is staged automatically; additional php-src-relative files or
directories can be authorized with repeatable `--external-fixture` arguments.
The focused runner supports `FILE`/`FILEEOF`/`FILE_EXTERNAL`, `SKIPIF`, `CLEAN`,
`INI`, `ENV`, `ARGS`, `EXTENSIONS`, `EXPECT`, `EXPECTF`, `EXPECTREGEX`, and the
three external expectation variants. External sections are resolved only
inside the pinned PHPT directory. EXPECTF and EXPECTREGEX are evaluated by the
pinned oracle's PCRE engine with the PHP 8.5.8 `run-tests.php` substitutions,
including raw `%r...%r` regions. All 1,056 frozen DOM/libxml/SimpleXML PHPTs
parse through this supported CLI surface.

The Elephc sandbox also receives the committed managed-PCRE2 manifest and lock
from `examples/hello-preg`, because regex and dynamic `eval()` paths must link
the verified native package even when the extracted PHPT lives outside the
repository. Install that locked host-target artifact once before replaying:

```bash
cd examples/hello-preg
../../target/debug/elephc native install --locked --target macos-aarch64
```

Use the matching public target for Linux runs. PHPT runtime temporary variables
remain isolated, but compiler invocations restore the caller's `PATH` and temp
variables so the managed-native toolchain fingerprint is stable across sandbox
directories. The runner never downloads or installs native packages itself.

The JSON report retains raw merged stdout/stderr as base64 plus hashes, exit
codes, timeouts, compile diagnostics, SKIPIF/CLEAN evidence, and filesystem
deltas before and after CLEAN. A test only receives `passed` when both runtimes
satisfy the frozen expectation and their exit codes and observable file changes
match. XFAILs, missing required extensions, unsupported non-CLI sections, and
oracle inconsistencies stay explicit and do not count as a clean run. For a
cross-target binary, pass `--target` and a repeatable `--execute-prefix` (for
example an emulator); the harness never treats a non-runnable linked artifact
as runtime evidence.

Run the harness self-tests and matcher syntax check with:

```bash
python3 -m unittest -v tools/php-dom/test_phpt_runner.py
php -l tools/php-dom/phpt_match.php
```

The runner deliberately does not mutate a ledger. Its report is evidence for a
subsequent reviewed ledger update; a `pending` entry remains pending until that
evidence has been assessed and linked.

Generate the stable bridge opcode manifest and Rust lookup table:

```bash
python3 tools/php-dom/generate_opcodes.py
python3 tools/php-dom/generate_opcodes.py --check
```

Validate checked-in snapshots without an external checkout:

```bash
python3 tools/php-dom/check_snapshots.py
```

Pass `--source-root /path/to/php-src-8.5.8` to additionally re-hash every
upstream PHPT and verify the pinned Git commit/tree.

Prepare the deterministic offline native-source archives from the exact
official release archives:

```bash
python3 tools/php-dom/prepare_native_sources.py \
  --php-archive /path/to/php-8.5.8.tar.xz \
  --libxml-archive /path/to/libxml2-2.15.3.tar.xz

python3 tools/php-dom/prepare_native_sources.py \
  --php-archive /path/to/php-8.5.8.tar.xz \
  --libxml-archive /path/to/libxml2-2.15.3.tar.xz \
  --check
```

Ledger entries begin as `pending`. Completion requires each entry to be changed
to `direct`, `translated`, or the narrowly permitted `not-applicable`, together
with its Elephc fixture/evidence metadata. The checker reports pending entries
but accepts them during implementation; `--require-closed` makes them fatal.
