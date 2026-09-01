---
title: "iconv_get_encoding()"
description: "Reports the configured input, output, or internal character encoding."
sidebar:
  order: 424
---

## iconv_get_encoding()

```php
function iconv_get_encoding(string $type = 'all'): mixed
```

Reports the configured input, output, or internal character encoding.

**Parameters**:
- `$type` (`string`), default `'all'`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/iconv_get_encoding.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_get_encoding.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_get_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_get_encoding.md).
