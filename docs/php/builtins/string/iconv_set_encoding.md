---
title: "iconv_set_encoding()"
description: "Sets the input, output, or internal character encoding."
sidebar:
  order: 483
---

## iconv_set_encoding()

```php
function iconv_set_encoding(string $type, string $encoding): bool
```

Sets the input, output, or internal character encoding.

**Parameters**:
- `$type` (`string`)
- `$encoding` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/iconv_set_encoding.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_set_encoding.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_set_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_set_encoding.md).
