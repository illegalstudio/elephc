# iOS target implementation specification

This working specification describes the shipped iOS implementation and the
owned-string library ABI introduced by #734. Reviewed against release candidate
`df57a9800f` on 2026-10-19, it is retained here because the SwiftUI examples
reference it. The permanent references are [targets](../docs/compiling/targets.md)
and [C libraries](../docs/beyond-php/cdylib.md).

The target support is shipped; host/device qualification and the explicitly
listed library-boundary follow-ups remain separate work. No tests, simulator
runs or physical-device checks were performed during this planning cleanup.

## Scope

The compiler supports two iOS compilation targets:

- `ios-arm64` / `aarch64-apple-ios`
- `ios-sim-arm64` / `aarch64-apple-ios-simulator`

Only AArch64 iOS targets exist. Device and Simulator x86_64 spellings are
rejected by `Target::parse()` with an arm64-only diagnostic.

Both can produce a `staticlib`, which is the normal delivery form for an Xcode
application. They can also use the shared library code path where the platform
toolchain permits it. The PHP program still uses Elephc's existing native
backend and runtime; this change does not add an interpreter or an iOS UI
runtime.

`--emit executable` is rejected for iOS code generation. Elephc's executable
shape is a CLI process, not a packaged, signed application; consumers link a
generated library into their own app host. Analysis-only `--check` and
`--emit-ir` remain available without selecting a library output.

## Target model

iOS is represented by the Apple variant on `Target`, not by a fourth
`Platform`. macOS, iOS device, and iOS Simulator share Darwin/XNU codegen and
object-format behavior, while `Target` carries the facts that genuinely differ:

- SDK name and path;
- LLVM target triple;
- Mach-O platform recorded in objects and final images;
- deployment target;
- sandbox capabilities;
- persistent cache and native-package identity.

`Target::as_str()` distinguishes `macos-aarch64`, `ios-arm64`, and
`ios-sim-arm64`. Runtime objects, native dependency receipts, and catalog
artifacts therefore cannot be reused across incompatible Apple variants.

No new `Platform` was added. Shared runtime code continues to select Darwin
constants through `Platform::MacOS`; Apple-variant decisions remain target
aware at SDK, assembler, linker, capability, and cache boundaries.

## Apple toolchain behavior

Non-macOS Apple objects are assembled through `clang` with an explicit
`-target` and `-isysroot`. The user object and cached runtime object share
`linker::assembler_command()`, preventing one from being stamped for macOS while
the other is stamped for iOS.

The linker also receives the selected SDK and an explicit
`-platform_version`. `APPLE_IOS_MIN_OS` keeps the object and image deployment
floor at `13.0`. A missing iOS SDK reports that full Xcode is required; the
Command Line Tools alone do not contain iPhoneOS or iPhoneSimulator SDKs.

Device and Simulator are separate targets even though both are AArch64. Their
Mach-O load commands are `IOS` and `IOSSIMULATOR`, and their native dependency
artifacts must never collide.

## Library output forms

`--emit cdylib` produces the platform shared-library form and uses PIC runtime
emission. `--emit staticlib` produces `lib<stem>.a` containing the user object,
the runtime object, and an archive index. Static libraries deliberately keep
direct relocations: the consuming linker merges the archive into its final PIE
image, so the dynamic loader's GOT path is unnecessary.

Both output forms are libraries for codegen purposes. They omit `main`, expose
the same lifecycle and `#[Export]` symbols, emit the same generated C header,
and use the same recoverable host boundary. Static libraries are not standalone
dependency bundles: bridge archives and managed native libraries used by the
PHP program must also be linked by the consuming Xcode target.

## Public library ABI v3

`ELEPHC_ABI_VERSION` is `3`. `is_string_return_signature()` recognizes every
non-variadic, by-value export with a declared `string` return and a fixed list
of supported `int`, `float`, `bool`, or `string` inputs. A zero-argument export
such as `render_view(): string` is valid. Arrays, objects, callables, nullable,
variadic, and by-reference export signatures are rejected.

Scalar returns retain their established C prototypes. A string return uses a
status result and two required output addresses. For example:

```c
int32_t spike_greet(const char *name_ptr, size_t name_len,
                    char **output_ptr, size_t *output_len);
```

The status values emitted in every generated header are:

| value | name | meaning |
|---:|---|---|
| 0 | `ELEPHC_STATUS_OK` | call completed successfully |
| 1 | `ELEPHC_STATUS_INVALID_ARGUMENT` | malformed public pointer/length input |
| 2 | `ELEPHC_STATUS_PHP_EXCEPTION` | PHP exception recovered at the host boundary |
| 3 | `ELEPHC_STATUS_ALLOCATION_FAILURE` | result allocation failed |
| 4 | `ELEPHC_STATUS_RUNTIME_FAILURE` | another recoverable runtime fatal occurred |

On success, `*output_ptr` is an independent owned allocation and
`*output_len` is its authoritative byte length. The wrapper copies the internal
result before publishing it, including results that alias a host input, a
literal, or temporary runtime storage. The host releases the published pointer
with `elephc_free()`. PHP strings are binary data; an optional trailing NUL is
outside `output_len` and does not make `strlen()` valid.

On failure, supplied output storage is cleared to `NULL`/zero before the status
is returned. `elephc_last_status()` exposes the most recent export status and
`elephc_last_error()` exposes a borrowed, stable diagnostic until the next
export or lifecycle reset.

There is no aggregate string return and no public register shuffle from the
internal string result pair. AArch64 returns the status in `w0`; x86_64 returns
it in `eax`. All payload publication happens through the C ABI out-parameters.

## Host-safety boundary

The post-#734 boundary applies to both `cdylib` and `staticlib` output.
String-return and scalar wrappers preserve public arguments, install a
target-specific `setjmp` recovery record, isolate concat scratch state, map
recoverable exceptions/allocation/runtime failures to status, and restore the
previous native boundary before returning to the host.

Hosts should call `elephc_init()` before the first export. Initialization resets
boundary diagnostics and arms the runtime stack-overflow floor by calling
`__rt_stack_limit_init`; AArch64 uses a real frame around that call so its link
register survives. Export wrappers also lazily arm the floor after saving all C
arguments, which keeps omitted or repeated initialization recoverable and
idempotent.

The runtime cache distinguishes two independent choices:

- relocation mode (`pic` for `cdylib`, direct for `staticlib`);
- library-boundary mode (enabled for both library forms).

Consequently, a static library reuses the host-safety semantics without being
forced through shared-library relocations.

## Native dependencies

The managed native catalog has five target names:

- `macos-aarch64`
- `ios-arm64`
- `ios-sim-arm64`
- `linux-aarch64`
- `linux-x86_64`

All eight current packages (PCRE2, zlib, OpenSSL, nghttp2, libssh2, curl,
Oniguruma and libxml2) are catalogued for both iOS targets. The authoritative
package/target matrix is `src/native_deps/catalog.rs`. Cross compilation requires
explicit `ELEPHC_NATIVE_CC`, `ELEPHC_NATIVE_AR`, and
`ELEPHC_NATIVE_RANLIB` tools appropriate for the selected SDK. Device and
Simulator compiler tuples are validated separately.

Every committed PCRE2 example lock records all five target plans. Running
`elephc native add pcre2 --target <host> --manifest-path <example>/elephc.toml`
must therefore leave the manifest and lockfile unchanged.

## iOS capability diagnostics

`system`, `passthru`, `exec`, `shell_exec`, `popen`, and `pclose` are rejected
at type-check time for iOS device and Simulator targets because their process
spawning model is unavailable in the application sandbox. The same calls remain
available on supported host targets. The diagnostic comes from each builtin's
checker hook and points at the PHP call site.

## In-tree hosts

`scripts/ios-relink-spike.sh` compiles `spike.php` as a static library and
includes the freshly generated `libspike.h` in its C host. Its string call uses
status plus `char **`/`size_t *`, checks the result, and releases the owned
buffer with `elephc_free()`.

The SwiftUI view-protocol and device-probe examples each use the conventional
`main.php` entrypoint and import thin bridging headers that include that
example's freshly generated `libmain.h`. The view wrapper's only adapter gives
the source export `dispatch` a collision-free Swift name; its inline C body is
type-checked against the generated prototype. The Swift hosts compare
`elephc_abi_version()` against the imported `ELEPHC_ABI_VERSION` macro and carry
no copied library ABI.

The earlier Simulator run (`42 hi iOS 6`) established the target, Mach-O,
static-link, and execution path before ABI v3 landed. The current host now speaks
ABI v3; replaying the line requires a macOS machine with full Xcode and a booted
arm64 Simulator.

## Verification surface

Focused tests cover:

- Apple target names parsing and round-tripping without `as_str()` collisions;
- parse-time refusal of x86_64 iOS spellings and codegen-time refusal of iOS
  executable output;
- the iOS `13.0` deployment floor and missing-SDK diagnostic;
- compile-time refusal of process-spawning builtins on both iOS variants;
- static archive creation and direct host linking;
- ABI-v3 header generation, status/out-parameter marshaling, binary strings,
  owned-buffer release, and recovered failure paths;
- explicit and lazy stack-limit initialization across the library boundary;
- native catalog target coverage and deterministic lockfiles;
- generated showcase headers compiling through the Swift bridging wrappers;
- AArch64 iOS assembly for zero-input and mixed-input string-return ABI-v3
  wrappers, independent of the CI host architecture.

CI remains authoritative for the full macOS AArch64, Linux AArch64, and Linux
x86_64 matrix. iOS SDK assembly/linking and SwiftUI execution require Xcode.

## Deliberate non-goals

The iOS implementation does not add another `Platform`, redesign staticlib
versus cdylib, or change PHP header newline behavior.

[#804](https://github.com/illegalstudio/elephc/issues/804) (implicit-magic
traversal for `__set`, `__isset`, `__unset`, and `__clone`) and
[#805](https://github.com/illegalstudio/elephc/issues/805) (remaining raw `_exit`
helpers) were still open at the 2026-10-19 review. They also apply to code inside
the generated archive and remain independent library-boundary follow-ups.

Physical-device execution still requires application packaging, provisioning,
entitlements, and signing. The compiler can build device-target objects and
archives without owning that Xcode workflow.
