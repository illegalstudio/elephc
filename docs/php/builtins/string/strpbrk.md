---
title: "strpbrk()"
description: "Returns the suffix beginning at the first byte found in a character list, or false."
sidebar:
  order: 1056
---

## strpbrk()

```php
function strpbrk(string $string, string $characters): mixed
```

Returns the suffix beginning at the first byte found in a character list, or false.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strpbrk.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strpbrk.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strpbrk` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strpbrk.md).
