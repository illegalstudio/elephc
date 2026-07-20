---
title: "strpos()"
description: "Finds the numeric position of the first occurrence of a substring."
sidebar:
  order: 412
---

## strpos()

```php
function strpos(string $haystack, string $needle, int $offset = 0): mixed
```

Finds the numeric position of the first occurrence of a substring.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strpos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strpos.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strpos.md).

