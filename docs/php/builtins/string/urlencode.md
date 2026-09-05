---
title: "urlencode()"
description: "URL-encodes a string using application/x-www-form-urlencoded rules."
sidebar:
  order: 545
---

## urlencode()

```php
function urlencode(string $string): string
```

URL-encodes a string using application/x-www-form-urlencoded rules.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/urlencode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/urlencode.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `urlencode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/urlencode.md).
