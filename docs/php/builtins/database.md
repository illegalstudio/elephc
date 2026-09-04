---
title: "Database builtins"
description: "Builtins in the Database category."
sidebar:
  order: 119
---

## Database builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`mysqli_affected_rows()`](./database/mysqli_affected_rows.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_autocommit()`](./database/mysqli_autocommit.md) | `(mixed $mysql, bool $enable): bool` | `bool` | ✓ | — |
| [`mysqli_begin_transaction()`](./database/mysqli_begin_transaction.md) | `(mixed $mysql, int $flags = 0, string $name = null): bool` | `bool` | ✓ | — |
| [`mysqli_character_set_name()`](./database/mysqli_character_set_name.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_close()`](./database/mysqli_close.md) | `(mixed $mysql): bool` | `bool` | ✓ | — |
| [`mysqli_commit()`](./database/mysqli_commit.md) | `(mixed $mysql, int $flags = 0, string $name = null): bool` | `bool` | ✓ | — |
| [`mysqli_connect()`](./database/mysqli_connect.md) | `(string $hostname = null, string $username = null, string $password = null, string $database = null, int $port = null, string $socket = null): mixed` | `mixed` | ✓ | — |
| [`mysqli_connect_errno()`](./database/mysqli_connect_errno.md) | `(): int` | `int` | ✓ | — |
| [`mysqli_connect_error()`](./database/mysqli_connect_error.md) | `(): string` | `string` | ✓ | — |
| [`mysqli_data_seek()`](./database/mysqli_data_seek.md) | `(mixed $result, int $offset): bool` | `bool` | ✓ | — |
| [`mysqli_errno()`](./database/mysqli_errno.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_error()`](./database/mysqli_error.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_error_list()`](./database/mysqli_error_list.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | — |
| [`mysqli_escape_string()`](./database/mysqli_escape_string.md) | `(mixed $mysql, string $string): string` | `string` | ✓ | — |
| [`mysqli_execute()`](./database/mysqli_execute.md) | `(mixed $statement): bool` | `bool` | ✓ | — |
| [`mysqli_execute_query()`](./database/mysqli_execute_query.md) | `(mixed $mysql, string $query, mixed $params = null): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_all()`](./database/mysqli_fetch_all.md) | `(mixed $result, int $mode = 2): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_array()`](./database/mysqli_fetch_array.md) | `(mixed $result, int $mode = 3): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_assoc()`](./database/mysqli_fetch_assoc.md) | `(mixed $result): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_column()`](./database/mysqli_fetch_column.md) | `(mixed $result, int $column = 0): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_field()`](./database/mysqli_fetch_field.md) | `(mixed $result): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_field_direct()`](./database/mysqli_fetch_field_direct.md) | `(mixed $result, int $index): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_fields()`](./database/mysqli_fetch_fields.md) | `(mixed $result): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_lengths()`](./database/mysqli_fetch_lengths.md) | `(mixed $result): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_object()`](./database/mysqli_fetch_object.md) | `(mixed $result, string $class = 'stdClass', mixed $constructor_args = []): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_row()`](./database/mysqli_fetch_row.md) | `(mixed $result): mixed` | `mixed` | ✓ | — |
| [`mysqli_field_count()`](./database/mysqli_field_count.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_field_seek()`](./database/mysqli_field_seek.md) | `(mixed $result, int $index): bool` | `bool` | ✓ | — |
| [`mysqli_field_tell()`](./database/mysqli_field_tell.md) | `(mixed $result): int` | `int` | ✓ | — |
| [`mysqli_free_result()`](./database/mysqli_free_result.md) | `(mixed $result): void` | `void` | ✓ | — |
| [`mysqli_get_charset()`](./database/mysqli_get_charset.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | — |
| [`mysqli_get_client_info()`](./database/mysqli_get_client_info.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_get_client_version()`](./database/mysqli_get_client_version.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_get_host_info()`](./database/mysqli_get_host_info.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_get_proto_info()`](./database/mysqli_get_proto_info.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_get_server_info()`](./database/mysqli_get_server_info.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_get_server_version()`](./database/mysqli_get_server_version.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_info()`](./database/mysqli_info.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_init()`](./database/mysqli_init.md) | `(): mixed` | `mixed` | ✓ | — |
| [`mysqli_insert_id()`](./database/mysqli_insert_id.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_more_results()`](./database/mysqli_more_results.md) | `(mixed $mysql): bool` | `bool` | ✓ | — |
| [`mysqli_multi_query()`](./database/mysqli_multi_query.md) | `(mixed $mysql, string $query): bool` | `bool` | ✓ | — |
| [`mysqli_next_result()`](./database/mysqli_next_result.md) | `(mixed $mysql): bool` | `bool` | ✓ | — |
| [`mysqli_num_fields()`](./database/mysqli_num_fields.md) | `(mixed $result): int` | `int` | ✓ | — |
| [`mysqli_num_rows()`](./database/mysqli_num_rows.md) | `(mixed $result): int` | `int` | ✓ | — |
| [`mysqli_options()`](./database/mysqli_options.md) | `(mixed $mysql, int $option, mixed $value): bool` | `bool` | ✓ | — |
| [`mysqli_ping()`](./database/mysqli_ping.md) | `(mixed $mysql): bool` | `bool` | ✓ | — |
| [`mysqli_prepare()`](./database/mysqli_prepare.md) | `(mixed $mysql, string $query): mixed` | `mixed` | ✓ | — |
| [`mysqli_query()`](./database/mysqli_query.md) | `(mixed $mysql, string $query, int $result_mode = 0): mixed` | `mixed` | ✓ | — |
| [`mysqli_real_connect()`](./database/mysqli_real_connect.md) | `(mixed $mysql, string $hostname = null, string $username = null, string $password = null, string $database = null, int $port = null, string $socket = null, int $flags = 0): bool` | `bool` | ✓ | — |
| [`mysqli_real_escape_string()`](./database/mysqli_real_escape_string.md) | `(mixed $mysql, string $string): string` | `string` | ✓ | — |
| [`mysqli_real_query()`](./database/mysqli_real_query.md) | `(mixed $mysql, string $query): bool` | `bool` | ✓ | — |
| [`mysqli_release_savepoint()`](./database/mysqli_release_savepoint.md) | `(mixed $mysql, string $name): bool` | `bool` | ✓ | — |
| [`mysqli_report()`](./database/mysqli_report.md) | `(int $flags): bool` | `bool` | ✓ | — |
| [`mysqli_rollback()`](./database/mysqli_rollback.md) | `(mixed $mysql, int $flags = 0, string $name = null): bool` | `bool` | ✓ | — |
| [`mysqli_savepoint()`](./database/mysqli_savepoint.md) | `(mixed $mysql, string $name): bool` | `bool` | ✓ | — |
| [`mysqli_select_db()`](./database/mysqli_select_db.md) | `(mixed $mysql, string $database): bool` | `bool` | ✓ | — |
| [`mysqli_set_charset()`](./database/mysqli_set_charset.md) | `(mixed $mysql, string $charset): bool` | `bool` | ✓ | — |
| [`mysqli_set_opt()`](./database/mysqli_set_opt.md) | `(mixed $mysql, int $option, mixed $value): bool` | `bool` | ✓ | — |
| [`mysqli_sqlstate()`](./database/mysqli_sqlstate.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_stat()`](./database/mysqli_stat.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | — |
| [`mysqli_stmt_affected_rows()`](./database/mysqli_stmt_affected_rows.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_bind_param()`](./database/mysqli_stmt_bind_param.md) | `(mixed $statement, string $types, ...$vars): bool` | `bool` | ✓ | — |
| [`mysqli_stmt_close()`](./database/mysqli_stmt_close.md) | `(mixed $statement): bool` | `bool` | ✓ | — |
| [`mysqli_stmt_errno()`](./database/mysqli_stmt_errno.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_error()`](./database/mysqli_stmt_error.md) | `(mixed $statement): string` | `string` | ✓ | — |
| [`mysqli_stmt_error_list()`](./database/mysqli_stmt_error_list.md) | `(mixed $statement): mixed` | `mixed` | ✓ | — |
| [`mysqli_stmt_execute()`](./database/mysqli_stmt_execute.md) | `(mixed $statement, mixed $params = null): bool` | `bool` | ✓ | — |
| [`mysqli_stmt_field_count()`](./database/mysqli_stmt_field_count.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_free_result()`](./database/mysqli_stmt_free_result.md) | `(mixed $statement): void` | `void` | ✓ | — |
| [`mysqli_stmt_get_result()`](./database/mysqli_stmt_get_result.md) | `(mixed $statement): mixed` | `mixed` | ✓ | — |
| [`mysqli_stmt_init()`](./database/mysqli_stmt_init.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | — |
| [`mysqli_stmt_insert_id()`](./database/mysqli_stmt_insert_id.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_num_rows()`](./database/mysqli_stmt_num_rows.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_param_count()`](./database/mysqli_stmt_param_count.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_prepare()`](./database/mysqli_stmt_prepare.md) | `(mixed $statement, string $query): bool` | `bool` | ✓ | — |
| [`mysqli_stmt_reset()`](./database/mysqli_stmt_reset.md) | `(mixed $statement): bool` | `bool` | ✓ | — |
| [`mysqli_stmt_sqlstate()`](./database/mysqli_stmt_sqlstate.md) | `(mixed $statement): string` | `string` | ✓ | — |
| [`mysqli_stmt_store_result()`](./database/mysqli_stmt_store_result.md) | `(mixed $statement): bool` | `bool` | ✓ | — |
| [`mysqli_store_result()`](./database/mysqli_store_result.md) | `(mixed $mysql, int $mode = 0): mixed` | `mixed` | ✓ | — |
| [`mysqli_thread_id()`](./database/mysqli_thread_id.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_thread_safe()`](./database/mysqli_thread_safe.md) | `(): bool` | `bool` | ✓ | — |
| [`mysqli_use_result()`](./database/mysqli_use_result.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | — |
| [`mysqli_warning_count()`](./database/mysqli_warning_count.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`pdo_drivers()`](./database/pdo_drivers.md) | `(): mixed` | `mixed` | ✓ | — |
