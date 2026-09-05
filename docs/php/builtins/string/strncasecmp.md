---
title: "strncasecmp()"
description: "Compares the first n bytes of two strings, ignoring ASCII case."
sidebar:
  order: 528
---

## strncasecmp()

```php
function strncasecmp(string $string1, string $string2, int $length): int
```

Compares the first n bytes of two strings, ignoring ASCII case.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)
- `$length` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strncasecmp.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strncasecmp.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strncasecmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strncasecmp.md).
