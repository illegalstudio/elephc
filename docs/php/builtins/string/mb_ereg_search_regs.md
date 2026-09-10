---
title: "mb_ereg_search_regs()"
description: "Returns the next multibyte regex match and captured groups, preserving numeric and named keys."
sidebar:
  order: 818
---

## mb_ereg_search_regs()

```php
function mb_ereg_search_regs(?string $pattern = null, ?string $options = null): array|false
```

Returns the next multibyte regex match and captured groups, preserving numeric and named keys.

**Parameters**:
- `$pattern` (`?string`), default `null`, optional
- `$options` (`?string`), default `null`, optional

**Returns**: `array|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search_regs.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search_regs.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_search_regs` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_search_regs.md).
