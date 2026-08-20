---
title: "quoted_printable_encode()"
description: "Encodes a string with the MIME quoted-printable transfer encoding."
sidebar:
  order: 445
---

## quoted_printable_encode()

```php
function quoted_printable_encode(string $string): string
```

Encodes a string with the MIME quoted-printable transfer encoding.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/quoted_printable_encode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/quoted_printable_encode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `quoted_printable_encode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/quoted_printable_encode.md).
