---
title: "inet_pton()"
description: "Packs a textual IPv4 or IPv6 address into its 4- or 16-byte network-order form, or false when the string is not a valid address."
sidebar:
  order: 838
---

## inet_pton()

```php
function inet_pton(string $ip): mixed
```

Packs a textual IPv4 or IPv6 address into its 4- or 16-byte network-order form, or false when the string is not a valid address.

**Parameters**:
- `$ip` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/inet_pton.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/inet_pton.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `inet_pton` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/inet_pton.md).
