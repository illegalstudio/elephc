# PHP extension bridge: producer integration and consumer backlog

- [x] Ship the ordinary C ABI library outputs and standalone PHP-shaped value helpers.
- [x] Define and implement the separate producer through a host-owned Zend adapter on PR #1862.
- [ ] Complete the parent-stack integration and deferred repository validation under the existing producer plan.
- [ ] Define a safe consumer hosting/extension-loader contract before linking a real PHP extension into a standalone program.
- [ ] Implement a consumer proof of concept with explicit value ownership, startup/shutdown and supported-target limits.
- [ ] Demonstrate both directions together only after their lifecycle contracts compose.

## Current status

As checked on 2026-10-28,
[PR #1743](https://github.com/illegalstudio/elephc/pull/1743) has merged.
[PR #1862](https://github.com/illegalstudio/elephc/pull/1862) has been rebased
onto `main`. Integration validation and fresh review are tracked under the
[producer plan](emit-phpext.md).

The active producer design and review record are Code Journal plan `3c86308a`
("Emit PHP extensions through a host-owned Zend adapter") and doc `38078d2a`.
They supersede this file's old handwritten-Zend-layout design. The six NTS/ZTS
producer checks have historical pass evidence there; complete repository
validation is explicitly deferred until the parent work completes and the PR
is retargeted. This cleanup does not execute that validation or alter its gate.

## Producer contract

`--emit phpext` is distinct from `cdylib`/`staticlib`. The C adapter is compiled
against the selected PHP host SDK and uses real Zend module, argument, value,
exception and output APIs. Validate API/build identity, NTS/ZTS, debug mode,
architecture and toolchain; do not infer compatibility from struct size alone.

Use request-owned native state, isolated activations, contained native recovery
and malloc-owned neutral transport. Restore state across callbacks and reentry;
reject unsupported cross-Fiber overlapping activations. Zend exceptions or
bailouts propagate only after native cleanup and return. There must be no Zend
bailout or Elephc longjmp across Rust frames.

Standalone `zval_pack`/`zval_unpack`/`zval_free` describe Elephc-owned PHP-shaped
memory. They do not allocate host Zend objects. Never pass that storage to Zend
as a live host zval or release host-owned storage with those helpers. The C ABI
library contract remains independent and compatible.

## Consumer work that remains

| Decision | Required proof |
|---|---|
| Extension entry and lifecycle | Choose an actual extension and document whether it requires embedding Zend, loader registration, request globals or another public C API. PHP function names are not generally exported C symbols. |
| Linking and discovery | Use a reviewed explicit input and target policy. Do not add a generic shared-object flag as if linking alone initializes an extension. |
| Value crossing | Specify borrowing, copying, mutation, references, returned owners and cleanup under the selected real host API. |
| Failure containment | Define exception/bailout, thread, request and shutdown behavior before calling the extension. |
| Proof of concept | Demonstrate one real extension function with observable results and balanced ownership; a dummy C function or successful dlopen is insufficient. |

The consumer may use lower-level public C APIs where that is the reviewed
boundary, but then document that scope rather than claiming general Zend
extension invocation. A future bidirectional example needs scalar, binary-string
and array cases plus normal/error cleanup, a clear unsupported-signature matrix,
and authoritative host/target qualification.

Permanent references: [C libraries](../docs/beyond-php/cdylib.md),
[standalone value helpers](../docs/beyond-php/zval-bridge.md), and
[roadmap](../ROADMAP.md). All compiler implementation goes through active EIR
lowering and target-aware shared support, never the removed legacy backend.
