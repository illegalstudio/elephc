---
title: "ip2long()"
description: "Converts a string containing an IPv4 address into a long integer."
sidebar:
  order: 393
---

## ip2long()

```php
function ip2long(string $ip): mixed
```

Converts a string containing an IPv4 address into a long integer.

**Parameters**:
- `$ip` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/ip2long.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/ip2long.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ip2long` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ip2long.md).
