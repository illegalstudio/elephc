---
title: "long2ip()"
description: "Converts an IPv4 address from long integer to dotted string notation."
sidebar:
  order: 388
---

## long2ip()

```php
function long2ip(int $ip): string
```

Converts an IPv4 address from long integer to dotted string notation.

**Parameters**:
- `$ip` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/long2ip.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/long2ip.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `long2ip` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/long2ip.md).
