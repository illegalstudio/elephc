---
title: Native debugger should be a debug-build runtime, not a second profiler
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 120 }
touches:
  - .plans/native-debugger.md
  - .plans/README.md
---

## Decided

- The primary architecture is an in-process PHP debug runtime for `--debug`
  builds (Option B): compact metadata, per-site breakpoint flags, a cheap
  per-line guard, a preserving slow path, a shadow stack, and a DAP adapter.
  `--debug-info`/lldb stays as a complementary native-crash mode.
- Do not rebuild `elephc monitor`: the profiler already owns exact call timing,
  allocations, retained objects, DB/network wait, call edges, sampling, remote
  capture, HTML and Perfetto. The debugger owns correctness (breakpoints,
  stepping, values, exceptions, coverage, postmortem call history).
- Reuse the existing contracts: `@fn`/`@src` markers, `Span::source_id`,
  `LocalSlot`/`FunctionParam`, `FrameLayout::local_offsets`, the bridge table,
  `elephc-probe` build-key handshake, and the monitor HTML renderer.
- A DAP adapter plus a terminal client is the UI strategy. No dedicated desktop
  UI. VS Code/Neovim use DAP; DBGp for PhpStorm is optional/deferred
  (elephc-jwio.2), not v1.
- ADR at `.plans/adr-native-debugger.md` (D1-D12) is accepted as of 2026-11-05,
  with macOS execution, shared-stack extraction and metadata-schema review as
  implementation gates. Developer usage walkthrough at
  `.plans/native-debugger-usage.md`.
- Product feedback removed the `trace` command: call values belong to
  `elephc monitor --calls`, rendered in the existing monitor HTML with the
  performance views. Coverage belongs to `elephc test --coverage`; no separate
  `trace`/`coverage` subcommands. `elephc_trace_start/end` are named segment
  markers inside an active capture, not standalone capture triggers.
- No xdebug API dependency. The source helpers are `elephc_break()`,
  `elephc_dump($value, $label, $to=auto|stderr|terminal|html)`, and
  `elephc_trace_start/end($name)` for named segments, declared as Elephc
  extensions and hidden by `--strict-php`.
- The three build modes are distinct: normal, `--debug` (interactive),
  `--with-monitoring` (performance). `--debug` must not appear in production.
- Interactive debugging is gated off on iOS device; trace/coverage metadata
  still ships there. The five-target emitter suite must cover every new hook.

## Tried and ruled out

- **lldb-only over DWARF** as the primary path: cheap for breakpoints and native
  crashes, but PHP locals, arrays/objects, fibers, web workers and PHP-style
  stepping all require custom metadata and runtime knowledge anyway. Keep it as
  a later companion.
- **Patch `brk`/`int3` at line sites**: zero dormant cost, but needs
  `mprotect`, instruction-cache flushes, code-signing/hardened-runtime handling,
  and does not work on iOS. The per-site flag + guard is the portable v1.
- **Full VM/bytecode debugger**: duplicates the eval/Magician path and changes
  the native execution model; rejected.
- **Reusing the profiler's shadow stack as the debugger stack**: selective
  `--with-monitoring` instrumentation means it is not a complete PHP stack, and
  monitoring must stay optional. Keep the hook families independent.

## External review

- GLM-5.3-Flash, MiniMax-M3.1-Flash and Mistral Large 4 (via Ollama) all
  returned GO with reserves. None rejected the in-process architecture or
  proposed a VM.
- Consensus corrections: reuse/extract the `elephc-instr` activation stack
  instead of duplicating it; treat the AST optimizer as part of the debug
  contract; fix included-file debug artifacts first (`elephc-jwio.1`); derive
  the ABI save set from target helpers; inspect values without refcount/COW
  mutation; stop at entry until `configurationDone`; budget 8-12 weeks for MVP,
  not 4-6. DBGp/PhpStorm is optional later, not v1.

## Watch out

- `--debug-info` currently emits only a line table and `DW_TAG_subprogram`, and
  `@src` has no file id. `--source-map` also has one `source`, so included-file
  breakpoints/line attributions collapse to the entry file. This is a real
  defect in both artifacts, not just a debugger gap. Reproduced on
  `db62f09226`: `main.php` requiring `lib.php` emits `.file 1 "main.php"` only,
  while `lib.php`'s line 3 emits `.loc 1 3 5`; `main.map` names only `main.php`.
  `Span::source_id()` and `Span::source_path()` already carry the information;
  the markers and schemas need to expose it.
- The slow path must preserve every caller-saved integer and SIMD register plus
  flags. A Rust hook call will clobber them; the wrapper belongs in
  `src/codegen_support/abi/` and needs assembly-level tests on AArch64 and
  x86_64.
- Locals are not all raw frame slots: ref-cell and dynamic representations have
  a separate state word. Metadata must carry the storage kind and enough type
  information to format values, or the debugger will print pointers as ints.
- `--debug` should disable EIR optimization/inlining by default. AST folding/DCE
  can still remove lines; the ADR must state the contract and the "unverified
  breakpoint" behavior.
- Fibers/generators and `--web` workers need per-fiber shadow stacks and
  request-scoped stopping. Do not start with stop-the-world assumptions.
- Monitor and debugger overhead are different products: dormant debug guards
  are acceptable in a debug build, not in a production `--with-monitoring`
  binary. Keep `--debug` out of production artifacts and document the boundary.
- xdebug's Zend extension ABI is not reproducible here, and the product does
  not want an xdebug API. Expose only the Elephc-native `elephc_break()` and
  `elephc_dump()`; keep DBGp/PhpStorm optional and late.
