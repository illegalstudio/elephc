---
title: "iconv_strlen()"
description: "Returns the character count of a string in the requested encoding."
sidebar:
  order: 429
---

## iconv_strlen()

```php
function iconv_strlen(string $string, string $encoding = null): mixed
```

Returns the character count of a string in the requested encoding.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/iconv_strlen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_strlen.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_strlen` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_strlen.md).
