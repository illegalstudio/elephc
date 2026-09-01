---
title: "base64_decode()"
description: "Decodes a Base64-encoded string back into its original data."
sidebar:
  order: 397
---

## base64_decode()

```php
function base64_decode(string $string, bool $strict = false): mixed
```

Decodes a Base64-encoded string back into its original data.

**Parameters**:
- `$string` (`string`)
- `$strict` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/base64_decode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/base64_decode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `base64_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/base64_decode.md).
