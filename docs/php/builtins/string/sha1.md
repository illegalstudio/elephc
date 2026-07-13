---
title: "sha1()"
description: "Calculates the SHA-1 hash of a string."
sidebar:
  order: 383
---

## sha1()

```php
function sha1(string $string, bool $binary = false): string
```

Calculates the SHA-1 hash of a string.

**Parameters**:
- `$string` (`string`)
- `$binary` (`bool`), default `false`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/sha1.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/sha1.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sha1` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sha1.md).

