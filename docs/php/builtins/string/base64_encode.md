---
title: "base64_encode()"
description: "Encodes binary data into a Base64 string."
sidebar:
  order: 358
---

## base64_encode()

```php
function base64_encode(string $string): string
```

Encodes binary data into a Base64 string.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/base64_encode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/base64_encode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `base64_encode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/base64_encode.md).

