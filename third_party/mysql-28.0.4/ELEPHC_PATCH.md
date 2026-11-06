# elephc patch of `mysql` 28.0.4

This is the crates.io `mysql` 28.0.4 source, wired in through `[patch.crates-io]` in the
workspace `Cargo.toml`, with these additions in `src/conn/mod.rs` (marked `elephc patch`):

- `Conn::status_flags()` — the server status flags of the last OK/EOF packet, which the
  upstream crate tracks but does not expose (mysqli's `MYSQLI_REPORT_INDEX`).
- `Conn::refresh(options)` — `COM_REFRESH` (mysqli's `mysqli::refresh()`).
- `Conn::dump_debug_info()` — `COM_DEBUG` (mysqli's `mysqli::dump_debug_info()`).
- `Conn::process_kill(id, read_reply)` — `COM_PROCESS_KILL` (mysqli's `mysqli::kill()`;
  the reply is skipped when a connection kills itself, like mysqlnd).
- `Conn::set_server_option(option)` — `COM_SET_OPTION` (mysqli's per-`multi_query()`
  multi-statement toggle), and in `src/conn/opts/mod.rs` the `multi_statements` option
  (`OptsBuilder::multi_statements`, default `true`) that lets a mysqli handshake omit
  `CLIENT_MULTI_STATEMENTS`, and `mariadb_extensions` (`OptsBuilder::mariadb_extensions`,
  default `true`) that lets it skip MariaDB's extended capabilities like mysqlnd.
- `Conn::exec_iter_with_cursor(stmt, params, cursor_type)`: `COM_STMT_EXECUTE` with the
  cursor-type flags byte (the upstream request builder always sends
  `CURSOR_TYPE_NO_CURSOR`), whose metadata EOF is parsed for `SERVER_STATUS_CURSOR_EXISTS`,
  and `Conn::stmt_fetch(stmt_id, num_rows, columns)`: `COM_STMT_FETCH` (mysqli's
  `MYSQLI_CURSOR_TYPE_READ_ONLY` statements).
- `Conn::stmt_reset(stmt_id)`: `COM_STMT_RESET` with its OK packet (mysqli's
  `mysqli_stmt::reset()`).
- `Conn::stmt_send_long_data(stmt_id, param_index, data)`: one `COM_STMT_SEND_LONG_DATA`
  chunk sent immediately (mysqli's `mysqli_stmt::send_long_data()`); the connection remembers
  the parameters per statement (`ConnInner::long_data_params`, cleared by execution, reset and
  close), and the next `COM_STMT_EXECUTE` (`write_execute_with_long_data`) types them
  `MYSQL_TYPE_LONG_BLOB`, not null, without a value, like mysqlnd.
- mysqlnd-exact wire statistics: `IO_STAT_NAMES`, `IoCounters`, the per-command response
  state machine that picks the mysqlnd packet reader of each received packet, and the
  `CountedStream` wrapper that counts raw received bytes.
- `IoCounters`, `GLOBAL_IO_COUNTERS`, and `Conn::io_counters()` — byte and packet counters
  updated in `raw_read_packet` / `write_packet`, the two points every packet passes
  (mysqli's `get_connection_stats()` / `mysqli_get_client_stats()`).
- `QueryResult::is_errored()`: whether the current set of a multi-statement batch is the
  server's error for its header, so a streaming consumer reports it before announcing the set
  (mysqlnd's `next_result()` fails with that error).

Everything else is byte-for-byte upstream (MIT / Apache-2.0, see the license files). To
upgrade, copy the new release over this directory and re-apply the additions.
