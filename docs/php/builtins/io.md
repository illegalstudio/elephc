---
title: "IO builtins"
description: "Builtins in the IO category."
sidebar:
  order: 109
---

## IO builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`closedir()`](./io/closedir.md) | `(resource $dir_handle): void` | `void` | ✓ | ✓ |
| [`fclose()`](./io/fclose.md) | `(resource $stream): bool` | `bool` | ✓ | ✓ |
| [`fdatasync()`](./io/fdatasync.md) | `(resource $stream): bool` | `bool` | ✓ | ✓ |
| [`feof()`](./io/feof.md) | `(resource $stream): bool` | `bool` | ✓ | ✓ |
| [`fflush()`](./io/fflush.md) | `(resource $stream): bool` | `bool` | ✓ | ✓ |
| [`fgetc()`](./io/fgetc.md) | `(resource $stream): mixed` | `mixed` | ✓ | ✓ |
| [`fgetcsv()`](./io/fgetcsv.md) | `(resource $stream, int $length = null, string $separator = ','): array` | `array` | ✓ | ✓ |
| [`fgets()`](./io/fgets.md) | `(resource $stream): mixed` | `mixed` | ✓ | ✓ |
| [`file()`](./io/file.md) | `(string $filename): array` | `array` | ✓ | ✓ |
| [`file_get_contents()`](./io/file_get_contents.md) | `(string $filename): mixed` | `mixed` | ✓ | ✓ |
| [`file_put_contents()`](./io/file_put_contents.md) | `(string $filename, string $data): int` | `int` | ✓ | ✓ |
| [`flock()`](./io/flock.md) | `(resource $stream, int $operation, bool $would_block = null): bool` | `bool` | ✓ | ✓ |
| [`fopen()`](./io/fopen.md) | `(string $filename, string $mode, bool $use_include_path = false, mixed $context = null): mixed` | `mixed` | ✓ | ✓ |
| [`fpassthru()`](./io/fpassthru.md) | `(resource $stream): int` | `int` | ✓ | ✓ |
| [`fprintf()`](./io/fprintf.md) | `(resource $stream, string $format, ...$values): int` | `int` | ✓ | ✓ |
| [`fputcsv()`](./io/fputcsv.md) | `(resource $stream, array $fields, string $separator = ',', string $enclosure = '"'): int` | `int` | ✓ | ✓ |
| [`fread()`](./io/fread.md) | `(resource $stream, int $length): string` | `string` | ✓ | ✓ |
| [`fscanf()`](./io/fscanf.md) | `(resource $stream, string $format, ...$vars): array` | `array` | ✓ | ✓ |
| [`fseek()`](./io/fseek.md) | `(resource $stream, int $offset, int $whence = 0): int` | `int` | ✓ | ✓ |
| [`fstat()`](./io/fstat.md) | `(resource $stream): mixed` | `mixed` | ✓ | ✓ |
| [`fsync()`](./io/fsync.md) | `(resource $stream): bool` | `bool` | ✓ | ✓ |
| [`ftell()`](./io/ftell.md) | `(resource $stream): int` | `int` | ✓ | ✓ |
| [`ftruncate()`](./io/ftruncate.md) | `(resource $stream, int $size): bool` | `bool` | ✓ | ✓ |
| [`fwrite()`](./io/fwrite.md) | `(resource $stream, string $data): int` | `int` | ✓ | ✓ |
| [`gethostbyaddr()`](./io/gethostbyaddr.md) | `(string $ip): mixed` | `mixed` | ✓ | ✓ |
| [`gethostbyname()`](./io/gethostbyname.md) | `(string $hostname): string` | `string` | ✓ | ✓ |
| [`gethostname()`](./io/gethostname.md) | `(): string` | `string` | ✓ | ✓ |
| [`getprotobyname()`](./io/getprotobyname.md) | `(string $protocol): mixed` | `mixed` | ✓ | ✓ |
| [`getprotobynumber()`](./io/getprotobynumber.md) | `(int $protocol): mixed` | `mixed` | ✓ | ✓ |
| [`getservbyname()`](./io/getservbyname.md) | `(string $service, string $protocol): mixed` | `mixed` | ✓ | ✓ |
| [`getservbyport()`](./io/getservbyport.md) | `(int $port, string $protocol): mixed` | `mixed` | ✓ | ✓ |
| [`hash_file()`](./io/hash_file.md) | `(string $algo, string $filename, bool $binary = false): mixed` | `mixed` | ✓ | ✓ |
| [`ob_clean()`](./io/ob_clean.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`ob_end_clean()`](./io/ob_end_clean.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`ob_end_flush()`](./io/ob_end_flush.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`ob_flush()`](./io/ob_flush.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`ob_get_clean()`](./io/ob_get_clean.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`ob_get_contents()`](./io/ob_get_contents.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`ob_get_flush()`](./io/ob_get_flush.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`ob_get_length()`](./io/ob_get_length.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`ob_get_level()`](./io/ob_get_level.md) | `(): int` | `int` | ✓ | ✓ |
| [`ob_get_status()`](./io/ob_get_status.md) | `(bool $full_status = false): array` | `array` | ✓ | ✓ |
| [`ob_implicit_flush()`](./io/ob_implicit_flush.md) | `(bool $enable = true): bool` | `bool` | ✓ | ✓ |
| [`ob_list_handlers()`](./io/ob_list_handlers.md) | `(): array` | `array` | ✓ | ✓ |
| [`ob_start()`](./io/ob_start.md) | `(mixed $callback = null, int $chunk_size = 0, int $flags = 112): bool` | `bool` | ✓ | ✓ |
| [`opendir()`](./io/opendir.md) | `(string $directory): mixed` | `mixed` | ✓ | ✓ |
| [`readdir()`](./io/readdir.md) | `(resource $dir_handle): mixed` | `mixed` | ✓ | ✓ |
| [`rewind()`](./io/rewind.md) | `(resource $stream): bool` | `bool` | ✓ | ✓ |
| [`rewinddir()`](./io/rewinddir.md) | `(resource $dir_handle): void` | `void` | ✓ | ✓ |
| [`stream_bucket_make_writeable()`](./io/stream_bucket_make_writeable.md) | `(mixed $brigade): mixed` | `mixed` | ✓ | ✓ |
| [`stream_bucket_new()`](./io/stream_bucket_new.md) | `(resource $stream, string $buffer): mixed` | `mixed` | ✓ | ✓ |
| [`stream_context_create()`](./io/stream_context_create.md) | `(array $options = null, array $params = null): mixed` | `mixed` | ✓ | ✓ |
| [`stream_context_get_default()`](./io/stream_context_get_default.md) | `(array $options = null): mixed` | `mixed` | ✓ | ✓ |
| [`stream_context_get_options()`](./io/stream_context_get_options.md) | `(resource $context): array` | `array` | ✓ | ✓ |
| [`stream_context_get_params()`](./io/stream_context_get_params.md) | `(resource $context): array` | `array` | ✓ | ✓ |
| [`stream_context_set_default()`](./io/stream_context_set_default.md) | `(array $options): mixed` | `mixed` | ✓ | ✓ |
| [`stream_context_set_option()`](./io/stream_context_set_option.md) | `(resource $context, string $wrapper_or_options, string $option_name = null, mixed $value = null): bool` | `bool` | ✓ | ✓ |
| [`stream_context_set_params()`](./io/stream_context_set_params.md) | `(resource $context, array $params): bool` | `bool` | ✓ | ✓ |
| [`stream_copy_to_stream()`](./io/stream_copy_to_stream.md) | `(resource $from, resource $to, int $length = null, int $offset = -1): mixed` | `mixed` | ✓ | ✓ |
| [`stream_filter_register()`](./io/stream_filter_register.md) | `(string $filter_name, string $class): bool` | `bool` | ✓ | ✓ |
| [`stream_filter_remove()`](./io/stream_filter_remove.md) | `(resource $stream_filter): bool` | `bool` | ✓ | ✓ |
| [`stream_get_contents()`](./io/stream_get_contents.md) | `(resource $stream, int $length = null, int $offset = -1): mixed` | `mixed` | ✓ | ✓ |
| [`stream_get_filters()`](./io/stream_get_filters.md) | `(): array` | `array` | ✓ | ✓ |
| [`stream_get_line()`](./io/stream_get_line.md) | `(resource $stream, int $length, string $ending = ''): string` | `string` | ✓ | ✓ |
| [`stream_get_meta_data()`](./io/stream_get_meta_data.md) | `(resource $stream): array` | `array` | ✓ | ✓ |
| [`stream_get_transports()`](./io/stream_get_transports.md) | `(): array` | `array` | ✓ | ✓ |
| [`stream_get_wrappers()`](./io/stream_get_wrappers.md) | `(): array` | `array` | ✓ | ✓ |
| [`stream_is_local()`](./io/stream_is_local.md) | `(resource $stream): bool` | `bool` | ✓ | ✓ |
| [`stream_isatty()`](./io/stream_isatty.md) | `(resource $stream): bool` | `bool` | ✓ | ✓ |
| [`stream_resolve_include_path()`](./io/stream_resolve_include_path.md) | `(string $filename): mixed` | `mixed` | ✓ | ✓ |
| [`stream_select()`](./io/stream_select.md) | `(array $read, array $write, array $except, int $seconds, int $microseconds = 0): int` | `int` | ✓ | ✓ |
| [`stream_set_blocking()`](./io/stream_set_blocking.md) | `(resource $stream, bool $enable): bool` | `bool` | ✓ | ✓ |
| [`stream_set_chunk_size()`](./io/stream_set_chunk_size.md) | `(resource $stream, int $size): int` | `int` | ✓ | ✓ |
| [`stream_set_read_buffer()`](./io/stream_set_read_buffer.md) | `(resource $stream, int $size): int` | `int` | ✓ | ✓ |
| [`stream_set_timeout()`](./io/stream_set_timeout.md) | `(resource $stream, int $seconds, int $microseconds = 0): bool` | `bool` | ✓ | ✓ |
| [`stream_set_write_buffer()`](./io/stream_set_write_buffer.md) | `(resource $stream, int $size): int` | `int` | ✓ | ✓ |
| [`stream_socket_accept()`](./io/stream_socket_accept.md) | `(resource $socket, float $timeout = null, string $peer_name = null): mixed` | `mixed` | ✓ | ✓ |
| [`stream_socket_client()`](./io/stream_socket_client.md) | `(string $address): mixed` | `mixed` | ✓ | ✓ |
| [`stream_socket_enable_crypto()`](./io/stream_socket_enable_crypto.md) | `(resource $stream, bool $enable, int $crypto_method = null, resource $session_stream = null): bool` | `bool` | ✓ | ✓ |
| [`stream_socket_get_name()`](./io/stream_socket_get_name.md) | `(resource $socket, bool $remote): mixed` | `mixed` | ✓ | ✓ |
| [`stream_socket_pair()`](./io/stream_socket_pair.md) | `(int $domain, int $type, int $protocol): mixed` | `mixed` | ✓ | ✓ |
| [`stream_socket_recvfrom()`](./io/stream_socket_recvfrom.md) | `(resource $socket, int $length, int $flags = 0, string $address = ''): mixed` | `mixed` | ✓ | ✓ |
| [`stream_socket_sendto()`](./io/stream_socket_sendto.md) | `(resource $socket, string $data, int $flags = 0, string $address = ''): mixed` | `mixed` | ✓ | ✓ |
| [`stream_socket_server()`](./io/stream_socket_server.md) | `(string $address): mixed` | `mixed` | ✓ | ✓ |
| [`stream_socket_shutdown()`](./io/stream_socket_shutdown.md) | `(resource $stream, int $mode): bool` | `bool` | ✓ | ✓ |
| [`stream_supports_lock()`](./io/stream_supports_lock.md) | `(resource $stream): bool` | `bool` | ✓ | ✓ |
| [`stream_wrapper_register()`](./io/stream_wrapper_register.md) | `(string $protocol, string $class, int $flags = 0): bool` | `bool` | ✓ | ✓ |
| [`stream_wrapper_restore()`](./io/stream_wrapper_restore.md) | `(string $protocol): bool` | `bool` | ✓ | ✓ |
| [`stream_wrapper_unregister()`](./io/stream_wrapper_unregister.md) | `(string $protocol): bool` | `bool` | ✓ | ✓ |
| [`vfprintf()`](./io/vfprintf.md) | `(resource $stream, string $format, array $values): int` | `int` | ✓ | ✓ |
