---
title: "crc32()"
description: "Calculates the CRC32 polynomial of a string."
sidebar:
  order: 364
---

## crc32()

```php
function crc32(string $string): int
```

Calculates the CRC32 polynomial of a string.

**Parameters**:
- `$string` (`string`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/crc32.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/crc32.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `crc32` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/crc32.md).
