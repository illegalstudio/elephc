---
title: "strrpos()"
description: "Finds the numeric position of the last occurrence of a substring."
sidebar:
  order: 400
---

## strrpos()

```php
function strrpos(string $haystack, string $needle, int $offset = 0): mixed
```

Finds the numeric position of the last occurrence of a substring.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strrpos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strrpos.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strrpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strrpos.md).

