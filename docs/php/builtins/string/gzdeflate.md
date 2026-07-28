---
title: "gzdeflate()"
description: "Deflate a string using the DEFLATE data format."
sidebar:
  order: 375
---

## gzdeflate()

```php
function gzdeflate(string $data, int $level = -1): string
```

Deflate a string using the DEFLATE data format.

**Parameters**:
- `$data` (`string`)
- `$level` (`int`), default `-1`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/gzdeflate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/gzdeflate.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gzdeflate` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzdeflate.md).
