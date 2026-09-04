---
title: "iconv()"
description: "Converts a string from one character encoding to another."
sidebar:
  order: 478
---

## iconv()

```php
function iconv(string $from_encoding, string $to_encoding, string $string): mixed
```

Converts a string from one character encoding to another.

**Parameters**:
- `$from_encoding` (`string`)
- `$to_encoding` (`string`)
- `$string` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/iconv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv.md).
