---
title: "str_split()"
description: "Converts a string into an array of chunks of the given length."
sidebar:
  order: 415
---

## str_split()

```php
function str_split(string $string, int $length = 1): array
```

Converts a string into an array of chunks of the given length.

**Parameters**:
- `$string` (`string`)
- `$length` (`int`), default `1`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_split.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_split.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `str_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_split.md).
