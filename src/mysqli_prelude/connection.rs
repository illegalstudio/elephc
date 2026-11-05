//! Purpose:
//! The `mysqli` connection class, as an elephc-PHP source fragment (the
//! parse-parity oracle for `build::connection`): DSN/TLS construction over
//! `elephc_pdo_open_persistent`, connect-time and per-op error bookkeeping,
//! `mysqli_report` dispatch, escaping, charset, transactions, change_user,
//! kill/refresh, MYSQLI_ASYNC queries with `poll()` / `reap_async_query()`,
//! warnings, and the mysqlnd-style statistics.
//!
//! Called from:
//! - `crate::mysqli_prelude::fragments::source_for_version` (concatenated into
//!   the injected prelude).
//!
//! Key details:
//! - `$conn = -1` means "not connected" (`mysqli_init()` / argument-less
//!   `new mysqli()`); a successful `real_connect` stores the bridge handle. The
//!   DSN prefix is always forced to `mysql:` and a successful open whose
//!   `elephc_pdo_driver_name` is not "mysql" is rejected (belt-and-braces).
//! - Failure dispatch follows `mysqli::$reportMode`: STRICT throws
//!   `mysqli_sql_exception`, ERROR writes to STDERR and the caller returns
//!   `false`, OFF is silent. `PDOException` is never thrown.
//! - TLS (`MYSQLI_CLIENT_SSL`, `ssl_set()`) rides the bridge's rustls backend
//!   through the packed `my_ssl_config`; peer verification follows mysqlnd.
//! - The bridge is synchronous, so a `MYSQLI_ASYNC` query runs when it is sent
//!   and its outcome waits for `reap_async_query()`; `poll()` reports such
//!   links as immediately readable.
//! - Private `__elephc*` helpers are reached from `mysqli_stmt`,
//!   `mysqli_result`, and the procedural aliases through the checker's mysqli
//!   friend channel; they are not part of PHP's surface.

/// The `mysqli` class and its connection-level behavior (fragment without a
/// `<?php` header).
pub(super) const SRC: &str = r##"
// -- mysqli connection surface --

class mysqli {
    // Opaque elephc_pdo bridge connection handle; -1 = not connected. Not part
    // of PHP's surface: private, handed to mysqli_stmt through the friend channel.
    private int $conn = -1;

    // Process-wide mysqli_report() mode. The literal default is rewritten at
    // injection time for --php-version 8.0 (OFF); see fragments.rs.
    public static int $reportMode = 3;

    // Process-wide last-connect failure, read by the no-argument procedural
    // mysqli_connect_errno() / mysqli_connect_error() exactly like PHP's
    // globals; updated by every construct / real_connect attempt.
    public static int $lastConnectErrno = 0;
    public static string $lastConnectError = "";

    // Process-wide mysqlnd-style client statistics (mysqli_get_client_stats())
    // and link counters (mysqli_get_links_stats()).
    public static array $clientStats = [];
    public static int $openLinks = 0;
    public static int $openPersistentLinks = 0;
    // Persistent (`p:`) links parked in the pool, the bridge handles the pool
    // has handed out (a handle id is never recycled, so seeing one again means
    // a pool hit), and each pooled handle's mysqlnd connection statistics, which
    // belong to the handle and survive its trips through the pool.
    public static int $cachedPlinks = 0;
    public static array $seenPlinks = [];
    public static array $plinkStats = [];

    // Public properties refreshed after operations (writes stick; documented
    // divergence — no write barriers).
    public int|string $affected_rows = 0;
    public string $client_info = "";
    public int $client_version = 0;
    public int $connect_errno = 0;
    public ?string $connect_error = null;
    public int $errno = 0;
    public string $error = "";
    public array $error_list = [];
    public int $field_count = 0;
    public string $host_info = "";
    public ?string $info = null;
    public int|string $insert_id = 0;
    public string $server_info = "";
    public int $server_version = 0;
    public string $sqlstate = "00000";
    public int $protocol_version = 10;
    public int $thread_id = 0;
    public int $warning_count = 0;

    // mysqli_options() / ssl_set() values collected before real_connect applies them.
    private int $optConnectTimeout = 0;
    private array $optInitCommands = [];
    private string $optCharsetName = "";
    private int $optLocalInfile = 0;
    private string $optLocalInfileDir = "";
    private string $optServerPublicKey = "";
    private int $optSslVerify = -1;
    private bool $optIntAndFloatNative = false;
    private int $optReadTimeout = 0;
    private bool $sslRequested = false;
    private string $sslKey = "";
    private string $sslCert = "";
    private string $sslCa = "";
    private string $sslCapath = "";
    private string $sslCipher = "";
    private bool $persistent = false;
    // php-src's link status: 0 initialized (mysqli_init / argument-less
    // constructor, or a real_connect that failed on such a link), 1 connected
    // (also a link whose reconnect failed: it stays connected to nothing and
    // answers 2014), 2 closed (close(), or a constructor that failed).
    private int $linkStatus = 0;
    // The mysqlnd handle behind the link: whether one is allocated (a failed
    // real_connect keeps it, a failed constructor frees it), and whether
    // mysqlnd allocated it persistent — mysqli_init() always does (so the link
    // could still connect to a `p:` host), and a `p:` host does.
    private bool $handleAllocated = false;
    private bool $handlePersistent = false;
    // Whether the link killed its own connection (kill($thread_id)): mysqlnd
    // considers it gone at once, and every later command fails with 2006.
    private bool $linkGone = false;
    // The live bridge statement of real_query() / multi_query() /
    // query(MYSQLI_USE_RESULT), kept until every result set is consumed; -1
    // when the link is idle. Its CURRENT set is either unclaimed
    // ($liveSetPending: waiting for store_result() / use_result()) or being
    // streamed by a MYSQLI_USE_RESULT result ($streaming). Rows are read from
    // the server on demand, so any other command meanwhile is mysqlnd's 2014.
    private int $liveStmt = -1;
    private int $liveStep = 0;
    private bool $liveSetPending = false;
    private bool $streaming = false;
    // Whether $liveStmt is already positioned on a further result set.
    private bool $multiMore = false;
    // MYSQLI_ASYNC: whether a query was sent and runs on the bridge's worker
    // thread (the link is busy until reap_async_query() collects it), and
    // whether its result is wanted unbuffered (MYSQLI_USE_RESULT).
    private bool $asyncPending = false;
    // The mysqli_stmt handle whose result set is still on the server, or -1.
    private int $busyStatement = -1;
    // Whether this link turned multi-statements on (mysqlnd's mysql->multi_query).
    private bool $multiStatementsOn = false;
    private bool $asyncUnbuffered = false;
    // Per-connection mysqlnd-style statistics (mysqli::get_connection_stats()).
    private array $connStats = [];

    public function __construct(
        ?string $hostname = null,
        ?string $username = null,
        ?string $password = null,
        ?string $database = null,
        ?int $port = null,
        ?string $socket = null
    ) {
        // The argument-less constructor is mysqli_init(): no connection attempt
        // until real_connect() (php-src behavior).
        if ($hostname === null && $username === null && $password === null
            && $database === null && $port === null && $socket === null) {
            $this->handleAllocated = true;
            $this->handlePersistent = true;
            return;
        }
        $this->connectLink($hostname, $username, $password, $database, $port, $socket, 0, false);
    }

    public function real_connect(
        ?string $hostname = null,
        ?string $username = null,
        ?string $password = null,
        ?string $database = null,
        ?int $port = null,
        ?string $socket = null,
        int $flags = 0
    ): bool {
        return $this->connectLink($hostname, $username, $password, $database, $port, $socket, $flags, true);
    }

    // mysqli_common_connect: the constructor / connect() ($isRealConnect false)
    // and real_connect() (true) differ only in what a failure leaves behind.
    // Drops the argument-less constructor's mysqlnd handle without accounting
    // for it: the internal placeholder links never had one in php, and
    // mysqli_connect() allocates its own (persistent only for a `p:` host).
    private function __elephcDetachHandle(): void {
        $this->handleAllocated = false;
        $this->handlePersistent = false;
    }

    // mysqli_connect(): the constructor's connect path on a detached link.
    private function __elephcConnectProcedural(?string $hostname, ?string $username, ?string $password, ?string $database, ?int $port, ?string $socket): bool {
        return $this->connectLink($hostname, $username, $password, $database, $port, $socket, 0, false);
    }

    // mysqlnd's upsert state after a protocol command that is not a statement
    // (COM_PING, COM_INIT_DB, COM_REFRESH, COM_DEBUG, COM_PROCESS_KILL,
    // COM_STATISTICS): affected_rows reads -1, and an OK reply clears the
    // previous statement's info string (any other reply leaves it).
    private function adminReply(bool $okPacket): void {
        $this->affected_rows = -1;
        if ($okPacket) {
            $this->info = null;
        }
    }

    // The calls php allows on an unconnected link but not on a closed one.
    private function requireNotClosed(): void {
        if ($this->linkStatus == 2) {
            throw new Error("mysqli object is already closed");
        }
    }

    private function connectLink(
        ?string $hostname,
        ?string $username,
        ?string $password,
        ?string $database,
        ?int $port,
        ?string $socket,
        int $flags,
        bool $isRealConnect
    ): bool {
        if ($this->linkStatus == 2) {
            if ($isRealConnect) {
                throw new Error("mysqli object is already closed");
            }
            // connect() on a closed object starts it over on a new link.
            $this->linkStatus = 0;
            $this->resetOptions();
            $this->connStats = [];
        }
        $_host = $hostname === null ? "localhost" : $hostname;
        $_persistent = 0;
        // php-src compares the `p:` persistent-connection prefix
        // case-insensitively, so `P:host` is persistent too; the real host is
        // the remainder.
        if (strlen($_host) >= 2 && strtolower(substr($_host, 0, 2)) === "p:") {
            $_persistent = 1;
            $_host = substr($_host, 2);
        }
        if ($this->linkStatus == 1) {
            // Connecting a link that is already connected: php-src closes it
            // implicitly and starts over on a fresh mysqlnd handle, so every
            // option and the connection statistics are back to their defaults.
            $this->closeLink(false);
            $this->resetOptions();
            $this->connStats = [];
        }
        if (!$this->handleAllocated) {
            $this->handleAllocated = true;
            $this->handlePersistent = $_persistent == 1;
        }
        $_port = $port === null ? 3306 : $port;
        $_socket = $socket === null ? "" : $socket;
        // SECURITY: host, dbname, and unix_socket are folded verbatim into the
        // bridge DSN, which splits on ';' and applies the LAST duplicate of a
        // directive; unlike user/password these three are NOT percent-decoded
        // by the bridge, so a ';' in any of them would inject a second
        // directive (e.g. host="localhost;host=attacker" redirects the
        // connection). None of the three ever legitimately contains a ';', so
        // reject it rather than silently trusting the crafted value.
        if (strpos($_host, ";") !== false || ($database !== null && strpos($database, ";") !== false) || strpos($_socket, ";") !== false) {
            return $this->connectFailure(2002, "elephc mysqli: host, database, and socket must not contain ';'", "HY000");
        }
        // php-src mysqli honors the socket only when the host is empty or
        // exactly "localhost"; any other host goes over TCP.
        $_dsn = "mysql:";
        if ($_socket !== "" && ($_host === "" || $_host === "localhost")) {
            $_dsn = $_dsn . "unix_socket=" . $_socket;
        } else {
            $_dsn = $_dsn . "host=" . ($_host === "" ? "localhost" : $_host) . ";port=" . $_port;
        }
        if ($database !== null && $database !== "") {
            $_dsn = $_dsn . ";dbname=" . $database;
        }
        if ($username !== null) {
            // '%' first, so the '%' introduced by encoding ';' is not itself
            // re-encoded; the bridge percent-decodes user=/password= DSN values,
            // so a ';' or '%' inside a credential survives the DSN's split on
            // ';'. An explicitly-passed empty credential is still transmitted.
            $_dsn = $_dsn . ";user=" . str_replace(";", "%3B", str_replace("%", "%25", $username));
        }
        if ($password !== null) {
            $_dsn = $_dsn . ";password=" . str_replace(";", "%3B", str_replace("%", "%25", $password));
        }
        if ($this->optConnectTimeout > 0) {
            $_dsn = $_dsn . ";connect_timeout=" . $this->optConnectTimeout;
        }
        // TLS: MYSQLI_CLIENT_SSL or any ssl_set() value enables it. Peer
        // verification follows mysqlnd: an explicit VERIFY / DONT_VERIFY client
        // flag wins, then MYSQLI_OPT_SSL_VERIFY_SERVER_CERT, then the default —
        // verify when ssl_set() named any key/cert/CA/cipher, otherwise not.
        $_sslConfig = "";
        $_sslFiles = $this->sslKey !== "" || $this->sslCert !== "" || $this->sslCa !== ""
            || $this->sslCapath !== "" || $this->sslCipher !== "";
        if (($flags & 2048) != 0 || ($flags & 1073741824) != 0 || $this->sslRequested) {
            // The packed TLS config is split on ';' bridge-side without decoding,
            // so a path containing ';' could smuggle a second key; reject it.
            if (strpos($this->sslKey . $this->sslCert . $this->sslCa, ";") !== false) {
                return $this->connectFailure(2026, "elephc mysqli: ssl_set() paths must not contain ';'", "HY000");
            }
            $_verify = $_sslFiles ? 1 : 0;
            if ($this->optSslVerify >= 0) {
                $_verify = $this->optSslVerify;
            }
            if (($flags & 1073741824) != 0) {
                $_verify = 1;
            }
            if (($flags & 64) != 0) {
                $_verify = 0;
            }
            if ($this->sslCa !== "") {
                $_sslConfig = $_sslConfig . "ca=" . $this->sslCa . ";";
            }
            if ($this->sslCert !== "") {
                $_sslConfig = $_sslConfig . "cert=" . $this->sslCert . ";";
            }
            if ($this->sslKey !== "") {
                $_sslConfig = $_sslConfig . "key=" . $this->sslKey . ";";
            }
            $_sslConfig = $_sslConfig . "verify=" . $_verify . ";";
        }
        // Client flags and options map onto the bridge's packed driver config
        // (the format PDO packs for $my_driver_config); values are
        // percent-encoded because the bridge decodes them after splitting on ';'.
        $_driverConfig = "";
        // Like a mysqlnd link, the session starts without multi-statements —
        // php-src masks CLIENT_MULTI_STATEMENTS out of $flags ("don't allow
        // multi_queries via connect parameter"); multi_query() turns them on with
        // COM_SET_OPTION, and query() / real_query() turn them off again.
        $_driverConfig = $_driverConfig . "servermulti=0;";
        // mysqlnd negotiates none of MariaDB's extended capabilities either.
        $_driverConfig = $_driverConfig . "mariadbext=0;";
        // Every prepare() reaches the server and close() sends COM_STMT_CLOSE.
        $_driverConfig = $_driverConfig . "stmtcache=0;";
        // Streaming result sets stop at set boundaries until next_result().
        $_driverConfig = $_driverConfig . "lazysets=1;";
        if (($flags & 32) != 0) {
            $_driverConfig = $_driverConfig . "compress=1;";
        }
        if (($flags & 256) != 0) {
            $_driverConfig = $_driverConfig . "ignore=1;";
        }
        if ($this->optLocalInfile != 0) {
            $_driverConfig = $_driverConfig . "local=1;";
        }
        if ($this->optLocalInfileDir !== "") {
            $_driverConfig = $_driverConfig . "dir=" . $this->encodeDriverOption($this->optLocalInfileDir) . ";";
        }
        if ($this->sslCapath !== "" && $_sslConfig !== "") {
            $_driverConfig = $_driverConfig . "capath=" . $this->encodeDriverOption($this->sslCapath) . ";";
        }
        if ($this->sslCipher !== "" && $_sslConfig !== "") {
            $_driverConfig = $_driverConfig . "cipher=" . $this->encodeDriverOption($this->sslCipher) . ";";
        }
        if ($this->optReadTimeout > 0) {
            // MYSQLI_OPT_READ_TIMEOUT: the socket read (and write) timeout.
            $_driverConfig = $_driverConfig . "iotimeout=" . $this->optReadTimeout . ";";
        }
        if ($this->optServerPublicKey !== "") {
            $_driverConfig = $_driverConfig . "serverkey=" . $this->encodeDriverOption($this->optServerPublicKey) . ";";
        }
        $_foundRows = (($flags & 2) != 0) ? 1 : 0;
        // The first MYSQLI_INIT_COMMAND rides the bridge's own init hook; any
        // further ones (mysqlnd keeps a list) run right after authentication.
        $_initCommand = "";
        if (count($this->optInitCommands) > 0) {
            $_initCommand = (string) $this->optInitCommands[0];
        }
        $_conn = elephc_pdo_open_persistent($_dsn, $_persistent, 0, $_initCommand, $_sslConfig, $_foundRows, "", $_driverConfig);
        if ($_conn < 0) {
            $_raw = elephc_pdo_last_open_error();
            $_message = mysqli::__elephcServerMessage($_raw);
            $_state = elephc_pdo_last_open_sqlstate();
            if ($_state === "") {
                // mysqlnd records a connect/authentication failure with the
                // generic SQLSTATE: "(HY000/1045): Access denied …".
                $_state = "HY000";
            }
            $_code = elephc_pdo_last_open_native_code();
            if ($_code == 0) {
                // The server's ERR packet code, kept in the bridge's
                // "MySqlError { ERROR 1045 (28000): … }" rendering.
                $_marker = strpos($_raw, "ERROR ");
                if ($_marker !== false) {
                    $_code = (int) substr($_raw, $_marker + 6, 5);
                }
            }
            if ($_code == 0) {
                // CR_CONN_HOST_ERROR: generic client-side connect failure.
                $_code = 2002;
            }
            $this->bump("connect_failure");
            if (!$isRealConnect) {
                // A failed constructor frees the handle: the object is closed.
                $this->freeHandle();
                $this->linkStatus = 2;
            }
            return $this->connectFailure($_code, $_message, $_state);
        }
        // Cannot happen while the DSN prefix is forced to mysql:, but keep the
        // guard: a non-mysql handle must never become a mysqli connection.
        if (elephc_pdo_driver_name($_conn) !== "mysql") {
            elephc_pdo_release($_conn, 0);
            return $this->connectFailure(2002, "elephc mysqli opened a non-mysql bridge connection", "HY000");
        }
        $this->conn = $_conn;
        $this->linkStatus = 1;
        $this->linkGone = false;
        $this->multiStatementsOn = false;
        $this->persistent = $_persistent == 1;
        $this->connect_errno = 0;
        $this->connect_error = null;
        mysqli::$lastConnectErrno = 0;
        mysqli::$lastConnectError = "";
        mysqli::$openLinks = mysqli::$openLinks + 1;
        $_pooled = false;
        if ($this->persistent) {
            mysqli::$openPersistentLinks = mysqli::$openPersistentLinks + 1;
            $_pooled = in_array($_conn, mysqli::$seenPlinks, true);
            if ($_pooled) {
                // A pool hit hands back the cached handle and its statistics;
                // mysqlnd records no new connection.
                mysqli::$cachedPlinks = mysqli::$cachedPlinks - 1;
                $this->connStats = (array) (mysqli::$plinkStats[$_conn] ?? []);
            } else {
                $_seen = mysqli::$seenPlinks;
                $_seen[] = $_conn;
                mysqli::$seenPlinks = $_seen;
            }
        }
        if (!$_pooled) {
            if ($this->handlePersistent) {
                $this->bump("pconnect_success");
                $this->bump("active_persistent_connections");
            }
            $this->bump("connect_success");
            $this->bump("active_connections");
        }
        $this->clearError();
        // Connection information, refreshed once per connect.
        $this->host_info = elephc_pdo_connection_status($_conn);
        $this->server_info = elephc_pdo_server_version($_conn);
        $this->server_version = $this->versionStringToInt($this->server_info);
        $this->client_info = elephc_pdo_mysql_client_info();
        $this->client_version = $this->versionStringToInt($this->client_info);
        // Both come from the handshake with ZERO round-trips, like php-src: the
        // server thread id straight from the bridge, and the live charset is
        // tracked bridge-side so character_set_name() never issues a statement.
        $this->thread_id = elephc_pdo_mysql_thread_id($_conn);
        $this->warning_count = elephc_pdo_warning_count($_conn);
        $this->info = null;
        $_n = count($this->optInitCommands);
        for ($_i = 1; $_i < $_n; $_i++) {
            if (elephc_pdo_exec($_conn, (string) $this->optInitCommands[$_i]) < 0) {
                $this->opFailed();
                mysqli::$openLinks = mysqli::$openLinks - 1;
                $this->dropActive();
                $this->releaseConnection();
                if (!$isRealConnect) {
                    $this->freeHandle();
                    $this->linkStatus = 2;
                }
                return false;
            }
        }
        if ($this->optCharsetName !== "") {
            // MYSQLI_SET_CHARSET_NAME collected before connect applies now.
            $this->set_charset($this->optCharsetName);
        }
        return true;
    }

    public function connect(
        ?string $hostname = null,
        ?string $username = null,
        ?string $password = null,
        ?string $database = null,
        ?int $port = null,
        ?string $socket = null
    ): bool {
        // Alias of the constructor's connect path (php-src keeps it for
        // backwards compatibility with the old procedural object).
        return $this->connectLink($hostname, $username, $password, $database, $port, $socket, 0, false);
    }

    // php 8.1 deprecated: re-initializes an unconnected object; a no-op on a
    // connected one (mysqlnd keeps the live handle).
    public function init() {
        return null;
    }

    public function close(): bool {
        // php 8: closing a closed object is an Error; a never-connected one
        // just frees its handle.
        if ($this->linkStatus == 2) {
            throw new Error("mysqli object is already closed");
        }
        $this->closeLink(true);
        $this->linkStatus = 2;
        return true;
    }

    public function __destruct() {
        // php-src frees a link through the same close path, recorded as an
        // explicit close.
        if ($this->linkStatus != 2) {
            $this->closeLink(true);
        }
    }

    // php_mysqli_close: a pooled persistent link goes back to the pool without
    // touching mysqlnd's statistics; any other link quits a live connection
    // (explicit_close / implicit_close, com_quit, one active_connections) and
    // frees its mysqlnd handle (another active_connections — mysqlnd decrements
    // it twice for a connection it closes).
    private function closeLink(bool $explicit): void {
        if ($this->linkStatus == 1) {
            mysqli::$openLinks = mysqli::$openLinks - 1;
        }
        if ($this->conn >= 0 && $this->persistent) {
            $_stats = mysqli::$plinkStats;
            $_stats[$this->conn] = $this->connStats;
            mysqli::$plinkStats = $_stats;
            mysqli::$cachedPlinks = mysqli::$cachedPlinks + 1;
            $this->releaseConnection();
            $this->handleAllocated = false;
            return;
        }
        if ($this->conn >= 0) {
            $this->bump($explicit ? "explicit_close" : "implicit_close");
            $this->bump("com_quit");
            $this->dropActive();
            $this->releaseConnection();
        }
        $this->freeHandle();
    }

    // Frees the link's mysqlnd handle (mysqlnd's dtor drops one
    // active_connections — and one active_persistent_connections for a
    // persistent handle — whether or not it ever connected).
    private function freeHandle(): void {
        if (!$this->handleAllocated) {
            return;
        }
        $this->handleAllocated = false;
        $_client = mysqli::$clientStats;
        $_client["active_connections"] = ((int) ($_client["active_connections"] ?? 0)) - 1;
        if ($this->handlePersistent) {
            $_client["active_persistent_connections"] = ((int) ($_client["active_persistent_connections"] ?? 0)) - 1;
        }
        mysqli::$clientStats = $_client;
    }

    // A fresh mysqlnd handle starts from the default options: every
    // mysqli_options() / ssl_set() value set on the previous one is gone.
    private function resetOptions(): void {
        $this->optConnectTimeout = 0;
        $this->optInitCommands = [];
        $this->optCharsetName = "";
        $this->optLocalInfile = 0;
        $this->optLocalInfileDir = "";
        $this->optServerPublicKey = "";
        $this->optSslVerify = -1;
        $this->optIntAndFloatNative = false;
        $this->optReadTimeout = 0;
        $this->sslRequested = false;
        $this->sslKey = "";
        $this->sslCert = "";
        $this->sslCa = "";
        $this->sslCapath = "";
        $this->sslCipher = "";
    }

    public function ping(): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        $this->bump("com_ping");
        // A real COM_PING (no statement, no result set, no status change).
        if (elephc_pdo_mysql_ping($this->conn) < 0) {
            $this->adminReply(false);
            return $this->opFailed();
        }
        $this->clearError();
        $this->adminReply(true);
        return true;
    }

    public function change_user(string $username, string $password, ?string $database): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        $this->bump("com_change_user");
        // A real COM_CHANGE_USER: the server re-authenticates this same
        // connection and resets its session (open transaction rolled back,
        // temporary tables dropped, session variables and charset reset).
        $_db = $database === null ? "" : $database;
        $_hasDb = $database === null ? 0 : 1;
        if (elephc_pdo_mysql_change_user($this->conn, $username, $password, $_db, $_hasDb) < 0) {
            return $this->opFailed();
        }
        $this->closeLive();
        $this->thread_id = elephc_pdo_mysql_thread_id($this->conn);
        $this->affected_rows = 0;
        $this->insert_id = 0;
        $this->field_count = 0;
        $this->warning_count = 0;
        $this->info = null;
        $this->clearError();
        return true;
    }

    public function kill(int $process_id): bool {
        if ($process_id <= 0) {
            throw new ValueError("mysqli::kill(): Argument #1 (\$process_id) must be greater than 0");
        }
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        $this->bump("com_process_kill");
        // A real COM_PROCESS_KILL, like mysqlnd (MySQL 8.4 removed the command
        // and answers 1047 "Unknown command"). Killing our own connection does
        // not wait for the reply: the server drops the link.
        $_own = $process_id == $this->thread_id;
        if (elephc_pdo_mysql_process_kill($this->conn, $process_id) < 0) {
            $this->adminReply(false);
            return $this->opFailed();
        }
        $this->clearError();
        // Killing our own connection reads no reply; the link is gone.
        $this->adminReply(!$_own);
        if ($_own) {
            $this->linkGone = true;
        }
        return true;
    }

    public function refresh(int $flags): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        $this->bump("com_refresh");
        // A real COM_REFRESH with the REFRESH_* bitmask. Like php, a failure is
        // recorded (errno / error) and returns false without mysqli_report.
        if (elephc_pdo_mysql_refresh($this->conn, $flags) < 0) {
            $this->__elephcAdoptBridgeError();
            $this->adminReply(false);
            return false;
        }
        $this->clearError();
        $this->adminReply(true);
        return true;
    }

    public function ssl_set(?string $key, ?string $certificate, ?string $ca_certificate, ?string $ca_path, ?string $cipher_algos): bool {
        $this->requireNotClosed();
        // Collected for the next real_connect(), which then negotiates TLS
        // (mysqlnd enables SSL as soon as ssl_set() ran).
        $this->sslRequested = true;
        $this->sslKey = $key === null ? "" : $key;
        $this->sslCert = $certificate === null ? "" : $certificate;
        $this->sslCa = $ca_certificate === null ? "" : $ca_certificate;
        $this->sslCapath = $ca_path === null ? "" : $ca_path;
        $this->sslCipher = $cipher_algos === null ? "" : $cipher_algos;
        return true;
    }

    public function debug(string $options): bool {
        // mysqlnd debug tracing is compiled out of release builds
        // (MYSQLI_DEBUG_TRACE_ENABLED is 0), where php ignores the trace
        // options and just returns true.
        $_ignoredOptions = $options;
        return true;
    }

    public function dump_debug_info(): bool {
        // A real COM_DEBUG: the server writes debug information to its own error
        // log (it needs the SUPER privilege). mysqlnd reports success whatever the
        // server answers: a refusal is only recorded in errno / error, with no
        // mysqli_report dispatch.
        $this->requireInitialized();
        $this->bump("com_debug");
        if (elephc_pdo_mysql_dump_debug_info($this->conn) < 0) {
            $this->__elephcAdoptBridgeError();
            $this->adminReply(false);
            return true;
        }
        $this->clearError();
        $this->adminReply(true);
        return true;
    }

    public function get_connection_stats(): array {
        $this->requireInitialized();
        // Values are numeric strings, like php's mysqlnd statistics.
        $_out = [];
        foreach (mysqli::__elephcStatKeys() as $_key) {
            $_name = (string) $_key;
            // A closed link (handle -1) keeps its recorded counters only.
            $_out[$_name] = mysqli::__elephcWireStat($this->conn < 0 ? -2 : $this->conn, $_name)
                ?? mysqli::__elephcStatString((int) ($this->connStats[$_name] ?? 0));
        }
        return $_out;
    }

    // mysqlnd keeps its statistics as uint64_t: a counter driven below zero
    // (active_connections is decremented twice per close) wraps around.
    private static function __elephcStatString(int $value): string {
        if ($value >= 0) {
            return (string) $value;
        }
        $_magnitude = -$value;
        $_low = 709551616 - ($_magnitude % 1000000000);
        $_high = 18446744073 - intdiv($_magnitude, 1000000000);
        if ($_low < 0) {
            $_low = $_low + 1000000000;
            $_high = $_high - 1;
        }
        return (string) $_high . str_pad((string) $_low, 9, "0", STR_PAD_LEFT);
    }

    // mysqlnd's wire-level statistics, which the patched bridge client counts
    // exactly like mysqlnd (per-send and per-reader accounting, mysqlnd's own
    // shifted per-packet-type tables included): one connection's (handle >= 0)
    // or the process-wide totals (handle -1); null for every other statistic.
    private static function __elephcWireStat(int $handle, string $key): ?string {
        if ($handle < -1) {
            return null;
        }
        $_index = [
            "bytes_sent" => 0,
            "bytes_received" => 1,
            "packets_sent" => 2,
            "packets_received" => 3,
            "protocol_overhead_in" => 4,
            "protocol_overhead_out" => 5,
            "packets_sent_command" => 6,
            "bytes_received_ok_packet" => 7,
            "bytes_received_eof_packet" => 8,
            "bytes_received_rset_header_packet" => 9,
            "bytes_received_rset_field_meta_packet" => 10,
            "bytes_received_rset_row_packet" => 11,
            "bytes_received_prepare_response_packet" => 12,
            "bytes_received_change_user_packet" => 13,
            "packets_received_ok" => 14,
            "packets_received_eof" => 15,
            "packets_received_rset_header" => 16,
            "packets_received_rset_field_meta" => 17,
            "packets_received_rset_row" => 18,
            "packets_received_prepare_response" => 19,
            "packets_received_change_user" => 20,
        ][$key] ?? -1;
        if ($_index < 0) {
            return null;
        }
        return (string) elephc_pdo_mysql_io_stat($handle, $_index);
    }

    public function get_warnings(): mysqli_warning|false {
        $this->requireInitialized();
        if ($this->warning_count == 0) {
            return false;
        }
        return $this->__elephcFetchWarnings();
    }

    public function thread_safe(): bool {
        return false;
    }

    public function select_db(string $database): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        $this->bump("com_init_db");
        // A real COM_INIT_DB, like mysqlnd.
        if (elephc_pdo_mysql_init_db($this->conn, $database, strlen($database)) < 0) {
            $this->adminReply(false);
            return $this->opFailed();
        }
        $this->clearError();
        $this->adminReply(true);
        return true;
    }

    public function set_charset(string $charset): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        // Same [A-Za-z0-9_] identifier filter the PDO DSN charset key uses: a
        // charset name is an identifier, never quoted, so anything else is
        // rejected before it can smuggle SQL.
        if (!$this->charsetIdentIsValid($charset)) {
            $this->syntheticFailure(2019, "Invalid characterset or character set not supported", "HY000");
            return false;
        }
        if (elephc_pdo_exec($this->conn, "SET NAMES " . $charset) < 0) {
            return $this->opFailed();
        }
        $this->clearError();
        // The statement's OK packet: no rows affected, no info string.
        $this->affected_rows = 0;
        $this->info = null;
        return true;
    }

    public function character_set_name(): string {
        // php 8: an Error on an unconnected object (not "").
        $this->requireInitialized();
        // The connection's live charset, tracked by the bridge from the
        // handshake + every SET NAMES (no round-trip).
        return elephc_pdo_mysql_charset($this->conn);
    }

    public function real_escape_string(string $string): string {
        // php 8: real_escape_string on an unconnected object is an Error.
        $this->requireInitialized();
        // SECURITY: charset-aware escaping through the bridge, which uses the
        // connection's OWN tracked charset (never assuming utf8mb4) and its
        // NO_BACKSLASH_ESCAPES state. The length-counted read preserves NUL bytes.
        $_len = elephc_pdo_real_escape_string($this->conn, $string, strlen($string));
        if ($_len <= 0) {
            return "";
        }
        return __elephc_ptr_read_string(elephc_pdo_blob_data_ptr(), $_len);
    }

    public function escape_string(string $string): string {
        return $this->real_escape_string($string);
    }

    // -- elephc PHP >= 8.6 mysqli quote_string begin --
    public function quote_string(string $string): string {
        // PHP 8.6: the escaped payload wrapped in single quotes, ready to splice
        // into SQL as a string literal.
        return "'" . $this->real_escape_string($string) . "'";
    }
    // -- elephc PHP >= 8.6 mysqli quote_string end --

    public function begin_transaction(int $flags = 0, ?string $name = null): bool {
        // php-src: $name is a SQL COMMENT on START TRANSACTION, never a
        // savepoint; the ValueErrors are raised BEFORE any SQL goes out.
        if ($flags < 0) {
            throw new ValueError("mysqli::begin_transaction(): Argument #1 (\$flags) must be one of the MYSQLI_TRANS_* constants");
        }
        $_comment = $this->transactionComment("mysqli::begin_transaction", $name, true);
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        // php composes the whole statement once (captured wire:
        // "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ WRITE"); the bridge's
        // note_transaction_sql updates in_transaction from raw START TRANSACTION
        // too, so close()/__destruct still auto-rollback.
        $_parts = [];
        if (($flags & 1) != 0) {
            $_parts[] = "WITH CONSISTENT SNAPSHOT";
        }
        if (($flags & 2) != 0) {
            $_parts[] = "READ WRITE";
        } elseif (($flags & 4) != 0) {
            $_parts[] = "READ ONLY";
        }
        $_sql = "START TRANSACTION";
        if (count($_parts) > 0) {
            $_sql = $_sql . " " . implode(", ", $_parts);
        }
        $_sql = $_sql . $_comment;
        if (elephc_pdo_exec($this->conn, $_sql) < 0) {
            return $this->opFailed();
        }
        $this->refreshStatus();
        $this->clearError();
        return true;
    }

    public function commit(int $flags = 0, ?string $name = null): bool {
        $_comment = $this->transactionComment("mysqli::commit", $name, false);
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        // php: COMMIT [AND [NO] CHAIN] [[NO] RELEASE] /*name*/.
        $_sql = "COMMIT" . $this->transactionCorFlags($flags) . $_comment;
        if (elephc_pdo_exec($this->conn, $_sql) < 0) {
            return $this->opFailed();
        }
        $this->refreshStatus();
        $this->clearError();
        return true;
    }

    public function rollback(int $flags = 0, ?string $name = null): bool {
        $_comment = $this->transactionComment("mysqli::rollback", $name, false);
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        $_sql = "ROLLBACK" . $this->transactionCorFlags($flags) . $_comment;
        if (elephc_pdo_exec($this->conn, $_sql) < 0) {
            return $this->opFailed();
        }
        $this->refreshStatus();
        $this->clearError();
        return true;
    }

    public function savepoint(string $name): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        if ($name === "") {
            throw new ValueError("mysqli::savepoint(): Argument #1 (\$name) must not be empty");
        }
        if (elephc_pdo_exec($this->conn, "SAVEPOINT `" . str_replace("`", "``", $name) . "`") < 0) {
            return $this->opFailed();
        }
        $this->refreshStatus();
        $this->clearError();
        return true;
    }

    public function release_savepoint(string $name): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        if ($name === "") {
            throw new ValueError("mysqli::release_savepoint(): Argument #1 (\$name) must not be empty");
        }
        if (elephc_pdo_exec($this->conn, "RELEASE SAVEPOINT `" . str_replace("`", "``", $name) . "`") < 0) {
            return $this->opFailed();
        }
        $this->refreshStatus();
        $this->clearError();
        return true;
    }

    public function autocommit(bool $enable): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        if (elephc_pdo_set_autocommit($this->conn, $enable ? 1 : 0) != 1) {
            return $this->opFailed();
        }
        $this->clearError();
        $this->affected_rows = 0;
        $this->info = null;
        return true;
    }

    public function options(int $option, mixed $value): bool {
        $this->requireNotClosed();
        // mysqlnd's option ids. Connect-time options are collected and applied
        // by the next real_connect(); unknown ids return false like php-src.
        if ($option == 0) {
            // MYSQLI_OPT_CONNECT_TIMEOUT -> DSN connect_timeout at connect.
            $this->optConnectTimeout = (int) $value;
            return true;
        }
        if ($option == 3) {
            // MYSQLI_INIT_COMMAND: mysqlnd keeps a LIST; every command runs
            // after authentication, in order.
            $this->optInitCommands[] = (string) $value;
            return true;
        }
        if ($option == 7) {
            // MYSQLI_SET_CHARSET_NAME -> SET NAMES after connect.
            $this->optCharsetName = (string) $value;
            return true;
        }
        if ($option == 8) {
            // MYSQLI_OPT_LOCAL_INFILE -> LOAD DATA LOCAL INFILE allowed.
            $this->optLocalInfile = ((int) $value) != 0 ? 1 : 0;
            return true;
        }
        if ($option == 43) {
            // MYSQLI_OPT_LOAD_DATA_LOCAL_DIR -> LOCAL INFILE restricted to a dir.
            $this->optLocalInfileDir = $value === null ? "" : (string) $value;
            return true;
        }
        if ($option == 21) {
            // MYSQLI_OPT_SSL_VERIFY_SERVER_CERT.
            $this->optSslVerify = ((int) $value) != 0 ? 1 : 0;
            return true;
        }
        if ($option == 35) {
            // MYSQLI_SERVER_PUBLIC_KEY: RSA key file for sha256/caching_sha2 auth.
            $this->optServerPublicKey = (string) $value;
            return true;
        }
        if ($option == 201) {
            // MYSQLI_OPT_INT_AND_FLOAT_NATIVE: text-protocol integer and float
            // columns come back as PHP int / float instead of strings.
            $this->optIntAndFloatNative = ((int) $value) != 0;
            return true;
        }
        if ($option == 11) {
            // MYSQLI_OPT_READ_TIMEOUT: socket read/write timeout in seconds,
            // applied at the next real_connect().
            $this->optReadTimeout = (int) $value;
            return true;
        }
        if ($option == 1 || $option == 4 || $option == 5 || $option == 37
            || $option == 202 || $option == 203) {
            // MYSQLI_OPT_COMPRESS, READ_DEFAULT_FILE/GROUP (mysqlnd ignores
            // option files), CAN_HANDLE_EXPIRED_PASSWORDS, and the mysqlnd
            // network buffer sizes are accepted without effect, like mysqlnd.
            return true;
        }
        return false;
    }

    public function set_opt(int $option, mixed $value): bool {
        return $this->options($option, $value);
    }

    public function query(string $query, int $result_mode = 0): mysqli_result|bool {
        if ($query === "") {
            throw new ValueError("mysqli::query(): Argument #1 (\$query) must not be empty");
        }
        // php-src: MYSQLI_USE_RESULT or MYSQLI_STORE_RESULT (optionally with the
        // deprecated MYSQLI_STORE_RESULT_COPY_DATA), plus the MYSQLI_ASYNC bit.
        $_base = ($result_mode & 8) != 0 ? $result_mode - 8 : $result_mode;
        $_storeBase = ($_base & 16) != 0 ? $_base - 16 : $_base;
        if ($_base != 1 && $_storeBase != 0) {
            throw new ValueError("mysqli::query(): Argument #2 (\$result_mode) must be either MYSQLI_USE_RESULT or MYSQLI_STORE_RESULT with MYSQLI_ASYNC as an optional bitmask flag");
        }
        if (($result_mode & 8) != 0) {
            return $this->sendAsyncQuery($query, $_base == 1);
        }
        // MYSQLI_USE_RESULT streams the rows from the server on demand; the
        // default MYSQLI_STORE_RESULT buffers them all before returning.
        $_unbuffered = $_base == 1;
        $_code = $this->runQuery($query, $_unbuffered);
        if ($_code == 0) {
            return false;
        }
        if ($_code == 1) {
            $this->__elephcReportIndex($query);
            return true;
        }
        $_result = $_unbuffered ? $this->claimStream() : $this->claimStored();
        if ($_result === null) {
            return false;
        }
        $this->__elephcReportIndex($query);
        return $_result;
    }

    public function reap_async_query(): mysqli_result|bool {
        $this->requireInitialized();
        if (!$this->asyncPending) {
            return false;
        }
        $this->asyncPending = false;
        // Blocks until the worker finished, then walks the buffered outcome; a
        // failed query is reported here, exactly where mysqlnd surfaces it.
        $_stmt = elephc_pdo_mysql_async_reap($this->conn);
        if ($_stmt < 0) {
            return $this->opFailed();
        }
        // An unbuffered (MYSQLI_USE_RESULT) set keeps its rows on the server: like
        // mysqlnd, the reap reads only the header and metadata, and the first row
        // is read by the first fetch (2 = not read yet).
        $_rc = 2;
        if (!$this->asyncUnbuffered) {
            $_rc = elephc_pdo_step($_stmt);
            if ($_rc < 0) {
                elephc_pdo_finalize($_stmt);
                return $this->opFailed();
            }
        }
        $_columnCount = elephc_pdo_column_count($_stmt);
        $this->__elephcHeaderStats($_columnCount, true);
        if ($_columnCount == 0) {
            $this->refreshStatus();
            $this->field_count = 0;
            elephc_pdo_finalize($_stmt);
            $this->clearError();
            return true;
        }
        $this->liveStmt = $_stmt;
        $this->liveStep = $_rc;
        $this->liveSetPending = true;
        $this->field_count = $_columnCount;
        $this->clearError();
        $_result = $this->asyncUnbuffered ? $this->claimStream() : $this->claimStored();
        if ($_result === null) {
            return false;
        }
        return $_result;
    }

    // mysqli::poll(): waits up to $seconds + $microseconds for MYSQLI_ASYNC
    // queries to finish. On return $read holds the links whose query can be
    // reaped, $reject the links that have no async query in flight, and $error
    // is empty (a failed query is reported by reap_async_query()). php's exact
    // signature: $read / $error may be null, but not both. Returns the number
    // of readable links.
    public static function poll(?array &$read, ?array &$error, array &$reject, int $seconds, int $microseconds = 0): int|false {
        if ($seconds < 0) {
            throw new ValueError("mysqli::poll(): Argument #4 (\$seconds) must be greater than or equal to 0");
        }
        if ($microseconds < 0) {
            throw new ValueError("mysqli::poll(): Argument #5 (\$microseconds) must be greater than or equal to 0");
        }
        if ($read === null && $error === null) {
            throw new ValueError("No stream arrays were passed");
        }
        $_pending = [];
        $_rejected = [];
        if ($read !== null) {
            foreach ($read as $_link) {
                if (!($_link instanceof mysqli)) {
                    throw new TypeError("mysqli::poll(): Argument #1 (\$read) must only contain objects of type mysqli");
                }
                if ($_link->__elephcAsyncPending()) {
                    $_pending[] = $_link;
                } else {
                    $_rejected[] = $_link;
                }
            }
        }
        // $error links only take part in mysqlnd's select() (for exceptional
        // conditions, which a MySQL socket never raises), so they are validated
        // like $read's (php names Argument #1 for both) and keep the call waiting
        // for its timeout, never readying or rejecting anything themselves.
        $_watched = 0;
        if ($error !== null) {
            foreach ($error as $_link) {
                if (!($_link instanceof mysqli)) {
                    throw new TypeError("mysqli::poll(): Argument #1 (\$read) must only contain objects of type mysqli");
                }
                $_watched = $_watched + 1;
            }
        }
        // Wait in short slices over every pending link until one is ready or
        // the timeout elapses (the bridge waits without holding any lock).
        $_ready = [];
        $_budget = $seconds * 1000000 + $microseconds;
        $_deadline = microtime(true) + $_budget / 1000000;
        $_count = count($_pending);
        while (true) {
            foreach ($_pending as $_link) {
                if ($_link->__elephcAsyncReady(0)) {
                    $_ready[] = $_link;
                }
            }
            if (count($_ready) > 0 || ($_count == 0 && $_watched == 0)) {
                break;
            }
            $_left = (int) (($_deadline - microtime(true)) * 1000000);
            if ($_left <= 0) {
                break;
            }
            if ($_count == 0) {
                usleep($_left);
                continue;
            }
            $_slice = $_left < 2000 ? $_left : 2000;
            foreach ($_pending as $_link) {
                $_link->__elephcAsyncReady((int) ($_slice / $_count));
            }
        }
        if ($read !== null) {
            $read = $_ready;
        }
        if ($error !== null) {
            $error = [];
        }
        $reject = $_rejected;
        return count($_ready);
    }

    public function prepare(string $query): mysqli_stmt|false {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        $_statement = new mysqli_stmt($this);
        if (!$_statement->__elephcPrepareOrCopyError($query)) {
            return false;
        }
        return $_statement;
    }

    public function stmt_init(): mysqli_stmt|false {
        // The two-step form: an unprepared statement to be prepared with
        // $stmt->prepare($sql) (or mysqli_stmt_prepare()).
        $this->requireInitialized();
        return new mysqli_stmt($this);
    }

    public function get_charset(): mixed {
        // php describes the connection charset from mysqlnd's compiled-in table:
        // the first entry for the session's charset name (its default collation,
        // the one the bridge client negotiates), with state 1 and an empty dir.
        $this->requireInitialized();
        $_charset = strtolower(elephc_pdo_mysql_charset($this->conn));
        $_entry = mysqli::__elephcCharsetEntry($_charset);
        $_info = new stdClass();
        $_info->{"charset"} = $_charset;
        $_info->{"collation"} = (string) $_entry[0];
        $_info->{"dir"} = "";
        $_info->{"min_length"} = (int) $_entry[2];
        $_info->{"max_length"} = (int) $_entry[3];
        $_info->{"number"} = (int) $_entry[1];
        $_info->{"state"} = 1;
        $_info->{"comment"} = (string) $_entry[4];
        return $_info;
    }

    // mysqlnd's charset table (ext/mysqlnd/mysqlnd_charset.c), reduced to the
    // first entry of each charset name, the one mysqlnd_find_charset_name()
    // returns: [collation, number, min_length, max_length, comment].
    private static function __elephcCharsetEntry(string $name): array {
        $_table = [
            "big5" => ["big5_chinese_ci", 1, 1, 2, ""],
            "dec8" => ["dec8_swedish_ci", 3, 1, 1, ""],
            "cp850" => ["cp850_general_ci", 4, 1, 1, ""],
            "hp8" => ["hp8_english_ci", 6, 1, 1, ""],
            "koi8r" => ["koi8r_general_ci", 7, 1, 1, ""],
            "latin1" => ["latin1_swedish_ci", 8, 1, 1, ""],
            "latin2" => ["latin2_general_ci", 9, 1, 1, ""],
            "swe7" => ["swe7_swedish_ci", 10, 1, 1, ""],
            "ascii" => ["ascii_general_ci", 11, 1, 1, ""],
            "ujis" => ["ujis_japanese_ci", 12, 1, 3, ""],
            "sjis" => ["sjis_japanese_ci", 13, 1, 2, ""],
            "hebrew" => ["hebrew_general_ci", 16, 1, 1, ""],
            "filename" => ["filename", 17, 1, 5, ""],
            "tis620" => ["tis620_thai_ci", 18, 1, 1, ""],
            "euckr" => ["euckr_korean_ci", 19, 1, 2, ""],
            "koi8u" => ["koi8u_general_ci", 22, 1, 1, ""],
            "gb2312" => ["gb2312_chinese_ci", 24, 1, 2, ""],
            "greek" => ["greek_general_ci", 25, 1, 1, ""],
            "cp1250" => ["cp1250_general_ci", 26, 1, 1, ""],
            "gbk" => ["gbk_chinese_ci", 28, 1, 2, ""],
            "latin5" => ["latin5_turkish_ci", 30, 1, 1, ""],
            "armscii8" => ["armscii8_general_ci", 32, 1, 1, ""],
            "utf8" => ["utf8_general_ci", 33, 1, 3, "UTF-8 Unicode"],
            "ucs2" => ["ucs2_general_ci", 35, 2, 2, "UCS-2 Unicode"],
            "cp866" => ["cp866_general_ci", 36, 1, 1, ""],
            "keybcs2" => ["keybcs2_general_ci", 37, 1, 1, ""],
            "macce" => ["macce_general_ci", 38, 1, 1, ""],
            "macroman" => ["macroman_general_ci", 39, 1, 1, ""],
            "cp852" => ["cp852_general_ci", 40, 1, 1, ""],
            "latin7" => ["latin7_general_ci", 41, 1, 1, ""],
            "utf8mb4" => ["utf8mb4_general_ci", 45, 1, 4, "UTF-8 Unicode"],
            "cp1251" => ["cp1251_general_ci", 51, 1, 1, ""],
            "utf16" => ["utf16_general_ci", 54, 2, 4, "UTF-16 Unicode"],
            "utf16le" => ["utf16le_general_ci", 56, 2, 4, "UTF-16LE Unicode"],
            "cp1256" => ["cp1256_general_ci", 57, 1, 1, ""],
            "cp1257" => ["cp1257_general_ci", 59, 1, 1, ""],
            "utf32" => ["utf32_general_ci", 60, 4, 4, "UTF-32 Unicode"],
            "binary" => ["binary", 63, 1, 1, ""],
            "geostd8" => ["geostd8_general_ci", 92, 1, 1, ""],
            "cp932" => ["cp932_japanese_ci", 95, 1, 2, ""],
            "eucjpms" => ["eucjpms_japanese_ci", 97, 1, 3, ""],
            "gb18030" => ["gb18030_chinese_ci", 248, 1, 4, ""],
        ];
        return $_table[$name] ?? ["", 0, 0, 0, ""];
    }

    // -- elephc PHP >= 8.2 mysqli execute_query begin --
    public function execute_query(string $query, ?array $params = null): mysqli_result|bool {
        // prepare + execute($params) + get_result in one call (PHP 8.2+).
        $_statement = $this->prepare($query);
        if ($_statement === false) {
            return false;
        }
        if (!$_statement->execute($params)) {
            $_statement->close();
            return false;
        }
        if ($_statement->field_count == 0) {
            // Non-select: mirror the statement's outcome on the connection and
            // report success as `true`, like mysqli::query does.
            $this->affected_rows = $_statement->affected_rows;
            $this->insert_id = $_statement->insert_id;
            $this->field_count = 0;
            $_statement->close();
            return true;
        }
        $_result = $_statement->get_result();
        $_statement->close();
        if ($_result === false) {
            return false;
        }
        $this->field_count = $_result->field_count;
        return $_result;
    }
    // -- elephc PHP >= 8.2 mysqli execute_query end --

    public function real_query(string $query): bool {
        // A produced result set stays on the server until store_result() buffers
        // it or use_result() streams it (mysqlnd's two-step protocol).
        $_code = $this->runQuery($query, true);
        if ($_code == 1) {
            $this->__elephcReportIndex($query);
        }
        return $_code != 0;
    }

    public function multi_query(string $query): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        // One server round-trip for the whole batch, after mysqlnd's
        // COM_SET_OPTION(MULTI_STATEMENTS_ON); its result sets are walked with
        // store_result() / use_result() / next_result().
        $this->enableMultiStatements();
        $this->bump("com_query");
        $_stmt = $this->prepareText($query, true);
        if ($_stmt < 0) {
            $this->opFailed();
            $this->disableMultiStatements();
            return false;
        }
        // The first set's header and metadata only (2 = no row read yet).
        $_rc = elephc_pdo_mysql_execute($_stmt);
        if ($_rc == 0) {
            $_rc = 2;
        }
        if ($_rc < 0) {
            $this->opFailed();
            elephc_pdo_finalize($_stmt);
            return false;
        }
        $this->liveStmt = $_stmt;
        $this->enterSet($_rc);
        return true;
    }

    public function more_results(): bool {
        $this->requireInitialized();
        return $this->multiMore;
    }

    public function next_result(): bool {
        $this->requireInitialized();
        // Like mysqlnd, the current set must have been consumed first (an
        // unclaimed or still-streaming set leaves the link out of sync).
        if ($this->liveSetPending || $this->streaming) {
            return false;
        }
        if (!$this->multiMore || $this->liveStmt < 0) {
            $this->multiMore = false;
            return false;
        }
        $this->multiMore = false;
        // Read the next set's header and metadata (its rows stay unread).
        $_advanced = elephc_pdo_next_rowset($this->liveStmt);
        if ($_advanced < 0) {
            $this->opFailed();
            $this->closeLive();
            return false;
        }
        if ($_advanced == 0) {
            $this->closeLive();
            return false;
        }
        $this->enterSet(2);
        return true;
    }

    public function store_result(int $mode = 0): mysqli_result|false {
        $this->requireInitialized();
        // $mode (MYSQLI_STORE_RESULT_COPY_DATA) has been ignored since PHP 8.1.
        $_ignoredMode = $mode;
        $_result = $this->claimStored();
        if ($_result === null) {
            return false;
        }
        $this->__elephcReportIndex("from previous query");
        return $_result;
    }

    public function use_result(): mysqli_result|false {
        $this->requireInitialized();
        // The rows stay on the server and are read one fetch at a time.
        $_result = $this->claimStream();
        if ($_result === null) {
            return false;
        }
        return $_result;
    }

    public function get_server_info(): string {
        $this->requireInitialized();
        return $this->server_info;
    }

    public function get_client_info(): string {
        return elephc_pdo_mysql_client_info();
    }

    public function stat(): string|false {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        $this->bump("com_statistics");
        // The bridge's server_info is MySQL's own "Uptime: … Questions: …"
        // statistics line — exactly what mysqli::stat() returns.
        $_stat = elephc_pdo_server_info($this->conn);
        $this->adminReply(false);
        if ($_stat === "") {
            return false;
        }
        return $_stat;
    }

    // -- internal helpers ($_-prefixed locals; same checker rule as PDO) --

    // Every mysqlnd statistics key php reports, in php's order. Shared by
    // get_connection_stats() and the procedural mysqli_get_client_stats().
    private static function __elephcStatKeys(): array {
        return explode(" ", "bytes_sent bytes_received packets_sent packets_received protocol_overhead_in protocol_overhead_out bytes_received_ok_packet bytes_received_eof_packet bytes_received_rset_header_packet bytes_received_rset_field_meta_packet bytes_received_rset_row_packet bytes_received_prepare_response_packet bytes_received_change_user_packet packets_sent_command packets_received_ok packets_received_eof packets_received_rset_header packets_received_rset_field_meta packets_received_rset_row packets_received_prepare_response packets_received_change_user result_set_queries non_result_set_queries no_index_used bad_index_used slow_queries buffered_sets unbuffered_sets ps_buffered_sets ps_unbuffered_sets flushed_normal_sets flushed_ps_sets ps_prepared_never_executed ps_prepared_once_executed rows_fetched_from_server_normal rows_fetched_from_server_ps rows_buffered_from_client_normal rows_buffered_from_client_ps rows_fetched_from_client_normal_buffered rows_fetched_from_client_normal_unbuffered rows_fetched_from_client_ps_buffered rows_fetched_from_client_ps_unbuffered rows_fetched_from_client_ps_cursor rows_affected_normal rows_affected_ps rows_skipped_normal rows_skipped_ps copy_on_write_saved copy_on_write_performed command_buffer_too_small connect_success connect_failure connection_reused reconnect pconnect_success active_connections active_persistent_connections explicit_close implicit_close disconnect_close in_middle_of_command_close explicit_free_result implicit_free_result explicit_stmt_close implicit_stmt_close mem_emalloc_count mem_emalloc_amount mem_ecalloc_count mem_ecalloc_amount mem_erealloc_count mem_erealloc_amount mem_efree_count mem_efree_amount mem_malloc_count mem_malloc_amount mem_calloc_count mem_calloc_amount mem_realloc_count mem_realloc_amount mem_free_count mem_free_amount mem_estrndup_count mem_strndup_count mem_estrdup_count mem_strdup_count mem_edupl_count mem_dupl_count proto_text_fetched_null proto_text_fetched_bit proto_text_fetched_tinyint proto_text_fetched_short proto_text_fetched_int24 proto_text_fetched_int proto_text_fetched_bigint proto_text_fetched_decimal proto_text_fetched_float proto_text_fetched_double proto_text_fetched_date proto_text_fetched_year proto_text_fetched_time proto_text_fetched_datetime proto_text_fetched_timestamp proto_text_fetched_string proto_text_fetched_blob proto_text_fetched_enum proto_text_fetched_set proto_text_fetched_geometry proto_text_fetched_other proto_binary_fetched_null proto_binary_fetched_bit proto_binary_fetched_tinyint proto_binary_fetched_short proto_binary_fetched_int24 proto_binary_fetched_int proto_binary_fetched_bigint proto_binary_fetched_decimal proto_binary_fetched_float proto_binary_fetched_double proto_binary_fetched_date proto_binary_fetched_year proto_binary_fetched_time proto_binary_fetched_datetime proto_binary_fetched_timestamp proto_binary_fetched_string proto_binary_fetched_json proto_binary_fetched_blob proto_binary_fetched_enum proto_binary_fetched_set proto_binary_fetched_geometry proto_binary_fetched_other init_command_executed_count init_command_failed_count com_quit com_init_db com_query com_field_list com_create_db com_drop_db com_refresh com_shutdown com_statistics com_process_info com_connect com_process_kill com_debug com_ping com_time com_delayed_insert com_change_user com_binlog_dump com_table_dump com_connect_out com_register_slave com_stmt_prepare com_stmt_execute com_stmt_send_long_data com_stmt_close com_stmt_reset com_stmt_set_option com_stmt_fetch com_deamon bytes_received_real_data_normal bytes_received_real_data_ps");
    }

    // The errno php reports for a bridge failure: a transport error (which the
    // bridge numbers 1) is mysqlnd's CR_SERVER_GONE_ERROR 2006.
    private static function __elephcClientErrno(int $errno, string $message): int {
        if ($errno == 1 && $message === "MySQL server has gone away") {
            return 2006;
        }
        return $errno;
    }

    // The server's own error text, as php's mysqli_error() reports it: the
    // bridge renders a server error as "MySqlError { ERROR 1146 (42S02): <text> }",
    // so the wrapper is peeled off (client-side messages pass through unchanged).
    private static function __elephcServerMessage(string $message): string {
        // A transport failure (read timeout, dropped socket) is mysqlnd's
        // CR_SERVER_GONE_ERROR; the bridge renders it as a codec/IO error.
        if (strpos($message, "IO error") !== false || strpos($message, "IoError") !== false) {
            return "MySQL server has gone away";
        }
        $_prefix = "MySqlError { ERROR ";
        if (strlen($message) > strlen($_prefix) + 2
            && substr($message, 0, strlen($_prefix)) === $_prefix
            && substr($message, -2) === " }") {
            $_at = strpos($message, "): ");
            if ($_at !== false) {
                return substr($message, $_at + 3, strlen($message) - $_at - 5);
            }
        }
        return $message;
    }

    // MYSQLI_REPORT_INDEX: when the last statement's server status says it used
    // no (good) index, php reports "No index used in query/prepared statement …"
    // (an exception under STRICT, a warning otherwise — elephc writes STDERR).
    private function __elephcReportIndex(string $query): void {
        if ($this->conn < 0) {
            return;
        }
        // (The no_index_used / bad_index_used statistics are counted when the
        // result header is read, see __elephcHeaderStats().)
        $_status = elephc_pdo_mysql_server_status($this->conn);
        $_index = "";
        if (($_status & 16) != 0) {
            $_index = "Bad index";
        } elseif (($_status & 32) != 0) {
            $_index = "No index";
        } else {
            return;
        }
        if ((mysqli::$reportMode & 4) == 0) {
            return;
        }
        $_message = $_index . " used in query/prepared statement " . $query;
        if ((mysqli::$reportMode & 2) != 0) {
            throw mysqli_sql_exception::__elephcCreate($_message, 0, "00000");
        }
        mysqli::__elephcWarn($_message);
    }

    // mysqlnd's MYSQLI_ENABLE_MQ / MYSQLI_DISABLE_MQ: COM_SET_OPTION only when
    // the link's recorded state changes (counted as com_stmt_set_option).
    private function enableMultiStatements(): void {
        if (!$this->multiStatementsOn) {
            $this->bump("com_stmt_set_option");
            elephc_pdo_mysql_set_server_option($this->conn, 0);
            $this->multiStatementsOn = true;
        }
    }

    private function disableMultiStatements(): void {
        if ($this->multiStatementsOn) {
            $this->bump("com_stmt_set_option");
            elephc_pdo_mysql_set_server_option($this->conn, 1);
            $this->multiStatementsOn = false;
        }
    }

    // Increments one per-connection statistic and its process-wide twin.
    private function bump(string $key): void {
        $this->connStats[$key] = ((int) ($this->connStats[$key] ?? 0)) + 1;
        $_client = mysqli::$clientStats;
        $_client[$key] = ((int) ($_client[$key] ?? 0)) + 1;
        mysqli::$clientStats = $_client;
    }

    // Adds $by to one mysqlnd statistic: the connection's and the process-wide
    // one, or only the process-wide one ($connection false) for the counters
    // mysqlnd bumps with MYSQLND_INC_GLOBAL_STATISTIC.
    private function __elephcStat(string $key, int $by, bool $connection): void {
        if ($by == 0) {
            return;
        }
        if ($connection) {
            $this->connStats[$key] = ((int) ($this->connStats[$key] ?? 0)) + $by;
        }
        $_client = mysqli::$clientStats;
        $_client[$key] = ((int) ($_client[$key] ?? 0)) + $by;
        mysqli::$clientStats = $_client;
    }

    // mysqlnd_query_read_result_set_header: a result set counts as a result-set
    // query plus the server's index / slow-query flags; anything else as a
    // non-result-set query plus (outside prepared statements) its affected rows.
    private function __elephcHeaderStats(int $cols, bool $normal): void {
        if ($cols > 0) {
            $this->bump("result_set_queries");
            $_status = elephc_pdo_mysql_server_status($this->conn);
            if (($_status & 16) != 0) {
                $this->bump("bad_index_used");
            } elseif (($_status & 32) != 0) {
                $this->bump("no_index_used");
            } elseif (($_status & 2048) != 0) {
                $this->bump("slow_queries");
            }
            return;
        }
        $this->bump("non_result_set_queries");
        if ($normal) {
            $_affected = elephc_pdo_changes($this->conn);
            if ($_affected > 0) {
                $this->__elephcStat("rows_affected_normal", $_affected, true);
            }
        }
    }

    // Friend-channel twin of bump() for mysqli_stmt / mysqli_result.
    private function __elephcBump(string $key): void {
        $this->bump($key);
    }

    // Decrements the process-wide active-connection gauge on close.
    private function dropActive(): void {
        $_client = mysqli::$clientStats;
        $_client["active_connections"] = ((int) ($_client["active_connections"] ?? 0)) - 1;
        if ($this->handlePersistent) {
            $_client["active_persistent_connections"] = ((int) ($_client["active_persistent_connections"] ?? 0)) - 1;
        }
        mysqli::$clientStats = $_client;
    }

    // Records the connection's live bridge error state WITHOUT reporting it:
    // mysqli::prepare() copies a failed statement prepare onto the link before
    // the statement reports it (php's close_stmt_and_copy_errors).
    private function __elephcAdoptBridgeError(): void {
        $this->error = mysqli::__elephcServerMessage(elephc_pdo_errmsg($this->conn));
        $this->errno = mysqli::__elephcClientErrno(elephc_pdo_errcode($this->conn), $this->error);
        $this->sqlstate = elephc_pdo_sqlstate($this->conn);
        if ($this->sqlstate === "") {
            $this->sqlstate = "HY000";
        }
        $this->error_list = [["errno" => $this->errno, "sqlstate" => $this->sqlstate, "error" => $this->error]];
    }

    // Dispatches mysqli_report for the link's recorded error (a streaming
    // result's failed read, after it copied the error and released the set).
    private function __elephcReportError(): void {
        $this->report($this->error, $this->errno, $this->sqlstate);
    }

    // Returns the bridge handle (for mysqli_stmt through the friend channel).
    private function __elephcConn(): int {
        return $this->conn;
    }

    // Whether a MYSQLI_ASYNC query waits to be reaped (mysqli::poll()).
    private function __elephcAsyncPending(): bool {
        return $this->asyncPending;
    }

    // Waits up to $timeoutUs microseconds for this link's async query to finish.
    private function __elephcAsyncReady(int $timeoutUs): bool {
        return $this->asyncPending && elephc_pdo_mysql_async_wait($this->conn, $timeoutUs) == 1;
    }

    // Releases the bridge handle and every piece of per-connection state: an
    // open transaction is rolled back first (keeping a persistent handle clean
    // for the pool), the multi_query batch is finalized, and unconsumed
    // results die with the connection (php-src frees all pending state).
    private function releaseConnection(): void {
        if ($this->conn < 0) {
            return;
        }
        $this->closeLive();
        if (elephc_pdo_in_transaction($this->conn) === 1) {
            elephc_pdo_rollback($this->conn);
        }
        elephc_pdo_release($this->conn, 0);
        $this->conn = -1;
        $this->asyncPending = false;
        if ($this->persistent) {
            mysqli::$openPersistentLinks = mysqli::$openPersistentLinks - 1;
        }
        $this->persistent = false;
    }

    // Sends a MYSQLI_ASYNC query: the bridge runs it on a worker thread and
    // this returns at once. Only send-time failures (unconnected / busy link,
    // the multi-statement guard) are reported here, like mysqlnd's async send;
    // the query's own outcome waits for reap_async_query().
    private function sendAsyncQuery(string $query, bool $unbuffered): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        $this->disableMultiStatements();
        $this->bump("com_query");
        if (elephc_pdo_mysql_async_send($this->conn, $query, (int) $unbuffered) < 0) {
            return $this->opFailed();
        }
        $this->asyncPending = true;
        $this->asyncUnbuffered = $unbuffered;
        $this->clearError();
        return true;
    }

    // Runs SHOW WARNINGS and wraps its rows in a mysqli_warning chain (false
    // when the server reports none). Friend channel: mysqli_stmt::get_warnings
    // uses the statement's connection.
    private function __elephcFetchWarnings(): mysqli_warning|false {
        if ($this->conn < 0 || $this->__elephcHasPendingResults()) {
            return false;
        }
        $_stmt = elephc_pdo_prepare($this->conn, "SHOW WARNINGS", 1);
        if ($_stmt < 0) {
            return false;
        }
        $_codes = [];
        $_messages = [];
        $_rc = elephc_pdo_step($_stmt);
        while ($_rc == 1) {
            $_codes[] = (int) $this->columnValue($_stmt, 1);
            $_messages[] = (string) $this->columnValue($_stmt, 2);
            $_rc = elephc_pdo_step($_stmt);
        }
        elephc_pdo_finalize($_stmt);
        if (count($_codes) == 0) {
            return false;
        }
        return mysqli_warning::__elephcFromRows($_codes, $_messages);
    }

    // Guards statement-issuing operations while multi_query result sets (or a
    // real_query result) remain unconsumed: php-src raises
    // CR_COMMANDS_OUT_OF_SYNC (2014) there.
    private function requireNoPendingResults(): bool {
        if ($this->__elephcHasPendingResults()) {
            $this->syntheticFailure(2014, "Commands out of sync; you can't run this command now", "HY000");
            return false;
        }
        return true;
    }

    // Read-only pending probe for mysqli_stmt's two-step prepare()/execute()
    // guard (the statement records the 2014 on ITS OWN error state).
    private function __elephcHasPendingResults(): bool {
        return $this->liveStmt >= 0 || $this->asyncPending || $this->busyStatement >= 0;
    }

    // The same probe for a mysqli_stmt's own operations: its own unread result
    // set does not block it (mysqlnd flushes it on re-execute / reset / close).
    private function __elephcBusyFor(int $handle): bool {
        return $this->liveStmt >= 0 || $this->asyncPending
            || ($this->busyStatement >= 0 && $this->busyStatement != $handle);
    }

    // Records mysqlnd's 2014 on the link without reporting it: a cursor
    // statement refused while another result owns the link reports on its own
    // error state, and the link's errno reads 2014 too.
    private function __elephcOutOfSync(): void {
        $this->errno = 2014;
        $this->error = "Commands out of sync; you can't run this command now";
        $this->sqlstate = "HY000";
        $this->error_list = [["errno" => 2014, "sqlstate" => "HY000", "error" => "Commands out of sync; you can't run this command now"]];
    }

    // mysqli_stmt result bookkeeping: a statement whose result set (or further
    // CALL result sets) is still on the server keeps the link out of sync, like
    // mysqlnd's CONN_FETCHING_DATA / CONN_NEXT_RESULT_PENDING states.
    private function __elephcStatementBusy(int $handle): void {
        $this->busyStatement = $handle;
    }

    private function __elephcStatementIdle(int $handle): void {
        if ($this->busyStatement == $handle) {
            $this->busyStatement = -1;
        }
    }

    // Percent-encodes one packed driver-config value ('%' first, then ';' and
    // '='), the encoding the bridge decodes after splitting the config.
    private function encodeDriverOption(string $value): string {
        return str_replace("=", "%3D", str_replace(";", "%3B", str_replace("%", "%25", $value)));
    }

    // Renders a transaction $name as php-src's ` /*name*/` SQL comment suffix
    // ("" when $name is null). An empty $name is a ValueError ONLY for
    // begin_transaction ($emptyThrows). A non-empty name is STRIPPED to php's
    // allowlist [A-Za-z0-9 -_=] before it is wrapped: a name starting with `!`
    // would otherwise open an EXECUTABLE `/*! … */` comment.
    private function transactionComment(string $context, ?string $name, bool $emptyThrows): string {
        if ($name === null) {
            return "";
        }
        if ($name === "") {
            if ($emptyThrows) {
                throw new ValueError($context . "(): Argument #2 (\$name) must not be empty");
            }
            return " /**/";
        }
        $_clean = "";
        $_len = strlen($name);
        for ($_i = 0; $_i < $_len; $_i++) {
            $_c = substr($name, $_i, 1);
            $_o = ord($_c);
            if (($_o >= 48 && $_o <= 57)
                || ($_o >= 65 && $_o <= 90)
                || ($_o >= 97 && $_o <= 122)
                || $_c === " " || $_c === "-" || $_c === "_" || $_c === "=") {
                $_clean = $_clean . $_c;
            }
        }
        return " /*" . $_clean . "*/";
    }

    // Renders the COMMIT/ROLLBACK completion flags (MYSQLI_TRANS_COR_*), matching
    // php-src's mysqlnd: AND [NO] CHAIN and [NO] RELEASE.
    private function transactionCorFlags(int $flags): string {
        $_sql = "";
        if (($flags & 1) != 0 && ($flags & 2) == 0) {
            $_sql = $_sql . " AND CHAIN";
        } elseif (($flags & 2) != 0 && ($flags & 1) == 0) {
            $_sql = $_sql . " AND NO CHAIN";
        }
        if (($flags & 4) != 0 && ($flags & 8) == 0) {
            $_sql = $_sql . " RELEASE";
        } elseif (($flags & 8) != 0 && ($flags & 4) == 0) {
            $_sql = $_sql . " NO RELEASE";
        }
        return $_sql;
    }

    // php returns an AUTO_INCREMENT id beyond PHP_INT_MAX (a BIGINT UNSIGNED
    // column) as its decimal string, any other id as an int.
    private static function __elephcUnsignedId(string $text): int|string {
        if ($text === "") {
            return 0;
        }
        $_id = (int) $text;
        return (string) $_id === $text ? $_id : $text;
    }

    // Refreshes the connection status fields from the last command's OK packet,
    // matching php (which updates them after EVERY command).
    private function refreshStatus(): void {
        $this->affected_rows = elephc_pdo_changes($this->conn);
        $this->insert_id = self::__elephcUnsignedId(elephc_pdo_last_insert_id_text($this->conn, ""));
        $this->warning_count = elephc_pdo_warning_count($this->conn);
        $_info = elephc_pdo_mysql_info($this->conn);
        $this->info = $_info === "" ? null : $_info;
    }

    // php 8 raises `Error: mysqli object is not fully initialized` for any
    // operation on a `mysqli_init()` / argument-less `new mysqli()` object (or a
    // failed/closed connection), regardless of mysqli_report mode.
    // The procedural accessors' link validation (MYSQLI_FETCH_RESOURCE_CONN).
    private function __elephcRequireValid(): void {
        $this->requireInitialized();
    }

    private function requireInitialized(): void {
        if ($this->linkStatus == 2) {
            throw new Error("mysqli object is already closed");
        }
        if ($this->linkStatus == 0) {
            throw new Error("mysqli object is not fully initialized");
        }
    }

    // Guards every operation that needs a live connection. A link whose
    // reconnect failed is still "connected" for php but has no connection:
    // mysqlnd answers CR_COMMANDS_OUT_OF_SYNC (2014) and the call fails.
    private function requireConnection(): bool {
        $this->requireInitialized();
        if ($this->linkGone) {
            $this->syntheticFailure(2006, "MySQL server has gone away", "HY000");
            return false;
        }
        if ($this->conn < 0) {
            $this->syntheticFailure(2014, "Commands out of sync; you can't run this command now", "HY000");
            return false;
        }
        return true;
    }

    // Records a client-side failure that has no live bridge error state
    // (unconnected handle, invalid charset) and dispatches mysqli_report.
    private function syntheticFailure(int $errno, string $message, string $sqlstate): void {
        $this->errno = $errno;
        $this->error = $message;
        $this->sqlstate = $sqlstate;
        $this->error_list = [["errno" => $errno, "sqlstate" => $sqlstate, "error" => $message]];
        $this->report($message, $errno, $sqlstate);
    }

    // Records a connect-time failure on both the instance (connect_errno /
    // connect_error, distinct from errno/error) and the process-wide statics
    // behind mysqli_connect_errno()/mysqli_connect_error(), then dispatches
    // mysqli_report. Always returns false so connect paths can tail-call it.
    private function connectFailure(int $errno, string $message, string $sqlstate): bool {
        $this->connect_errno = $errno;
        $this->connect_error = $message;
        $this->errno = $errno;
        $this->error = $message;
        $this->sqlstate = $sqlstate;
        $this->error_list = [["errno" => $errno, "sqlstate" => $sqlstate, "error" => $message]];
        mysqli::$lastConnectErrno = $errno;
        mysqli::$lastConnectError = $message;
        // php_mysqli_throw_sql_exception: a connect failure throws under
        // MYSQLI_REPORT_STRICT and is otherwise always an E_WARNING, whatever
        // the report mode.
        if ((mysqli::$reportMode & 2) != 0) {
            throw mysqli_sql_exception::__elephcCreate($message, $errno, $sqlstate);
        }
        mysqli::__elephcWarn("(" . $sqlstate . "/" . $errno . "): " . $message);
        return false;
    }

    // Refreshes errno/error/sqlstate/error_list from the live bridge error
    // state after a failed operation, then dispatches mysqli_report.
    private function opFailed(): bool {
        $this->error = mysqli::__elephcServerMessage(elephc_pdo_errmsg($this->conn));
        $this->errno = mysqli::__elephcClientErrno(elephc_pdo_errcode($this->conn), $this->error);
        $this->sqlstate = elephc_pdo_sqlstate($this->conn);
        if ($this->sqlstate === "") {
            $this->sqlstate = "HY000";
        }
        $this->error_list = [["errno" => $this->errno, "sqlstate" => $this->sqlstate, "error" => $this->error]];
        $this->report($this->error, $this->errno, $this->sqlstate);
        return false;
    }

    // Clears the per-operation error state after a successful operation.
    private function clearError(): void {
        $this->errno = 0;
        $this->error = "";
        $this->sqlstate = "00000";
        $this->error_list = [];
    }

    // mysqli_report dispatch: STRICT throws mysqli_sql_exception (never
    // PDOException), ERROR alone writes the message to STDERR, OFF is silent.
    private function report(string $message, int $errno, string $sqlstate): void {
        if ((mysqli::$reportMode & 2) != 0) {
            throw mysqli_sql_exception::__elephcCreate($message, $errno, $sqlstate);
        }
        if ((mysqli::$reportMode & 1) != 0) {
            mysqli::__elephcWarn("(" . $sqlstate . "/" . $errno . "): " . $message);
        }
    }

    // php_error_docref(NULL, E_WARNING, ...): a real E_WARNING (set_error_handler,
    // error_get_last, error_reporting) prefixed with the PHP-visible mysqli entry
    // point the program called — "mysqli::query(): " or "mysqli_query(): " — and
    // reported at the user's call-site line. The entry is the outermost mysqli
    // frame of the current trace (helpers and nested calls are inner frames).
    private static function __elephcWarn(string $message): void {
        $_entry = "";
        foreach (debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS) as $_frame) {
            $_function = (string) ($_frame["function"] ?? "");
            $_class = (string) ($_frame["class"] ?? "");
            if ($_class === "mysqli" || $_class === "mysqli_stmt" || $_class === "mysqli_result"
                || $_class === "mysqli_driver") {
                $_entry = $_class . "::" . $_function;
            } elseif ($_class === "" && strncmp($_function, "mysqli_", 7) == 0) {
                $_entry = $_function;
            }
        }
        __elephc_builtin_warning($_entry === "" ? $message : $_entry . "(): " . $message);
    }

    // Executes one statement through the bridge. Returns 0 = failure (error
    // state recorded and reported), 1 = success with no result set (DML/DDL:
    // affected_rows/insert_id set), 2 = success with a result set left
    // unclaimed on $liveStmt (claimStored() / claimStream() pick it up).
    // $unbuffered prepares the statement in the bridge's streaming mode, so the
    // rows stay on the server until they are fetched.
    private function runQuery(string $query, bool $unbuffered): int {
        if (!$this->requireConnection()) {
            return 0;
        }
        if (!$this->requireNoPendingResults()) {
            return 0;
        }
        // Like mysqlnd, query() / real_query() switch a multi_query()-enabled
        // session back to single statements, so the SERVER rejects a "1; DROP
        // TABLE …" string with its own 1064 syntax error.
        $this->disableMultiStatements();
        $this->bump("com_query");
        $_stmt = $this->prepareText($query, $unbuffered);
        if ($_stmt < 0) {
            $this->opFailed();
            return 0;
        }
        // A streaming (real_query / MYSQLI_USE_RESULT) execution reads only the
        // result header and metadata, like mysqlnd; rows come on demand (2 =
        // first row not read yet). A stored query() reads everything here anyway.
        $_rc = $unbuffered ? elephc_pdo_mysql_execute($_stmt) : elephc_pdo_step($_stmt);
        if ($_rc == 0 && $unbuffered) {
            $_rc = 2;
        }
        if ($_rc < 0) {
            $this->opFailed();
            elephc_pdo_finalize($_stmt);
            return 0;
        }
        // Column metadata is definitely known after the first step, including
        // for emulated prepares that only execute at step time.
        $_columnCount = elephc_pdo_column_count($_stmt);
        $this->__elephcHeaderStats($_columnCount, true);
        if ($_columnCount == 0) {
            $this->refreshStatus();
            $this->field_count = 0;
            elephc_pdo_finalize($_stmt);
            $this->clearError();
            return 1;
        }
        $this->liveStmt = $_stmt;
        $this->liveStep = $_rc;
        $this->liveSetPending = true;
        $this->field_count = $_columnCount;
        $this->clearError();
        return 2;
    }

    // Prepares a text-protocol statement, in the bridge's streaming
    // (unbuffered) mode when $unbuffered: the connection's buffering default is
    // flipped only around the prepare, which snapshots it.
    private function prepareText(string $query, bool $unbuffered): int {
        if (!$unbuffered) {
            return elephc_pdo_prepare($this->conn, $query, 1);
        }
        elephc_pdo_set_buffered_query($this->conn, 0);
        $_stmt = elephc_pdo_prepare($this->conn, $query, 1);
        elephc_pdo_set_buffered_query($this->conn, 1);
        return $_stmt;
    }

    // Records the live statement's CURRENT set after a step: a column-less set
    // (an OK packet inside a batch) refreshes the status and is consumed at
    // once; a result set waits for store_result() / use_result().
    private function enterSet(int $firstStep): void {
        $_cols = elephc_pdo_column_count($this->liveStmt);
        $this->__elephcHeaderStats($_cols, true);
        if ($_cols == 0) {
            $this->refreshStatus();
            $this->field_count = 0;
            $this->clearError();
            $this->finishSet();
            return;
        }
        $this->liveStep = $firstStep;
        $this->liveSetPending = true;
        $this->field_count = $_cols;
        $this->clearError();
    }

    // Buffers the unclaimed current set into an independent mysqli_result
    // (MYSQLI_STORE_RESULT), or null when there is none or a step failed.
    private function claimStored(): ?mysqli_result {
        if (!$this->liveSetPending) {
            return null;
        }
        $this->liveSetPending = false;
        $_cols = elephc_pdo_column_count($this->liveStmt);
        $_result = $this->drainResultSet($this->liveStmt, $this->liveStep, $_cols);
        if ($_result === null) {
            $this->closeLive();
            return null;
        }
        $this->finishSet();
        return $_result;
    }

    // Hands the unclaimed current set to a streaming mysqli_result
    // (MYSQLI_USE_RESULT): every fetch reads the next row from the server, and
    // the link stays busy (2014) until the set is exhausted or freed.
    private function claimStream(): ?mysqli_result {
        if (!$this->liveSetPending) {
            return null;
        }
        $this->liveSetPending = false;
        $this->streaming = true;
        $_stmt = $this->liveStmt;
        $_cols = elephc_pdo_column_count($_stmt);
        $_names = [];
        $_tables = [];
        $_natives = [];
        $_flags = [];
        $_lens = [];
        $_decimals = [];
        $_kinds = [];
        for ($_i = 0; $_i < $_cols; $_i++) {
            $_names[] = elephc_pdo_column_name($_stmt, $_i);
            $_tables[] = elephc_pdo_column_table_name($_stmt, $_i);
            $_native = elephc_pdo_column_native_type($_stmt, $_i);
            $_natives[] = $_native;
            $_flags[] = elephc_pdo_column_flags($_stmt, $_i);
            $_lens[] = elephc_pdo_column_len($_stmt, $_i);
            $_decimals[] = elephc_pdo_column_precision($_stmt, $_i);
            $_kinds[] = mysqli_result::__elephcCellKind($_native, (int) $_flags[$_i], $this->optIntAndFloatNative, false);
        }
        $this->bump("unbuffered_sets");
        // mysqlnd reports -1 affected rows until an unbuffered set is read.
        $this->affected_rows = -1;
        $this->insert_id = 0;
        $this->field_count = $_cols;
        $this->info = null;
        $this->clearError();
        return mysqli_result::__elephcFromStream($this, $_stmt, $this->liveStep, $_names, $_tables, $_natives, $_flags, $_lens, $_decimals, $_kinds);
    }

    // Called by a streaming mysqli_result once its set is exhausted or freed. A
    // result kept across a reconnect names a statement the link no longer streams,
    // so it must not finish the link's current stream.
    private function __elephcStreamFinished(int $stmt, int $rows): void {
        if (!$this->streaming || $this->liveStmt < 0 || $this->liveStmt != $stmt) {
            return;
        }
        $this->affected_rows = $rows;
        $this->finishSet();
    }

    // The current set is consumed: probe for a further set (multi_query /
    // CALL) and release the statement when the batch is done.
    private function finishSet(): void {
        $this->liveSetPending = false;
        $this->streaming = false;
        $this->warning_count = elephc_pdo_warning_count($this->conn);
        // The set's closing EOF/OK says whether another follows; mysqlnd reads
        // that next set only on next_result().
        $this->multiMore = elephc_pdo_mysql_more_results($this->liveStmt) == 1;
        if (!$this->multiMore) {
            elephc_pdo_finalize($this->liveStmt);
            $this->liveStmt = -1;
        }
    }

    // Abandons the live statement (close, reconnect, change_user, failures).
    private function closeLive(): void {
        if ($this->liveStmt >= 0) {
            elephc_pdo_finalize($this->liveStmt);
            $this->liveStmt = -1;
        }
        $this->liveSetPending = false;
        $this->streaming = false;
        $this->multiMore = false;
    }

    // Drains the current result set of a bridge statement (whose first step
    // verdict is $firstStep) into an independent mysqli_result and refreshes
    // the connection status for a SELECT. Returns null — error recorded and
    // reported — when a step fails mid-drain.
    private function drainResultSet(int $stmt, int $firstStep, int $cols): ?mysqli_result {
        $_names = [];
        $_tables = [];
        $_natives = [];
        $_flags = [];
        $_lens = [];
        $_decimals = [];
        for ($_i = 0; $_i < $cols; $_i++) {
            $_names[] = elephc_pdo_column_name($stmt, $_i);
            $_tables[] = elephc_pdo_column_table_name($stmt, $_i);
            $_natives[] = elephc_pdo_column_native_type($stmt, $_i);
            $_flags[] = elephc_pdo_column_flags($stmt, $_i);
            $_lens[] = elephc_pdo_column_len($stmt, $_i);
            $_decimals[] = elephc_pdo_column_precision($stmt, $_i);
        }
        // mysqlnd's per-column cell conversion (MYSQLI_OPT_INT_AND_FLOAT_NATIVE,
        // BIT decoding).
        $_native = [];
        for ($_i = 0; $_i < $cols; $_i++) {
            $_native[] = mysqli_result::__elephcCellKind((string) $_natives[$_i], (int) $_flags[$_i], $this->optIntAndFloatNative, false);
        }
        // Cells are buffered FLAT in row-major order (see mysqli_result).
        $_cells = [];
        $_rowCount = 0;
        $_rc = $firstStep == 2 ? elephc_pdo_step($stmt) : $firstStep;
        while ($_rc == 1) {
            for ($_i = 0; $_i < $cols; $_i++) {
                $_cells[] = mysqli_result::__elephcConvertCell($this->columnValue($stmt, $_i), (int) $_native[$_i], (int) $_lens[$_i]);
            }
            $_rowCount = $_rowCount + 1;
            $_rc = elephc_pdo_step($stmt);
        }
        if ($_rc < 0) {
            $this->opFailed();
            return null;
        }
        $this->bump("buffered_sets");
        $this->__elephcStat("rows_fetched_from_server_normal", $_rowCount, true);
        $this->__elephcStat("rows_buffered_from_client_normal", $_rowCount, true);
        // php-src: for a SELECT, affected_rows mirrors num_rows and insert_id
        // resets to 0 (the statement generated no AUTO_INCREMENT value).
        $this->affected_rows = $_rowCount;
        $this->insert_id = 0;
        $this->field_count = $cols;
        $this->warning_count = elephc_pdo_warning_count($this->conn);
        $this->info = null;
        $this->clearError();
        $_result = mysqli_result::__elephcFromDrain($_cells, $_rowCount, $_names, $_tables, $_natives, $_flags, $_lens, $_decimals);
        $_result->__elephcCountFor($this, false);
        return $_result;
    }


    // Decodes one cell of the current row (int / float / null / length-counted
    // TEXT-or-BLOB copy so embedded NUL bytes survive).
    private function columnValue(int $stmt, int $index): mixed {
        $_type = elephc_pdo_column_type($stmt, $index);
        if ($_type == 1) {
            return elephc_pdo_column_int($stmt, $index);
        }
        if ($_type == 2) {
            return elephc_pdo_column_double($stmt, $index);
        }
        if ($_type == 5) {
            return null;
        }
        // The $_len == 0 guard is load-bearing: the bridge returns a NULL
        // pointer for an empty buffer and ptr_read_string fatals on NULL.
        $_len = elephc_pdo_column_data_len($stmt, $index);
        if ($_len > 0) {
            return __elephc_ptr_read_string(elephc_pdo_column_data_ptr($stmt, $index), $_len);
        }
        return "";
    }

    // "8.0.36-log" -> 80036, php-src's major*10000 + minor*100 + patch. The
    // bridge's client string is "mysql x.y.z"; strip that prefix first.
    private function versionStringToInt(string $version): int {
        if (str_starts_with($version, "mysql ")) {
            $version = substr($version, 6);
        }
        $_parts = explode(".", $version);
        $_major = (int) $_parts[0];
        $_minor = count($_parts) > 1 ? (int) $_parts[1] : 0;
        $_patch = count($_parts) > 2 ? (int) $_parts[2] : 0;
        return $_major * 10000 + $_minor * 100 + $_patch;
    }

    // [A-Za-z0-9_] identifier filter shared by set_charset, same rule the PDO
    // DSN charset key enforces bridge-side.
    private function charsetIdentIsValid(string $charset): bool {
        $_len = strlen($charset);
        if ($_len == 0) {
            return false;
        }
        for ($_i = 0; $_i < $_len; $_i++) {
            $_c = ord(substr($charset, $_i, 1));
            $_ok = ($_c >= 48 && $_c <= 57) || ($_c >= 65 && $_c <= 90) || ($_c >= 97 && $_c <= 122) || $_c == 95;
            if (!$_ok) {
                return false;
            }
        }
        return true;
    }
}
"##;
