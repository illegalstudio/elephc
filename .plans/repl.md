# Eval-backed interactive REPL

- [x] Add parser-backed input classification and Magician terminal input support.
- [x] Add the persistent eval bootstrap and `elephc repl` CLI integration.
- [x] Cache the host executable with build/configuration/dependency identity, locking, and atomic publication.
- [x] Cover multiline input, persistent values/declarations, errors, terminal controls, and cache behavior with focused tests.
- [x] Document the command, cache lifecycle, capabilities, and an executable example.
- [x] Complete focused build, regression, and hygiene validation.
- [x] Recover reported eval failures at the submission boundary and cover repeated errors without losing session state.

## Execution

Compile a small PHP bootstrap once for each host configuration. It repeatedly invokes
dynamic `eval` in one top-level scope, using the existing runtime and Magician ABI.
Input handling is provided by private Magician C ABI helpers. Reuse Magician's parser
for completeness checks and expression result capture; never execute speculative
input. Retain normal PHP effects and recover only from errors the runtime can safely
handle. Reported eval failures recover at the outer submission boundary after
interpreter cleanup; native process failures and explicit PHP exit terminate the session.

## Terminal and parser

Provide line editing, history, continuation prompts, input cancellation, EOF, and
`:help`/`:quit`. Non-terminal input has no banner or prompts. Report incomplete input
at EOF. Persist history only for interactive sessions and allow disabling it.

## Cache and configuration

Use the existing per-user cache root with a separate `repl` namespace. Fingerprint
the compiler, bootstrap, linked bridge/native dependencies, target, PHP profile,
and effective compile configuration. Publish only complete verified executables,
serialize competing builds, and clean task staging files on failure. Every launch
creates fresh PHP state. Resolve project configuration and relative paths from the
invoking directory rather than the cache directory.

## Validation

Parser units exercise expressions, declarations, multiline strings/comments, nested
blocks, alternative syntax, invalid input, and exactly-once expression evaluation.
CLI integration tests exercise the actual cached native host, persistent state,
recoverable errors, cache reuse/invalidation, and piped input. Terminal tests cover
line editing, continuation, Ctrl-C, and Ctrl-D where PTYs are available. All compiler
targets keep the existing target-aware eval lowering; the REPL runs on desktop hosts.

Validated on Linux x86_64 with the compiler, Magician, and timezone bridge build;
12 REPL integration tests (including PTY coverage), 5 REPL CLI/cache unit tests, all 222
Magician parser tests, 2 FFI buffer hygiene checks, and the existing dynamic global
eval scope regression. The documented pricing example was executed against the
real CLI, followed by a warm cache launch. Nextest configuration, Rust module
preambles, diff whitespace, and temporary fixture cleanup were checked. Other
supported desktop targets use the same host code and remain covered by CI.

The terminal fixture also renders cursor movement and line clearing, proving that
`echo` without a trailing newline remains visible after the next prompt. This
regression failed before enabling the editor's cursor-position check. A paired
piped session verifies that prompt layout does not add bytes to redirected output.

Undefined direct calls now raise catchable `Error` values through Magician's
existing Throwable path. Tests cover namespace fallback and aliases, argument
side effects, catches within eval and across the native boundary, and REPL state
and prompt recovery. Known unavailable builtin capabilities retain their runtime
diagnostics. Focused operand and closure tests cover cleanup during propagation.

Reported eval failures now recover at the outer submission boundary, including
non-displaying statements. Tests cover repeated runtime and unsupported errors,
invalid nested eval, user fatals, argument errors, and by-reference state after a
failed function call. Three recovery unit tests and 47 FFI tests validate owned
Throwable transfer and guarded ABI behavior. Ordinary eval error contracts and
PCNTL escape checks remain covered separately. Native process termination and
unsafe ABI failures intentionally remain unrecoverable.
