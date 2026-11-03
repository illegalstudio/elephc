---
title: "strrchr()"
description: "Returns the suffix starting at the final occurrence of the needle's first byte, or false."
sidebar:
  order: 1060
---

## strrchr()

```php
function strrchr(string $haystack, string $needle): mixed
```

Returns the suffix starting at the final occurrence of the needle's first byte, or false.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strrchr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strrchr.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strrchr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strrchr.md).
