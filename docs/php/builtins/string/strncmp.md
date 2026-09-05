---
title: "strncmp()"
description: "Compares the first n bytes of two strings."
sidebar:
  order: 529
---

## strncmp()

```php
function strncmp(string $string1, string $string2, int $length): int
```

Compares the first n bytes of two strings.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)
- `$length` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strncmp.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strncmp.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strncmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strncmp.md).
