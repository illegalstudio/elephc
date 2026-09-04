---
title: "chunk_split()"
description: "Splits a string into fixed-length chunks separated by a given string."
sidebar:
  order: 455
---

## chunk_split()

```php
function chunk_split(string $string, int $length = 76, string $separator = '\r\n'): string
```

Splits a string into fixed-length chunks separated by a given string.

**Parameters**:
- `$string` (`string`)
- `$length` (`int`), default `76`, optional
- `$separator` (`string`), default `'\r\n'`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/chunk_split.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/chunk_split.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `chunk_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/chunk_split.md).
