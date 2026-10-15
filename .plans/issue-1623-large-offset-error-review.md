# PR #1833 review follow-up

- [x] Reproduce the long-class-name diagnostic leak on the reviewed head.
- [x] Preserve and retire the first message-concat allocation on both architectures.
- [x] Add explanatory instruction-group comments to conversion and throw emitters.
- [x] Add a heap regression for repeated fixed-array and linked-list rejections.
- [x] Run focused final validation and prepare the verified review response.

Reviewed base: `0a5f5574158314ff03412455be71d940664c040a`.

The first `<prefix><class name>` concatenation can overflow the 64 KiB concat
scratch region. Appending the suffix allocates another buffer, and persisting
only that final buffer loses ownership of the first allocation. The error
constructor now keeps the first pointer in a dedicated frame slot, persists
and saves the final message, then frees the intermediate buffer before creating
the Throwable. Scratch pointers are ignored by the existing heap-free helper.
ARM64 uses a 64-byte frame; x86_64 uses a 48-byte local area. Both remain aligned.

The regression declares a class with a name over 64 KiB, catches and retires
three errors from each SPL container, and checks the complete diagnostic lengths
against PHP plus a clean heap summary. The exact reviewed native compiler emits
the correct six PHP lines but leaves 6 blocks / 393,696 bytes live. This direct
object-source fixture isolates message construction.

An earlier prototype passed the same `mixed` function parameter to both methods;
its second call hit a separate existing consumed-box/bad-refcount path. That
prototype was replaced with the direct object source before drawing conclusions
about the message leak. The failed prototype log is retained as invalid evidence
for this review's leak claim.

Evidence directory: `/private/tmp/elephc-1833-review` (ephemeral).

Final verification:
- The new heap regression passes with IR optimization on and off, PHP cross-check enabled.
- Four `spl_offset` tests pass, including float diagnostics and exceptional cleanup.
- Linux x86_64 and ARM64 execute the large-name and scratch-buffer controls with optimization
  on/off: eight PHP-exact, heap-clean executions using this source's runtime and program assembly.
- macOS ARM64, iOS device and iOS Simulator assemble the runtime plus both optimized/unoptimized
  large-name programs: nine object files. This is compile/emitter evidence, not iOS execution.
- `cargo build -p elephc` is warning-free; assembly comment alignment and `git diff --check` pass.
- One initial ARM64 probe used the wrong Docker image name (`aarch64` rather than `arm64`);
  the corrected image completed successfully. The initial invocation is not runtime evidence.
