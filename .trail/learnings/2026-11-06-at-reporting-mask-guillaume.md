# Restored PHP 8 `@` reporting-mask semantics

- `@` now saves the active `error_reporting()` mask on a depth-indexed stack, installs PHP 8's
  reduced suppression mask `4437`, and restores the caller's mask when the operand completes.
  A user error handler is still invoked under `@` and observes `4437`; default warning output is
  filtered by that mask rather than by a blanket suppression-depth gate.
- The previous level is restored only when the operand did not change `error_reporting()`
  itself. PHP keeps an explicit change made inside `@` (`@(error_reporting(E_ALL & ~E_WARNING))`
  leaves `30717` behind).
- Exception unwinds restore the mask as well as the suppression depth. `TryPopHandler` compares
  the live depth with the handler's saved depth and, when an `@` scope was actually unwound,
  reloads the saved mask for the restored depth before entering `catch`.
- x86_64 mask helpers use only caller-saved registers; the first draft clobbered callee-saved
  `r12`-`r14`.
- Verified against PHP 8.5 on aarch64 and linux-x86_64/qemu: handler mask 4437, post-`@` restore,
  explicit nested level changes, default warning suppression, and warning restoration after an
  exception thrown through `@`.
- New regression:
  `tests/codegen/exceptions.rs::test_error_control_exposes_php_suppression_mask_to_handlers`.
  Existing `error_control` 8 passed and `suppress` 21 passed.
