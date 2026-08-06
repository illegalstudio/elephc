# iOS target specification

Tracking issue: [illegalstudio/elephc#662](https://github.com/illegalstudio/elephc/issues/662)
Branch: `feat/ios-target`
Status: delivered — compiled PHP runs on the iOS Simulator

---

## 1. Purpose

Let elephc emit an **arm64 static library of AOT-compiled PHP** that an iOS application links into its Xcode project and calls through the C ABI `--emit cdylib` already produces.

The UI stays in Swift. PHP supplies domain logic — validation, pricing, business rules — shared with the server and executed offline on device.

## 2. Non-goals

- **"Write your iOS app in PHP."** elephc ships no interpreter and no source. There is no runtime `eval`, no dynamic `include`, no framework boot on device.
- **Shipping an app shell.** The deliverable is a `.a` the app developer links. Code signing, entitlements, App Store submission and the Xcode project belong to the app, not to elephc.
- **Android, in this spec.** Android is reachable and shares most of the groundwork, but its cost centre is different (see §4) and it is out of scope here.
- **Migrating raw syscalls to libSystem.** Deliberately deferred — see §8.

## 3. What the tree already provides

Facts verified against `origin/main` (`d14d1ee18e`):

| Capability | Where | Note |
|---|---|---|
| C-ABI export trampolines | `src/codegen_support/cdylib.rs` | `#[Export]` PHP functions get unmangled C symbols via tail-branch |
| Lifecycle entry points | same | `elephc_init` / `elephc_shutdown` / `elephc_last_error` / `elephc_free` — currently v1 stubs |
| Shared-library emit | `Emit::Cdylib`, `src/codegen/mod.rs:86` | `-dylib` + `@rpath` install name on Darwin |
| PIC codegen | `Emitter::new_pic`, `src/codegen/mod.rs` | cross-object data through the GOT; required for PIE |
| Both mobile arches | `Arch::{AArch64, X86_64}`, `platform/target.rs:29` | no new ISA backend needed |
| Darwin SDK resolution | `src/linker/sdk.rs` | already drives `xcrun` |
| Host import mechanism | `src/web_prelude.rs` | 112 host symbols declared as PHP signatures, link-resolved against `crates/elephc-web` |

The last row matters beyond `--web`: it is the proven pattern for a future **native bridge** (camera, biometrics, push). Inbound calls — including string returns, e.g. `web_prelude.rs:184` `function elephc_web_header_name(int $i): string;` — already work. Outbound does not (§6).

## 4. Why iOS before Android

| | iOS | Android |
|---|---|---|
| Arch | arm64 — primary backend | arm64 |
| Object format | Mach-O, **identical to macOS** | ELF |
| libc | **the same libSystem as macOS** | Bionic — each of ~292 `bl_c()` sites is a question mark |
| Toolchain | `xcrun`/`ld`, already driven | NDK clang, new plumbing |
| Test loop | **Simulator is arm64 on Apple Silicon** — no device, no dev account | emulator or device |
| Extra constraints | code signing | 16 KB page alignment, JNI glue |

The libc row is the argument. On iOS the long tail is nearly empty because it is the same C library elephc already targets. On Android that tail *is* the project.

## 5. Architecture decision — iOS is a Darwin sub-variant, not a new `Platform`

### Current shape

```rust
pub enum Platform { MacOS, Linux, Windows }   // platform/target.rs:21
pub struct Target { pub platform: Platform, pub arch: Arch }  // :36
```

### Rejected: `Platform::IOS`

Measured cost: **~181 `Platform::MacOS` match arms across 54 files**. Every one would grow an iOS arm returning a value *identical* to macOS — same syscall numbers, same `O_NONBLOCK`, same `SOL_SOCKET`, same Mach-O conventions. Pure churn, and it turns every future OS constant into a four-way match.

### Adopted: a variant field on `Target`

```rust
pub enum AppleVariant { MacOS, IOS, IOSSimulator }
pub struct Target { platform: Platform, arch: Arch, apple_variant: AppleVariant }
```

`Platform::MacOS` keeps answering every ABI and constant question correctly and for free. Only the linker, the native-dependency toolchain and a small set of capability gates read the variant.

**Measured cost — much lower than a field addition usually implies.** There are **zero literal `Target { … }` constructions** in the tree. Every `Target` is built through one of three constructors, and only **7 of their 131 call sites are production code**:

| Constructor | Total | Production | Production sites |
|---|---|---|---|
| `Target::new` | 100 | **1** | `codegen_support/abi/registers.rs:198` |
| `Target::parse` | 7 | **2** | `cli.rs:530`, `native_deps/cli.rs:98` |
| `Target::detect_host` | 24 | **4** | `cli.rs:222`, `native_deps/toolchain.rs:106`, `native_deps/recipes/{pcre2.rs:44,zlib.rs:37}` |

The remaining 124 sites are tests. Adding a field with a sensible default is therefore a contained change, not a sweep.

Further confirmation of the decision: `Target::supports_current_backend()` (`platform/target.rs:602`) matches only on `(Platform, Arch)`, and `(MacOS, AArch64)` is **already `true`**. The central gate at `pipeline.rs:409` needs no change at all. The rejected `Platform::IOS` route would have forced it open.

### Where the variant must actually be read

`Target` derives `Copy, PartialEq, Eq` but **not `Hash`**, and is never a map key. The real serialization surface is `as_str()` / `Display`, and it is wider than first assumed — three distinct persisted keys would collide between a macOS and an iOS build if the variant is not encoded:

- `runtime_cache.rs:152` `runtime_cache_file_name()` — the cached runtime object filename;
- `native_deps/receipt.rs:24-40` `ArtifactReceipt` — a `Serialize`/`Deserialize` JSON persisted to disk with a `target: String` field fed from `as_str()`;
- `native_deps/catalog.rs:124-133` `ensure_target()` — per-package static `supported_targets` lists.

Sites that must branch on the variant:

| Site | Why |
|---|---|
| `linker/sdk.rs:16-29` `macos_sdk_path()` | runs `xcrun --show-sdk-path` with no `--sdk`, resolving the default (macOS) SDK |
| `linker/sdk.rs:53-68` `macos_sdk_version()` | hardcodes `--sdk macosx`, falls back to `"15.0"` |
| `linker/command.rs:108-164` `render_macos_command` | hardcodes `-platform_version macos <v> <v>`, reusing one version for both min-OS and SDK |
| `native_deps/toolchain.rs:183-214` `validate_tuple` | requires the compiler's `-dumpmachine` triple to contain **both** `apple` and `darwin`, and synthesises `"{arch}-apple-darwin"`. An iOS cross-compiler reports `arm64-apple-ios` — **this check rejects it today** |
| `cli.rs:96`, `Target::parse` error arm | user-visible target lists |

Sites that must keep seeing plain "macOS" — no variant reading, because XNU and Mach-O are identical on arm64: every `Platform::MacOS =>` syscall number / struct offset / errno / flag constant in `impl Platform`; `php_os_name()` → `"Darwin"` (correct for iOS too); `extern_symbol()` and `darwin_arch_name()`; `assembler_cmd()`/`linker_cmd()` staying `as`/`ld`; `runtime_cache.rs:96-98`; `linker/mod.rs` `dsymutil` / `archive_dedup` / Homebrew paths.

## 6. The blocking gap — `#[Export]` cannot return a string

```
src/exports.rs:155  is_v1_param_type  → Int | Float | Bool | Str    ← Str accepted IN
src/exports.rs:163  is_v1_return_type → Int | Float | Bool | Void   ← no Str OUT
```

`cdylib.rs` states it outright: `elephc_free` is a stub *"until string-return marshaling lands"*.

Every realistic mobile payload — serialized view tree, JSON result, domain object — **is a string**. An embedded elephc library can currently return only a number. This gap, not any iOS-specific concern, is what blocks the whole embedding story.

It is also **platform-independent**: it serves macOS, Linux, Android and iOS alike, it is testable entirely on macOS today, and it forces the ownership decision (`elephc_free` becoming real) to be made once, deliberately, rather than under demo pressure.

Hence it is Lot 1, ahead of any iOS-specific work.

### 6.1 What a `Str` actually is

There is no Zend-style string object. A `Str` is a raw **pointer + length** byte string (not UTF-8) carried in a register pair, and its provenance decides who owns it:

| Provenance | Storage | Owned? |
|---|---|---|
| Literal | `.rodata` | never — must not be freed |
| Scratch | `_concat_buf` bump arena with `_concat_off` cursor (`runtime/strings/concat.rs:13-82`) | no — **invalidated by the next concat** |
| Persisted | `__rt_str_persist` copies into an `__rt_heap_alloc` block, kind tag `1` (`runtime/strings/str_persist.rs:18-88` arm64, `:95-144` x86_64) | yes |

Heap blocks carry a 16-byte header before the user pointer — `[size:4][refcount:4][kind:8]` (`runtime/arrays/heap_alloc.rs:21`). Kinds: `1` owned string, `2` indexed array, `3` hash, `4` object, `5` boxed Mixed, `6` throwable.

**Strings are move-semantics, not shared-ownership.** `__rt_decref_any` dispatches kind `1` to `__rt_heap_free_safe`, which validates liveness and then frees **unconditionally** — it does not decrement (`runtime/arrays/decref_any.rs:65-66`, `runtime/arrays/heap_free.rs:266-300`). Only boxed Mixed (kind 5) has genuine refcounting. `str_persist.rs:36-37` records why: a zero-length early-return was removed precisely because it let two owners alias one buffer into a double-free.

### 6.2 Two register mismatches that would corrupt silently

The internal string-return pair is `string_result_regs` (`abi/registers.rs:97-104`): **arm64 `(x1, x2)`**, **x86_64 `(rax, rdx)`**.

**Return path — arm64 breaks.** AAPCS64 returns a 16-byte aggregate in `(x0, x1)`. Today's trampoline is a blind tail-branch (`cdylib.rs:108-113`), so the host would read `x0` — an unrelated value — as the pointer, and the real pointer as the length. Fix: a non-tail trampoline, `bl` then `mov x0, x1` / `mov x1, x2` / `ret`. x86_64 needs nothing: `(rax, rdx)` is exactly the SysV convention for a two-INTEGER-member 16-byte struct return, so the tail `jmp` stays valid.

**Free path — x86_64 breaks.** The C first argument arrives in `rdi`, but `__rt_heap_free_safe` expects the pointer in `rax` (`runtime/arrays/heap_free.rs:311`, `:540-554`). Fix: `mov rax, rdi` before the tail `jmp`. arm64 needs nothing: `x0` is both the C argument register and the helper's input.

The pain is symmetric and neither side is covered by the other's tests — which is exactly how this would have shipped as silent corruption on the arm64 path that iOS depends on.

### 6.3 ABI decisions

**Return `(ptr, len)` by value in the existing register pair**, host releases through `elephc_free`. This is the only option requiring no new runtime mechanism — a caller-supplied buffer needs a two-phase size probe, and a NUL-terminated `const char*` cannot represent PHP byte strings containing `\0`.

Three decisions the mapping forces:

1. **Persist every `Str` returned from an export, unconditionally — and do it in lowering, not in the trampoline.** `persist_scratch_return_string` (`ir_lower/stmt/mod.rs:2522-2544`) only persists values produced by scratch-categorised ops (`ir_lower/expr/mod.rs:673-686`). So `return $param;` can hand the host back *the very pointer the host passed in*. Today `heap_free_safe` happens to reject out-of-heap pointers silently, but that is an implementation accident, not a contract. An ABI where the host sometimes owns the result and sometimes does not is unusable.

   The obvious shortcut — calling `__rt_str_persist` from the trampoline — is wrong: on the common path the value has *already* been persisted by lowering, so a second persist allocates a fresh block and **leaks the first**. The persist must happen exactly once, which means lowering has to know the enclosing function is exported.

   That is the real cost of Lot 1, and it is wider than the signature-list change implies: **`ir_lower` currently has no notion of exports at all** (zero references), and `exports::collect` runs at `pipeline.rs:420`, ahead of both `lower_program_*` call sites (`:467`, `:502`). The set of exported names therefore has to be threaded into the lowering context. Feasible and correctly ordered — but it is the design decision Lot 1 turns on, not a five-line edit.
2. **Translate `NULL_SENTINEL` to a real C `NULL`** at the trampoline boundary. The internal "no string" marker is `0x7fff_ffff_ffff_fffe` (`sentinels.rs:62`), not a null pointer; leaking it to a C caller turns any deref or `elephc_free` into a crash.
3. **No NUL-termination guarantee.** PHP strings are byte strings. The host must use the returned length; `strlen()` breaks silently on embedded `\0`. To be stated in the header docs, not merely implied.

## 7. Work packages

### Lot 0 — iOS relink spike
**No compiler change.** Emit assembly with `--emit-asm`, assemble with `as`, then relink the user object plus the cached runtime object by hand against the iPhoneSimulator SDK:

```
ld -arch arm64 -dylib -o libfoo.dylib foo.o <cache>/runtime-*.o \
   -lSystem -syslibroot $(xcrun --sdk iphonesimulator --show-sdk-path) \
   -platform_version ios-simulator <min> <sdk>
```

Run against a real SDK, and it found the thing it existed to find.

**The premise was wrong.** Relinking a macOS-assembled object against the iOS SDK does not work:

```
ld: building for 'iOS-simulator', but linking in object file built for 'macOS'
```

A Mach-O object records the platform it was **assembled** for. `as -arch arm64` stamps macOS and has no way to say otherwise, so the earlier claim here — that only `-syslibroot` and `-platform_version` remained untested — was false. Nothing short of a real SDK could have surfaced it, which is precisely what this package was for.

Fixed in the compiler rather than worked around in the script: non-macOS Apple targets assemble through `clang` with `-target` and `-isysroot`, and the user object and cached runtime object now share one `linker::assembler_command` instead of building it separately. They must agree — the link fails on whichever one disagrees. `APPLE_IOS_MIN_OS` is likewise a single constant, because the assembler writes it into each object's `LC_BUILD_VERSION` and the linker into the image's `-platform_version`, and a mismatch between those is itself a link error.

`scripts/ios-relink-spike.sh` now exercises the shipping path — `--target ios-* --emit staticlib` — rather than a hand-rolled relink, since there is no longer anything to work around.

**Accept — met.** Compiled PHP runs on iOS:

```
==> compiling for ios-sim-arm64
    IOSSIMULATOR spike.o
    IOSSIMULATOR runtime-v0.26.2-ios-sim-arm64-…​.o
==> linking a arm64-apple-ios13.0-simulator host against the archive
     platform IOSSIMULATOR   minos 14.0   sdk 26.5
==> running inside booted simulator 9A3F533A-…​
    42 hi iOS 6
```

iPhone 17 Pro, iOS 26.5, arm64. `spike_add(40, 2)` returned 42, `spike_greet("iOS", 3)` returned `"hi iOS"` with length 6, and `elephc_free` released the buffer without incident — the int export, the string export and the ownership contract, all on device-class arm64.

The device target builds and links identically (`platform IOS`); running it needs provisioning and a signed bundle, which is an app-packaging concern rather than a compiler one.

An isolated `XDG_CACHE_HOME` matters more than it looks: the runtime object's cache key encodes the program's runtime feature set, so the shared cache holds several candidates and a "newest match" glob would silently pick the wrong one.

**Prereq:** full Xcode, licence accepted. The Command Line Tools carry no iOS SDK, and an unaccepted licence blocks `xcrun`, `cc` and `as` outright — not just iOS work.

### Lot 1 — `Str` return on `#[Export]`
Allow `PhpType::Str` in `is_v1_return_type`, implement C-ABI marshaling, settle ownership so `elephc_free` stops being a stub.

**Accept:** an exported PHP function returns a string to a C caller; the buffer is released through `elephc_free`; no leak under the existing GC-stats tooling; round-trip covered by a test naming the export.
**Blocked by:** nothing platform-related. Needs a working cargo build.

### Lot 2 — View-protocol spike, on macOS ✅ done
Delivered as `examples/swiftui-view-protocol/`. A native macOS app whose entire interface is decided by compiled PHP: `render_view(): string` returns a serialized view tree, `dispatch(string $action): string` returns the next one after an event, and Swift knows four node types and nothing else.

It needs **only the Command Line Tools** — `swiftc` ships with them and SwiftUI is a system framework — so there is no Xcode install and no `.xcodeproj` in the loop. `run.sh` compiles both sides, assembles and ad-hoc-signs a `.app`.

Why this shape is the one that fits AOT: a template engine must *evaluate itself* on the device, which requires a PHP runtime there; a tree *generator* compiles once and ships as machine code. It is the single corner of the UI problem where being ahead-of-time costs nothing.

Two findings worth carrying to the iOS host:

- **`ElephcStr` must be a C type.** Swift rejects a Swift-declared struct in a `@convention(c)` signature — only a C type carries the guarantee that the value rides the platform's aggregate-return registers. Hence `elephc_abi.h` and `-import-objc-header`. The same constraint applies to any Swift host, on any Apple platform.
- **`@main` needs `-parse-as-library`**, or swiftc treats the file as a script.

**Accept — met.** `--selftest` runs the round trip headlessly, so the example is verifiable without a display:

```
initial=nothing yet after++=2 items after-=one item reset=nothing yet
PASS: the view tree, the string ABI and PHP-side state all round-trip
```

That asserts more than rendering: the tree decodes into a typed model with the expected shape, a string crosses in both directions, `elephc_free` runs on every returned buffer, and the counter's state persists in the loaded library's own memory across host calls — `counter()` uses a function `static`, and Swift holds no counter at all.

**Depended on:** Lot 1. Nothing here is macOS-specific; the same library and protocol drive a UIKit or SwiftUI host on iOS once that target links.

### Lot 3 — `Target` Apple variant, then `Emit::Staticlib`
The enabling refactor (§5), then the delivery form.

**A staticlib must NOT reuse the cdylib PIC path.** An earlier draft of this spec claimed it should, on the grounds that "iOS mandates PIE". That conflated two different things and was wrong. `pic_data_refs` (`emit.rs:58-62`) routes cross-object symbol access through the **GOT**; its own comment (`emit.rs:21-26`) states the reason — shared-library output, *"where the loader cannot resolve cross-object relocations at dlopen time"*. That is about dynamic loading and symbol interposition, not position independence. The non-PIC path is already PC-relative (`adrp`+`add` on arm64, `lea [rip+sym]` on x86_64) and already produces PIE executables on macOS/arm64 today. A `.a` is merged once into the app's final binary by the app's own linker, exactly like the executable path.

So `Emit::Staticlib` takes `Emitter::new(target)`, and `runtime_pic` (`pipeline.rs:596`) stays `matches!(emit, Emit::Cdylib)`. Lot 0 settles it empirically: hand-linking a non-PIC object against the iPhoneSimulator SDK either works or does not.

Sites a third `Emit` variant must touch:

| Site | Action |
|---|---|
| `cli.rs:475-476` `parse_emit` | add `staticlib`/`static`/`lib` aliases |
| `cli.rs:387` | reject `--web` for Staticlib as for Cdylib |
| `codegen/mod.rs:175-177` | `Emitter::new` (**not** `new_pic`) |
| `codegen/mod.rs:269` | join the `emit_cdylib_exports` branch — same trampolines and lifecycle symbols |
| `codegen/mod.rs:305` | do **not** extend — ELF dynamic-symbol visibility is `.so`-only |
| `codegen/block_emit.rs:95-97` | join the branch that skips `main` |
| `pipeline.rs:596` `runtime_pic` | leave as-is |
| `pipeline.rs:764-770` `output_paths` | new branch producing `lib<stem>.a` |
| `pipeline.rs:720-733` | route to an archive step instead of `link_with_plan` |
| `linker/command.rs` | untouched — a staticlib never invokes `ld` |

**The existing `ar` flow is a precedent, not reusable logic.** `archive_dedup.rs` deduplicates *already-built bridge* archives before `ld -force_load`; bridge `.a`s themselves come from `cargo build -p <crate>` (`linker/bridges.rs:375-397`). What Lot 3 needs is a single `ar rcs <out.a> <obj> <runtime_obj>` — self-indexing, no member dedup. What carries over is the `run_tool()` idiom (`linker/command.rs:93-105`), nothing more. Bridges and managed native packages stay separate `.a`s for the consuming Xcode project to link.

Target strings follow the existing dual convention (short `platform-arch` plus an LLVM-style triple): `ios-arm64` / `aarch64-apple-ios`, and `ios-sim-arm64` / `aarch64-apple-ios-simulator`. `test_target_parse` (`platform/mod.rs:27-40`) and the shared integration fixture `tests/codegen/support/platform.rs:14-21` extend with them.

Includes the native-dependency path: `native_deps/toolchain.rs` `validate_tuple` currently **rejects** an iOS cross-compiler outright, and `native_deps/catalog.rs` `supported_targets` must list the iOS strings before pcre2/zlib can be built for the target.

**Accept — met, minus the SDK.** `--emit staticlib` produces a `lib<stem>.a` carrying the user object, the runtime object and a symbol index; a C host links it directly — no `dlopen` — and runs. That link succeeding into a PIE executable is what proves the non-PIC decision above.

`--target ios-arm64` runs the **entire** pipeline and assembles a real runtime object, cached under its own key alongside `macos-aarch64` (86 of those coexist without collision). Only the final link stops, on the absent SDK, with a diagnostic that names the SDK and points at full Xcode rather than looping back to `xcode-select --install`.

What is left for Lot 3 is therefore one command on a machine with Xcode, not more code.

Deliberately left for a real consumer: `native_deps/catalog.rs` `supported_targets` still lists no iOS strings, so pcre2/zlib cannot yet be built for the target. The toolchain validator no longer rejects an iOS triple, which was the blocking half.

### Lot 4 — Capability gating
Six builtins depend on `fork`/`exec` and are unusable in the iOS sandbox. `proc_open` and its family are **not** in this list — they do not exist in the compiler at all (zero occurrences in `src/`); the note to carry forward is that whenever they are added, they must ship with the gate from day one.

| Builtin | Declaration | Backed by |
|---|---|---|
| `system` | `builtins/system/system.rs:12-20` | `bl_c("system")` directly |
| `passthru` | `builtins/system/passthru.rs:12-21` | `bl_c("system")` directly |
| `exec` | `builtins/system/exec.rs:12-21` | `__rt_shell_exec` → `popen()` |
| `shell_exec` | `builtins/system/shell_exec.rs:12-21` | same helper |
| `popen` | `builtins/io/popen.rs:18-29` | `bl_c("popen")` |
| `pclose` | `builtins/io/pclose.rs:15-26` | `__rt_pclose` |

**Insertion point — the per-builtin `check:` hook, not the backend.** The builtin architecture is now one `builtin!` macro declaration per file under `src/builtins/<area>/`, collected by `inventory` in `builtins/registry.rs`; `types/signatures.rs` holds only the `FunctionSig` struct. `popen.rs:23` and `pclose.rs:20` **already carry a `check:` hook**, and `Checker.target_platform` already exists (`types/checker/mod.rs:61`) and is already consulted for a target-dependent decision in `require_macos_builtin_library()` (`types/checker/builtins/mod.rs:50-56`) — though only to *add* a requirement, never to reject. Pattern to imitate verbatim: `builtins/spl/spl_object_id.rs:27-37`, returning `Err(CompileError::new(cx.span, …))`, which propagates through `check_builtin` to `pipeline.rs`'s `process::exit(1)`.

This gives an exact PHP source position (`file:line:col` via `errors/report.rs`), which the WASM-style backend audit cannot: `WasmError` is not wired to `CompileError`/`Span` and reports only `collection::function block#N instruction#M`. The exhaustive-match model remains the right guarantee *against omission* — worth pairing with the hooks so a newly added process builtin cannot compile without a decision — but it is not the right diagnostic on its own.

Reaching the variant required the checker to hold the whole `Target` rather than just a `Platform`. `check_with_target` already received one and dropped the variant on the floor.

**Accept — met.**

```
error[1:7]: shell_exec() cannot be compiled for ios-arm64: that sandbox forbids
spawning a process, so the call would always fail at run time
```

All six are refused with an exact position, and the same source still compiles for the host — this is a target capability gate, not a removal of the builtin. `pclose(popen(…))` reports `popen`, its inner call being evaluated first.

The exhaustive-match guarantee was **not** adopted. It would prevent a future process builtin from being forgotten, but the WASM equivalent shows the price: its errors can only cite block and instruction indices, having no wiring to `CompileError`/`Span`. An exact source position is worth more here than compiler-enforced completeness, and the guard's doc comment carries the list forward instead.

## 8. Deliberately deferred — raw syscalls to libSystem

The runtime issues **225 `.syscall(N)`** calls, all funnelled through one choke point, `Emitter::syscall()` in `src/codegen_support/emit.rs`.

They *work* on iOS and in the simulator. Apple's supported ABI is libSystem, so this is a **long-term supportability risk, not a functional blocker** — Apple has broken the direct syscall ABI before, which is why Go migrated to libSystem on Darwin.

The trap is not the site count, it is the register contract. `svc #0x80` clobbers x0 and x16 only; `bl _write` clobbers x0–x17 and LR per AAPCS. All 225 sites were written under the syscall convention, so any value live in x1–x15 across the call would be silently destroyed. A naive substitution inside `Emitter::syscall()` produces diffuse corruption, not clean crashes.

Clean fix: one save/restore wrapper at the choke point, validated by diffing syscall-mode against libSystem-mode output across the codegen suite — measurable on macOS with no device.

Worth doing on its own merits as Darwin debt. **Not a prerequisite** for proving the rest, and not to be paid before Lots 0–2 have said the rest holds.

## 9. Risks and open questions

1. **Three persisted keys** must incorporate the Apple variant (§5) — the runtime-object cache filename, the `ArtifactReceipt` JSON, and the native-dependency catalog — or macOS and iOS builds collide silently on shared state.
1b. **Native dependencies are a second front.** `native_deps/` builds pcre2 and zlib from source per target; its toolchain validator rejects a non-`darwin` Apple triple today. Any PHP program reaching PCRE or zlib on iOS depends on this path, so it cannot be deferred past Lot 3.
2. **String ownership model** (Lot 1) is an ABI commitment. Options — return `ptr+len` and require `elephc_free`, versus copy into a caller-supplied buffer — must be weighed against the actual internal string representation and refcounting before choosing.
3. **Static vs dynamic delivery.** A `.a` avoids the embedded-framework signing dance entirely. A `.dylib` in an app bundle must live under `Frameworks/` and be signed separately. Lot 3 chooses static for that reason; revisit only if a consumer needs dynamic loading.
4. **Simulator vs device divergence.** The simulator is arm64 on Apple Silicon but is *not* the device: it uses a different platform load command and a host kernel. Lot 0 passing on the simulator does not by itself prove the device path.
5. **Scope honesty.** None of this creates new PHP capability. It relocates an artefact that already exists. It is a different bet from builtin/framework coverage, and should be judged as one.

## 10. Environment prerequisites

- **Full Xcode.** The dev machine currently reports `/Library/Developer/CommandLineTools`; `xcrun --sdk iphonesimulator` cannot locate an SDK. Lot 0 is blocked until this is installed.
- **Disk headroom.** Lots 1–4 need cargo builds; the dev machine has been fluctuating between 3 and 5 GB free, with roughly 31 GB held by accumulated git worktrees.
- **Build serialisation.** Concurrent cargo invocations relink the binary underneath a running suite and produce mass false failures. One cargo command at a time.
