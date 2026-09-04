---
title: "gzuncompress()"
description: "Uncompress a compressed string."
sidebar:
  order: 464
---

## gzuncompress()

```php
function gzuncompress(string $data, int $max_length = 0): mixed
```

Uncompress a compressed string.

**Parameters**:
- `$data` (`string`)
- `$max_length` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/gzuncompress.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/gzuncompress.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzuncompress` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzuncompress.md).
