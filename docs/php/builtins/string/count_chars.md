---
title: "count_chars()"
description: "Returns byte-frequency information about a string as a tally or a byte list."
sidebar:
  order: 403
---

## count_chars()

```php
function count_chars(string $string, int $mode = 0): array|string
```

Returns byte-frequency information about a string as a tally or a byte list.

**Parameters**:
- `$string` (`string`)
- `$mode` (`int`), default `0`, optional

**Returns**: `array|string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/count_chars.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/count_chars.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `count_chars` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/count_chars.md).
