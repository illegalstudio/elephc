---
title: "Interactive REPL"
description: "Run PHP interactively through eval with persistent state, parser-driven multiline input, and a cached native session host."
sidebar:
  order: 11
---

Start an interactive session with:

```bash
elephc repl
```

Every submission runs through dynamic [`eval()`](../php/eval.md) and Magician.
Variables, references, functions, classes, and constants survive between submissions.
Each new invocation starts with fresh PHP state.

```text
>>> $value = 10;
int(10)
>>> function twice($n) {
...     return $n * 2;
... }
>>> twice($value)
int(20)
```

## Input and results

Enter PHP fragments without an opening `<?php` tag. Magician's lexer and parser
recognize incomplete expressions, blocks, strings, and comments and request another
line at the `...` prompt. Complete statements execute immediately. Paste a whole
`if`/`else` or `try`/`catch` statement together when its first block could stand alone.

Supported expressions and simple variable assignments display their results using
`var_dump`. Other statements run without printing an extra result. A final semicolon
may be omitted when adding it completes the grammar. The parser checks candidate
forms without executing them; calls and assignments execute once per submission.

Output from `echo` remains visible even without a trailing newline: the interactive
prompt moves to a fresh line when needed. Piped sessions preserve PHP's exact bytes.

Use arrow keys to edit and recall history. Ctrl-C cancels the input currently being
edited, including a pending multiline fragment. Ctrl-D exits. Ctrl-C while PHP is
executing follows normal process signal behavior and can terminate the session.

| Command | Action |
|---|---|
| `:help`, `:h` | Show input help. |
| `:quit`, `:q` | End the session. |

Commands are recognized only when there is no pending multiline fragment.

## Errors and PHP behavior

Invalid submissions report a syntax error and return to the prompt. Escaping PHP
`Throwable` values report their class and message and also return to the prompt.
An undefined function, such as `isseet()` instead of `isset()`, raises a catchable
`Error` naming the function and leaves the session available for the next command.
Errors returned by the eval interpreter also abort only the current submission.
This includes unknown constants such as `das`, unsupported operations, invalid
nested eval source, and `trigger_error(..., E_USER_ERROR)`. The REPL reports the
available diagnostic and accepts the next command. An internal eval failure may
still have a generic message such as `Error: eval() runtime failed`.

Effects that happened before the error remain visible. Recovery does not roll back
or replay previous commands. It applies only at the outer submission boundary;
nested eval does not continue executing the failed command.

Explicit `exit()` ends the session. Recovery cannot safely resume after a native
process crash, a runtime operation that exits directly, memory exhaustion, an
internal panic, or a failed ABI safety check. Ordinary compiled programs keep their
existing eval error behavior.

The REPL supports the same language and builtin surface as dynamic eval. Namespace
and `use` declarations apply to their submitted fragment, following eval semantics.
The variable `$__elephc_repl_error` and the `__elephc_repl_*` helper functions are
reserved for session management.

Relative file operations use the invoking working directory. `__DIR__` initially
names that directory; the logical entry file is `.elephc-repl.php`, which is not
created on disk. Files may be loaded with `require` or `require_once` inside the
session. Composer PHP files are not automatically compiled into the host.

## Configuration and optional capabilities

The command reads the invoking project's PHP profile and `elephc.toml` INI settings.
Command-line overrides follow normal compiler precedence:

```bash
elephc repl --php-version 8.4 --strict-php
elephc repl --ini precision=10 --heap-size=16777216
elephc repl --with-pdo
```

Dynamic input is unavailable to compile-time feature detection. Enable optional
capabilities before starting the session. Managed native packages must already be
installed, just as for ordinary compilation:

```bash
elephc native add pcre2
elephc repl --with-regex
```

`--with-web` and monitoring modes are unavailable. `opcache.preload` is rejected;
use `require_once` through eval instead. The REPL runs on macOS ARM64, Linux ARM64,
and Linux x86_64 hosts. Cross-compilation and iOS library output remain ordinary
compiler commands.

The complete option list is in the [CLI reference](cli-reference.md#interactive-repl).

## Cache and history

On first use for a project/configuration, elephc compiles its embedded session
bootstrap with the normal native toolchain and links Magician and the selected
capabilities. Subsequent launches reuse that executable. Typed PHP is always
interpreted through eval, including after a cache hit.

Hosts live under `$XDG_CACHE_HOME/elephc/repl`, or `~/.cache/elephc/repl` when
`XDG_CACHE_HOME` is unset. The existing temporary-cache fallback applies if neither
`XDG_CACHE_HOME` nor `HOME` is available. Configuration entries account for the
compiler build, host architecture, working directory, PHP profile, project files,
INI settings, and selected capabilities. Receipts track the exact linked archives,
their identities, and executable integrity. Changed compiler or dependency inputs
and damaged executables trigger a rebuild. Concurrent first launches share a build
lock, and only completed executables are published.

The native assembler/linker and matching bridge archives are needed to build a
missing or invalidated host. A valid cache hit starts the existing executable.
Deleting a REPL cache entry causes it to be rebuilt on its next use.

Interactive history is stored in `repl/history` under the same private cache root,
with at most 1,000 entries. Lines beginning with a space are omitted. Use
`--no-history` to disable history loading and persistence. `--quiet` hides the banner
and cache-build notice while keeping interactive prompts.

## Piped sessions

Pipes and redirected files have no banner, prompts, or persistent history:

```bash
printf '$n = 20;\n$n * 2\n' | elephc repl
```

Submissions still share one scope and use the same multiline parser. Input errors
and caught exceptions make the final exit status nonzero while allowing later
submissions to run. Incomplete input at EOF is reported. PHP `exit($code)` preserves
its requested status. Each accumulated submission is limited to 1 MiB of UTF-8
source and cannot contain literal NUL bytes.

See [`examples/repl`](https://github.com/illegalstudio/elephc/tree/main/examples/repl)
for a small interactive workflow and a PHP file to load into a session.
