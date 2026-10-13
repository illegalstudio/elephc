---
title: "inet_ntop()"
description: "Renders a packed 4-byte IPv4 or 16-byte IPv6 address as its presentation string, or false for any other length."
sidebar:
  order: 823
---

## inet_ntop()

```php
function inet_ntop(string $ip): mixed
```

Renders a packed 4-byte IPv4 or 16-byte IPv6 address as its presentation string, or false for any other length.

**Parameters**:
- `$ip` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/inet_ntop.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/inet_ntop.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `inet_ntop` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/inet_ntop.md).
