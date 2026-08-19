# PHP PCNTL parity

- [x] Register target-aware PCNTL availability for the PHP 8.4 surface.
- [x] Add the `elephc-pcntl` bridge and stable panic-free process-control ABI.
- [x] Add target-aware PCNTL predefined constants and `Pcntl\\QosClass` on macOS.
- [x] Add shared builtin contracts, AOT home files, typed EIR targets, and backend lowering.
- [x] Implement process lifecycle calls: fork, exec, wait, waitpid, and waitid.
- [x] Implement wait-status inspection and resource-usage/siginfo output arrays.
- [x] Implement last-error, strerror, alarm, getpriority, and setpriority.
- [x] Implement queued signal handlers, manual dispatch, async safe points, and handler lookup.
- [x] Implement signal masks, sigwaitinfo, and sigtimedwait with target-specific siginfo.
- [x] Implement Linux namespaces, CPU affinity, and CPU queries.
- [x] Implement macOS QoS enum and get/set calls.
- [x] Add Magician/eval bindings with the same contracts and process-global semantics.
- [x] Add examples, generated builtin docs, the PHP PCNTL guide, CLI/linking docs, and limitations.
- [x] Run focused macOS, Linux AArch64, and Linux x86_64 tests plus builtin parity audits.

## Scope and authoritative baseline

The implementation targets the repository's PHP 8.4 compatibility profile and the supported
Elephc matrix: macOS AArch64, Linux AArch64, and Linux x86_64. The authoritative PHP behavior is
the PHP 8.4 `ext/pcntl` stub, implementation, and PHPT suite, cross-checked against the installed
PHP CLI for host-specific values.

The union of PHP 8.4 functions available on at least one supported target is:

- Common: `pcntl_alarm`, `pcntl_async_signals`, `pcntl_errno`, `pcntl_exec`, `pcntl_fork`,
  `pcntl_get_last_error`, `pcntl_getpriority`, `pcntl_setpriority`, `pcntl_signal`,
  `pcntl_signal_dispatch`, `pcntl_signal_get_handler`, `pcntl_sigprocmask`, `pcntl_strerror`,
  `pcntl_wait`, `pcntl_waitid`, `pcntl_waitpid`, `pcntl_wexitstatus`, `pcntl_wifcontinued`,
  `pcntl_wifexited`, `pcntl_wifsignaled`, `pcntl_wifstopped`, `pcntl_wstopsig`, and
  `pcntl_wtermsig`.
- Linux: `pcntl_getcpu`, `pcntl_getcpuaffinity`, `pcntl_setcpuaffinity`, `pcntl_setns`,
  `pcntl_sigtimedwait`, `pcntl_sigwaitinfo`, and `pcntl_unshare`.
- macOS: `pcntl_getqos_class` and `pcntl_setqos_class`.

`pcntl_rfork` and `pcntl_forkx` are conditional PHP APIs for operating systems outside Elephc's
supported target matrix. They must remain absent on the supported targets, matching PHP builds on
those targets; documentation must state this explicitly instead of claiming an implementation.

## Architecture

### Bridge and link ownership

Create `crates/elephc-pcntl` as a `staticlib` plus `rlib`, backed by `libc`. Every C ABI entry is
panic-free, records the extension-local last error on failure, and uses caller-owned buffers or
plain C-layout output records. Register `elephc_pcntl` in the bridge table with `--with-pcntl` and
PHP extension name `pcntl`. All PCNTL contracts, including pure wait-status queries, require the
bridge so `extension_loaded("pcntl")` and forced linking remain coherent.

The bridge owns only OS-facing state and async-signal-safe queueing. PHP values, callable
descriptors, Mixed cells, associative arrays, exceptions, and ownership stay in the compiler
runtime where their representation is authoritative.

### Target availability and constants

Extend shared builtin metadata with a dependency-neutral target availability description that
the checker resolves against the compilation target. A target must see the same function set that
a PHP 8.4 build exposes there: Linux-only functions are undefined on macOS, QoS functions and the
enum are undefined on Linux, and common functions remain available everywhere.

Add one target-aware PCNTL constant source used by checker initialization, name resolution, and
constant prescan. Values must come from PHP 8.4 headers/runtime for each target rather than the host
compiler's `libc`, because Elephc may emit a different target. Cover wait flags, signal numbers and
mask modes, priority modes, waitid id types, errno aliases, siginfo codes, and Linux clone flags.
Only constants PHP defines for that target may resolve.

### Calls, outputs, and ownership

Ordinary scalar calls lower through typed `RuntimeFnId` operations. Calls with output parameters
use dedicated argument-lowering strategies so caller storage is passed and updated without losing
source evaluation order:

- `wait`, `waitpid`: status plus optional resource-usage associative array.
- `waitid`, `sigwaitinfo`, `sigtimedwait`: target-aware siginfo associative array.
- `sigprocmask`: optional previous-mask indexed array.

Bridge output records are converted into fresh Elephc arrays by target-aware runtime helpers. The
helpers must preserve COW, replace the caller's prior value exactly once, and balance all owned
strings/arrays on success, failure, exceptions, and early process exit.

`pcntl_exec` materializes null-terminated argv/envp vectors from indexed/associative PHP arrays in
source order. A successful call never returns; a failed call records errno, releases temporary
storage, emits PHP-compatible warning behavior, and returns false.

### Signals

Never call PHP code from the native signal handler. The bridge preallocates or statically owns a
bounded signal queue and the installed `sigaction` handler performs only async-signal-safe writes.
Manual `pcntl_signal_dispatch()` drains queued records while signals are masked and invokes the
registered runtime callable descriptors with `(int $signal, array $siginfo)`.

The runtime owns one retained callable descriptor or `SIG_DFL`/`SIG_IGN` value per signal.
Replacing a handler releases the previous owned descriptor; `pcntl_signal_get_handler()` returns
the callable/int with correct ownership. Dispatch is non-reentrant and defers signals that arrive
while a handler is running. Handler exceptions stop the current drain, release remaining records,
restore masks/state, and propagate normally.

`pcntl_async_signals(true)` sets a process-global flag. The signal handler marks a pending interrupt;
compiler-inserted safe points dispatch before observable long-running progress without entering PHP
from signal context. Safe points must cover function calls, loop backedges, and blocking runtime
operations, and must not run while fibers or ownership-critical runtime sections are switching.

### Eval

Magician bindings reuse the neutral contracts. Scalar/status calls may use shared boxed runtime
dispatch where the ABI matches. By-reference outputs and signal callables use explicit adapters.
Opaque eval source must make the bridge available through the same capability/link mechanism; no
eval-only reimplementation may diverge in last-error or signal-table state.

## Verification

Each function needs normal, edge, named/case-insensitive, wrong-arity, wrong-type, and failure-path
coverage where applicable. Forking tests must bound waits, always reap children, and terminate
leftovers on failure. Signal tests must cover default/ignore/callable handlers, closures and
first-class callables, siginfo, replacement, nested arrivals, exception cleanup, manual dispatch,
async dispatch, masks, alarm, and forked state.

Run bridge unit tests first, then focused codegen/error/eval tests on macOS. Run the smallest Linux
x86_64 and Linux AArch64 filters that cover each target-specific block. Before publication run the
builtin generator/audits, target-architecture audit, docs site compatibility validator, assembly
comment checker for touched emitters, `cargo build`, and `git diff --check`.
