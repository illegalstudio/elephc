---
title: "rawurldecode()"
description: "Decodes an RFC 3986 percent-encoded string without treating '+' as a space."
sidebar:
  order: 394
---

## rawurldecode()

```php
function rawurldecode(string $string): string
```

Decodes an RFC 3986 percent-encoded string without treating '+' as a space.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/rawurldecode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/rawurldecode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rawurldecode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/rawurldecode.md).

