---
title: "strripos()"
description: "Finds the numeric position of the last case-insensitive occurrence of a substring."
sidebar:
  order: 532
---

## strripos()

```php
function strripos(string $haystack, string $needle, int $offset = 0): mixed
```

Finds the numeric position of the last case-insensitive occurrence of a substring.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strripos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strripos.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strripos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strripos.md).
