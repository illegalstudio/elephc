---
title: "gzcompress()"
description: "Compress a string using the ZLIB data format."
sidebar:
  order: 367
---

## gzcompress()

```php
function gzcompress(string $data, int $level = -1): string
```

Compress a string using the ZLIB data format.

**Parameters**:
- `$data` (`string`)
- `$level` (`int`), default `-1`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/gzcompress.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/gzcompress.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gzcompress` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzcompress.md).
