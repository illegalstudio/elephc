---
title: "elephc_break()"
description: "Stops at the caller's source line when an interactive or DAP debugger is attached; a no-op otherwise."
---

## elephc_break()

```php
function elephc_break(): void
```

Stops at the caller's source line when an interactive or DAP debugger is attached; a no-op otherwise.

**Parameters**: none.

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `elephc_break` is implemented in the compiler, see [the internals page](../../../internals/builtins/debug/elephc_break.md).
