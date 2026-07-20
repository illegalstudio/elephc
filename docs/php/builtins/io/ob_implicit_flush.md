---
title: "ob_implicit_flush()"
description: "Turns implicit flush on/off."
sidebar:
  order: 199
---

## ob_implicit_flush()

```php
function ob_implicit_flush(bool $enable = true): bool
```

Turns implicit flush on/off.

**Parameters**:
- `$enable` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/ob_implicit_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_implicit_flush.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ob_implicit_flush` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_implicit_flush.md).

