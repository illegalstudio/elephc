---
title: "gzinflate()"
description: "Inflate a deflated string."
sidebar:
  order: 376
---

## gzinflate()

```php
function gzinflate(string $data, int $max_length = 0): mixed
```

Inflate a deflated string.

**Parameters**:
- `$data` (`string`)
- `$max_length` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/gzinflate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/gzinflate.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gzinflate` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzinflate.md).
