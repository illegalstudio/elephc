---
title: "urldecode()"
description: "Decodes a URL-encoded string, including '+' as a space."
sidebar:
  order: 409
---

## urldecode()

```php
function urldecode(string $string): string
```

Decodes a URL-encoded string, including '+' as a space.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/urldecode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/urldecode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `urldecode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/urldecode.md).

