# PHP proc_open family

- [x] Add the `elephc-proc` bridge: posix_spawn builder, descriptor table, cached waits.
- [x] Add the internal resource builtins: process resource box/unbox/close, fd <-> stream.
- [x] Add the injected proc prelude: PHP 8.5 validation and descriptor walking over the bridge.
- [x] Add `proc_open`, `proc_get_status`, `proc_close`, `proc_terminate`, `proc_nice` as
      registry builtins composing the prelude helpers (GMP / xml_parse_into_struct model).
- [x] Codegen tests cross-checked against PHP 8.5, error tests, example, generated docs.
- [ ] Follow-up PR: `['pty']` descriptors (currently PHP's "not supported" warning).
- [ ] Follow-up PR: Magician (eval) support; today the family is AOT-only.
- [ ] Follow-up PR: `STDIN`/`STDOUT`/`STDERR` inside a heterogeneous array literal lose
      their resource type (pre-existing); homogeneous `[STDIN, STDOUT, STDERR]` works.
- [ ] Follow-up PR: `get_resources('process')`, and reaping a process resource dropped
      without `proc_close()`.

## Shape

`proc_open()` needs a write-only by-reference output (`$pipes`) on an undefined variable,
which only a registry builtin's checker hook admits, and the iOS sandbox refusal must be
reported at the user's call site. The five public names are therefore `builtin!` homes
whose EIR lowering boxes the operands and calls `__elephc_proc_*` helpers declared by an
injected synthetic-AST prelude, exactly like GMP. `proc_open()` stores the second helper's
pipes array into the caller's variable through `store_operand_local`.

The prelude does the PHP-visible work in PHP: argument validation with php-src's exact
`ValueError` texts and warnings, walking the descriptor spec, converting env entries,
and building `$pipes`. It talks to the bridge through `extern "elephc_proc"`
declarations that take only scalars and C strings.

The bridge spawns with `posix_spawn`/`posix_spawnp` (`addchdir_np` for `$cwd`), which is
what PHP 8.5 itself uses on macOS and Linux, so spawn failures are the same
`posix_spawn() failed: <strerror>` warning plus `false`. Parent pipe ends are
close-on-exec. `waitpid` results follow php-src's `waitpid_cached`: only a normal exit
is cached, a signal death is reported once.

The process resource is a tag-9 Mixed cell whose high word is kind 11, which
`__rt_resource_type_name` reports as `process`; kind 11 has no scope-exit destructor
(php-src's dtor only reaps with `WNOHANG`). `proc_close()` closes the pipes the bridge
spawned for the process (only while each descriptor still has the spawned pipe's
inode, so a number the program already closed and reused is never touched), waits,
stamps the closed sentinel, and returns the status.
