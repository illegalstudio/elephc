---
title: "octdec()"
description: "Converts a octal string to its decimal number."
sidebar:
  order: 670
---

## octdec()

```php
function octdec(string $octal_string): mixed
```

Converts a octal string to its decimal number.

**Parameters**:
- `$octal_string` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/octdec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/octdec.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `octdec` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/octdec.md).
