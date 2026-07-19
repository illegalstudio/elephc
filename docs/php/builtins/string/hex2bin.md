---
title: "hex2bin()"
description: "Decodes a hexadecimal string back into its binary representation."
sidebar:
  order: 364
---

## hex2bin()

```php
function hex2bin(string $string): string
```

Decodes a hexadecimal string back into its binary representation.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hex2bin.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hex2bin.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hex2bin` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hex2bin.md).

