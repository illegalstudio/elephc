<?php

const SNMP_OID_OUTPUT_SUFFIX = 1;
const SNMP_OID_OUTPUT_MODULE = 2;
const SNMP_OID_OUTPUT_FULL = 3;
const SNMP_OID_OUTPUT_NUMERIC = 4;
const SNMP_OID_OUTPUT_UCD = 5;
const SNMP_OID_OUTPUT_NONE = 6;
const SNMP_VALUE_LIBRARY = 0;
const SNMP_VALUE_PLAIN = 1;
const SNMP_VALUE_OBJECT = 2;
const SNMP_BIT_STR = 3;
const SNMP_OCTET_STR = 4;
const SNMP_OPAQUE = 68;
const SNMP_NULL = 5;
const SNMP_OBJECT_ID = 6;
const SNMP_IPADDRESS = 64;
const SNMP_COUNTER = 66;
const SNMP_UNSIGNED = 66;
const SNMP_TIMETICKS = 67;
const SNMP_UINTEGER = 71;
const SNMP_INTEGER = 2;
const SNMP_COUNTER64 = 70;

const SNMP_VERSION_1 = 0;
const SNMP_VERSION_2C = 1;
const SNMP_VERSION_3 = 3;
const SNMP_VALUE_INTEGER = 2;
const SNMP_VALUE_OCTET_STRING = 4;
const SNMP_VALUE_NULL = 5;
const SNMP_VALUE_OBJECT_ID = 6;
const SNMP_VALUE_IPADDRESS = 64;
const SNMP_VALUE_COUNTER = 65;
const SNMP_VALUE_GAUGE = 66;
const SNMP_VALUE_TIMETICKS = 67;
const SNMP_VALUE_OPAQUE = 68;
const SNMP_VALUE_COUNTER64 = 70;
const SNMP_VALUE_NOSUCHOBJECT = 128;
const SNMP_VALUE_NOSUCHINSTANCE = 129;
const SNMP_VALUE_ENDOFMIBVIEW = 130;

class SNMPException extends \Exception
{
}

extern "elephc_snmp" {
    function elephc_snmp_open(int $version, string $host, int $hostLength, string $community, int $communityLength, int $timeoutMs, int $retries): int;
    function elephc_snmp_close(int $session): void;
    function elephc_snmp_request(int $session, string $oid, int $oidLength, int $pdu): int;
    function elephc_snmp_request_set(int $session, string $oid, int $oidLength, string $type, int $typeLength, string $value, int $valueLength): int;
    function elephc_snmp_request_bulk(int $session, string $oid, int $oidLength, int $nonRepeaters, int $maxRepetitions): int;
    function elephc_snmp_read_mib(string $path, int $pathLength): int;
    function elephc_snmp_init_mib(string $dirs, int $dirsLength): void;
    function elephc_snmp_resolve_oid(string $oid, int $oidLength): ptr;
    function elephc_snmp_render_oid(string $oid, int $oidLength, int $format): ptr;
    function elephc_snmp_enum_label(string $oid, int $oidLength, int $value): ptr;
    function elephc_snmp_response_count(int $response): int;
    function elephc_snmp_response_error_status(int $response): int;
    function elephc_snmp_response_error_index(int $response): int;
    function elephc_snmp_response_oid(int $response, int $index): ptr;
    function elephc_snmp_response_type(int $response, int $index): int;
    function elephc_snmp_response_value(int $response, int $index): ptr;
    function elephc_snmp_response_free(int $response): void;
    function elephc_snmp_string_len(): int;
    function elephc_snmp_set_auth(int $session, string $protocol, int $protocolLength, string $password, int $passwordLength): int;
    function elephc_snmp_set_priv(int $session, string $protocol, int $protocolLength, string $password, int $passwordLength): int;
    function elephc_snmp_set_context(int $session, string $name, int $nameLength, string $engine, int $engineLength): int;
}

class SNMP
{
    public const int VERSION_1 = 0;
    public const int VERSION_2C = 1;
    public const int VERSION_2c = 1;
    public const int VERSION_3 = 3;
    public const int ERRNO_NOERROR = 0;
    public const int ERRNO_ANY = 126;
    public const int ERRNO_GENERIC = 2;
    public const int ERRNO_TIMEOUT = 4;
    public const int ERRNO_ERROR_IN_REPLY = 8;
    public const int ERRNO_OID_NOT_INCREASING = 16;
    public const int ERRNO_OID_PARSING_ERROR = 32;
    public const int ERRNO_MULTIPLE_SET_QUERIES = 64;

    public array $info = [];
    public ?int $max_oids = null;
    public bool $oid_increasing_check = true;
    public bool $quick_print = false;
    public bool $enum_print = false;
    public bool $numeric_index = false;
    public bool $numeric_timeticks = false;
    public bool $extended_index = false;
    public bool $dont_print_units = false;
    public bool $escape_quotes = false;
    public bool $print_hex_text = false;
    public int $valueretrieval = 0;
    public int $oid_output_format = 0;
    public int $exceptions_enabled = 0;

    private int $__session = 0;
    private int $__version = 0;
    private int $__string_output = 0;
    private string $__hostname = "";
    private int $__timeout = 1000000;
    private int $__retries = 5;
    private int $__errno = 0;
    private string $__error = "";

    public function __construct(int $version, string $hostname, string $community, int $timeout = -1, int $retries = -1)
    {
        if ($version !== self::VERSION_1 && $version !== self::VERSION_2C
            && $version !== self::VERSION_3) {
            throw new \SNMPException("Unsupported SNMP version");
        }
        $this->__version = $version;
        $this->enum_print = __ElephcSnmpConfig::$enum_print;
        $this->oid_output_format = __ElephcSnmpConfig::$oid_output_format;
        $this->valueretrieval = __ElephcSnmpConfig::$valueretrieval;
        $this->quick_print = __ElephcSnmpConfig::$quick_print;
        $this->numeric_timeticks = __ElephcSnmpConfig::$numeric_timeticks;
        $this->numeric_index = __ElephcSnmpConfig::$numeric_index;
        $this->extended_index = __ElephcSnmpConfig::$extended_index;
        $this->dont_print_units = __ElephcSnmpConfig::$dont_print_units;
        $this->escape_quotes = __ElephcSnmpConfig::$escape_quotes;
        $this->print_hex_text = __ElephcSnmpConfig::$print_hex_text;
        $this->__hostname = $hostname;
        $this->__timeout = $timeout < 0 ? 1000000 : $timeout;
        $this->__retries = $retries < 0 ? 5 : $retries;
        $this->info = [
            "hostname" => $hostname,
            "timeout" => $this->__timeout,
            "retries" => $this->__retries,
        ];
        $this->__session = __elephc_snmp_open($version, $hostname, $community, $timeout, $retries);
        if ($this->__session < 0) {
            throw new \SNMPException("Unable to open SNMP session");
        }
    }

    public function setSecurity(
        string $securityLevel,
        string $authProtocol = "",
        string $authPassphrase = "",
        string $privacyProtocol = "",
        string $privacyPassphrase = "",
        string $contextName = "",
        string $contextEngineId = ""
    ): bool {
        if ($this->__version !== self::VERSION_3) {
            return false;
        }
        $level = \strtolower($securityLevel);
        if ($level !== "noauthnopriv" && $level !== "authnopriv" && $level !== "authpriv") {
            throw new \ValueError("Security level must be one of \"noAuthNoPriv\", \"authNoPriv\", or \"authPriv\"");
        }
        if ($level === "noauthnopriv") {
            return true;
        }
        if ($authProtocol === "" || $authPassphrase === "") {
            return false;
        }
        if (\elephc_snmp_set_auth($this->__session, $authProtocol, \strlen($authProtocol), $authPassphrase, \strlen($authPassphrase)) !== 0) {
            return false;
        }
        if ($level === "authpriv") {
            if ($privacyProtocol === "" || $privacyPassphrase === "") {
                return false;
            }
            if (\elephc_snmp_set_priv($this->__session, $privacyProtocol, \strlen($privacyProtocol), $privacyPassphrase, \strlen($privacyPassphrase)) !== 0) {
                return false;
            }
        }
        if ($contextName !== "" || $contextEngineId !== "") {
            if (\elephc_snmp_set_context($this->__session, $contextName, \strlen($contextName), $contextEngineId, \strlen($contextEngineId)) !== 0) {
                return false;
            }
        }
        return true;
    }

    public function setOidOutputFormat(\Snmp\OidOutput $format): bool
    {
        $this->oid_output_format = __elephc_snmp_oid_output_format($format);
        return true;
    }

    public function setStringOutputFormat(\Snmp\StringOutput $format): bool
    {
        $this->__string_output = __elephc_snmp_string_output_format($format);
        return true;
    }

    public function getErrno(): int
    {
        return $this->__errno;
    }

    public function getError(): string
    {
        return $this->__error;
    }

    public function get(array|string $object_id, bool $preserveKeys = false)
    {
        $result = __elephc_snmp_session_get(
            $this->__session,
            $object_id,
            0xA0,
            $this->enum_print,
            $this->valueretrieval,
            $this->quick_print,
            $this->numeric_timeticks
        );
        if ($result === false) {
            $this->__errno = self::ERRNO_TIMEOUT;
            $this->__error = "SNMP: No response from " . $this->__hostname;
        }
        return $result;
    }

    public function getnext(array|string $object_id)
    {
        $result = __elephc_snmp_session_get(
            $this->__session,
            $object_id,
            0xA1,
            $this->enum_print,
            $this->valueretrieval,
            $this->quick_print,
            $this->numeric_timeticks
        );
        if ($result === false) {
            $this->__errno = self::ERRNO_TIMEOUT;
            $this->__error = "SNMP: No response from " . $this->__hostname;
        }
        return $result;
    }

    public function walk(array|string $object_id, bool $suffixAsKey = false, int $maxRepetitions = -1, int $nonRepeaters = -1): array|false
    {
        if ($this->__version === self::VERSION_1) {
            $result = __elephc_snmp_session_walk(
                $this->__session,
                $object_id,
                $this->oid_output_format,
                $this->enum_print,
                $suffixAsKey,
                $this->valueretrieval,
                $this->quick_print,
                $this->numeric_timeticks
            );
        } else {
            $repetitions = $maxRepetitions < 0 ? 20 : $maxRepetitions;
            if ($repetitions < 1) {
                $repetitions = 1;
            }
            $nonRepeating = $nonRepeaters < 0 ? 0 : $nonRepeaters;
            $result = __elephc_snmp_session_bulk_walk(
                $this->__session,
                $object_id,
                $repetitions,
                $nonRepeating,
                $this->oid_output_format,
                $this->enum_print,
                $suffixAsKey,
                $this->valueretrieval,
                $this->quick_print,
                $this->numeric_timeticks
            );
        }
        if ($result === false) {
            $this->__errno = self::ERRNO_TIMEOUT;
            $this->__error = "SNMP: No response from " . $this->__hostname;
        }
        return $result;
    }

    public function set(array|string $object_id, array|string $type, array|string $value): bool
    {
        $result = __elephc_snmp_session_set($this->__session, $object_id, $type, $value);
        if (!$result) {
            $this->__errno = self::ERRNO_ERROR_IN_REPLY;
            $this->__error = "SNMP: Error in packet";
        } else {
            $this->__errno = self::ERRNO_NOERROR;
            $this->__error = "";
        }
        return $result;
    }

    public function close(): bool
    {
        if ($this->__session !== 0) {
            \elephc_snmp_close($this->__session);
            $this->__session = 0;
        }
        return true;
    }

    public function __destruct()
    {
        $this->close();
    }
}

function snmp_get(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1)
{
    return __elephc_snmp_get(SNMP_VERSION_1, $hostname, $community, $object_id, 0xA0, $timeout, $retries);
}

function snmp2_get(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1)
{
    return __elephc_snmp_get(SNMP_VERSION_2C, $hostname, $community, $object_id, 0xA0, $timeout, $retries);
}

function snmp_getnext(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1)
{
    return __elephc_snmp_get(SNMP_VERSION_1, $hostname, $community, $object_id, 0xA1, $timeout, $retries);
}

function snmp2_getnext(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1)
{
    return __elephc_snmp_get(SNMP_VERSION_2C, $hostname, $community, $object_id, 0xA1, $timeout, $retries);
}

function snmp_walk(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1): array|false
{
    return __elephc_snmp_walk(SNMP_VERSION_1, $hostname, $community, $object_id, $timeout, $retries);
}

function snmp2_walk(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1): array|false
{
    return __elephc_snmp_walk(SNMP_VERSION_2C, $hostname, $community, $object_id, $timeout, $retries);
}

function snmp3_get(
    string $hostname,
    string $security_name,
    string $security_level,
    string $auth_protocol,
    string $auth_passphrase,
    string $privacy_protocol,
    string $privacy_passphrase,
    array|string $object_id,
    int $timeout = -1,
    int $retries = -1
) {
    return __elephc_snmp3_get($hostname, $security_name, $security_level, $auth_protocol, $auth_passphrase, $privacy_protocol, $privacy_passphrase, $object_id, 0xA0, $timeout, $retries);
}

function snmp3_getnext(
    string $hostname,
    string $security_name,
    string $security_level,
    string $auth_protocol,
    string $auth_passphrase,
    string $privacy_protocol,
    string $privacy_passphrase,
    array|string $object_id,
    int $timeout = -1,
    int $retries = -1
) {
    return __elephc_snmp3_get($hostname, $security_name, $security_level, $auth_protocol, $auth_passphrase, $privacy_protocol, $privacy_passphrase, $object_id, 0xA1, $timeout, $retries);
}

function snmpset(string $hostname, string $community, array|string $object_id, array|string $type, array|string $value, int $timeout = -1, int $retries = -1): bool
{
    $session = __elephc_snmp_open(SNMP_VERSION_1, $hostname, $community, $timeout, $retries);
    if ($session < 0) {
        return false;
    }
    $result = __elephc_snmp_session_set($session, $object_id, $type, $value);
    \elephc_snmp_close($session);
    return $result;
}

function snmp2_set(string $hostname, string $community, array|string $object_id, array|string $type, array|string $value, int $timeout = -1, int $retries = -1): bool
{
    $session = __elephc_snmp_open(SNMP_VERSION_2C, $hostname, $community, $timeout, $retries);
    if ($session < 0) {
        return false;
    }
    $result = __elephc_snmp_session_set($session, $object_id, $type, $value);
    \elephc_snmp_close($session);
    return $result;
}

function snmp3_set(
    string $hostname,
    string $security_name,
    string $security_level,
    string $auth_protocol,
    string $auth_passphrase,
    string $privacy_protocol,
    string $privacy_passphrase,
    array|string $object_id,
    array|string $type,
    array|string $value,
    int $timeout = -1,
    int $retries = -1
): bool {
    $session = __elephc_snmp3_open($hostname, $security_name, $security_level, $auth_protocol, $auth_passphrase, $privacy_protocol, $privacy_passphrase, $timeout, $retries);
    if ($session < 0) {
        return false;
    }
    $result = __elephc_snmp_session_set($session, $object_id, $type, $value);
    \elephc_snmp_close($session);
    return $result;
}

function snmp3_walk(
    string $hostname,
    string $security_name,
    string $security_level,
    string $auth_protocol,
    string $auth_passphrase,
    string $privacy_protocol,
    string $privacy_passphrase,
    array|string $object_id,
    int $timeout = -1,
    int $retries = -1
): array|false {
    $session = __elephc_snmp3_open($hostname, $security_name, $security_level, $auth_protocol, $auth_passphrase, $privacy_protocol, $privacy_passphrase, $timeout, $retries);
    if ($session < 0) {
        return false;
    }
    $result = __elephc_snmp_session_bulk_walk(
        $session,
        $object_id,
        20,
        0,
        __ElephcSnmpConfig::$oid_output_format,
        __ElephcSnmpConfig::$enum_print,
        false,
        __ElephcSnmpConfig::$valueretrieval,
        __ElephcSnmpConfig::$quick_print,
        __ElephcSnmpConfig::$numeric_timeticks
    );
    \elephc_snmp_close($session);
    return $result;
}

function snmp2_real_walk(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1): array|false
{
    return \snmp2_walk($hostname, $community, $object_id, $timeout, $retries);
}

function snmp3_real_walk(
    string $hostname,
    string $security_name,
    string $security_level,
    string $auth_protocol,
    string $auth_passphrase,
    string $privacy_protocol,
    string $privacy_passphrase,
    array|string $object_id,
    int $timeout = -1,
    int $retries = -1
): array|false {
    return \snmp3_walk($hostname, $security_name, $security_level, $auth_protocol, $auth_passphrase, $privacy_protocol, $privacy_passphrase, $object_id, $timeout, $retries);
}

function snmprealwalk(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1): array|false
{
    return \snmp_walk($hostname, $community, $object_id, $timeout, $retries);
}

function snmpwalkoid(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1): array|false
{
    return \snmprealwalk($hostname, $community, $object_id, $timeout, $retries);
}

function __elephc_snmp3_get(
    string $hostname,
    string $security_name,
    string $security_level,
    string $auth_protocol,
    string $auth_passphrase,
    string $privacy_protocol,
    string $privacy_passphrase,
    array|string $object_id,
    int $pdu,
    int $timeout,
    int $retries
) {
    $session = __elephc_snmp3_open($hostname, $security_name, $security_level, $auth_protocol, $auth_passphrase, $privacy_protocol, $privacy_passphrase, $timeout, $retries);
    if ($session < 0) {
        return false;
    }
    $value = __elephc_snmp_session_get(
        $session,
        $object_id,
        $pdu,
        __ElephcSnmpConfig::$enum_print,
        __ElephcSnmpConfig::$valueretrieval,
        __ElephcSnmpConfig::$quick_print,
        __ElephcSnmpConfig::$numeric_timeticks
    );
    \elephc_snmp_close($session);
    return $value;
}

function __elephc_snmp3_open(
    string $hostname,
    string $security_name,
    string $security_level,
    string $auth_protocol,
    string $auth_passphrase,
    string $privacy_protocol,
    string $privacy_passphrase,
    int $timeout,
    int $retries
): int {
    $session = __elephc_snmp_open(SNMP_VERSION_3, $hostname, $security_name, $timeout, $retries);
    if ($session < 0) {
        return -1;
    }
    $level = \strtolower($security_level);
    if ($level === "noauthnopriv") {
        return $session;
    }
    if ($level !== "authnopriv" && $level !== "authpriv") {
        \elephc_snmp_close($session);
        throw new \ValueError("Security level must be one of \"noAuthNoPriv\", \"authNoPriv\", or \"authPriv\"");
    }
    if ($auth_protocol === "" || $auth_passphrase === "") {
        \elephc_snmp_close($session);
        return -1;
    }
    if (\elephc_snmp_set_auth($session, $auth_protocol, \strlen($auth_protocol), $auth_passphrase, \strlen($auth_passphrase)) !== 0) {
        \elephc_snmp_close($session);
        return -1;
    }
    if ($level === "authpriv") {
        if ($privacy_protocol === "" || $privacy_passphrase === "") {
            \elephc_snmp_close($session);
            return -1;
        }
        if (\elephc_snmp_set_priv($session, $privacy_protocol, \strlen($privacy_protocol), $privacy_passphrase, \strlen($privacy_passphrase)) !== 0) {
            \elephc_snmp_close($session);
            return -1;
        }
    }
    return $session;
}

function __elephc_snmp_text(ptr $pointer): string|false
{
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_snmp_string_len());
}

function __elephc_snmp_open(int $version, string $hostname, string $community, int $timeout, int $retries): int
{
    $timeout_us = $timeout < 0 ? 1000000 : $timeout;
    $timeout_ms = \intdiv($timeout_us, 1000);
    if ($timeout_ms < 1) {
        $timeout_ms = 1;
    }
    return \elephc_snmp_open($version, $hostname, \strlen($hostname), $community, \strlen($community), $timeout_ms, $retries);
}

function __elephc_snmp_value(
    int $response,
    int $index,
    string $oid = "",
    bool $enum_print = false,
    int $valueretrieval = 0,
    bool $quick_print = false,
    bool $numeric_timeticks = false
) {
    $type = \elephc_snmp_response_type($response, $index);
    $pointer = \elephc_snmp_response_value($response, $index);
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    $text = \__elephc_ptr_read_string($pointer, \elephc_snmp_string_len());
    $label = "";
    if ($type === SNMP_VALUE_INTEGER && $oid !== "") {
        $label_pointer = \elephc_snmp_enum_label($oid, \strlen($oid), (int) $text);
        if (!\__elephc_ptr_is_null($label_pointer)) {
            $label = \__elephc_ptr_read_string($label_pointer, \elephc_snmp_string_len());
        }
    }
    if ($enum_print) {
        $label = "";
    }
    $plain = null;
    if ($type === SNMP_VALUE_OCTET_STRING || $type === SNMP_VALUE_OPAQUE) {
        $binary = @\hex2bin($text);
        $plain = $binary === false ? $text : $binary;
    } elseif ($type === SNMP_VALUE_INTEGER || $type === SNMP_VALUE_COUNTER
        || $type === SNMP_VALUE_GAUGE || $type === SNMP_VALUE_TIMETICKS
        || $type === SNMP_VALUE_COUNTER64) {
        $plain = $text;
    } elseif ($type === SNMP_VALUE_OBJECT_ID || $type === SNMP_VALUE_IPADDRESS) {
        $plain = $text;
    }

    if (($valueretrieval & 2) !== 0) {
        $formatted = ($valueretrieval & 1) !== 0
            ? $plain
            : __elephc_snmp_library_value($type, $text, $label, $quick_print, $numeric_timeticks);
        return (object) ["type" => $type, "value" => $formatted];
    }
    if (($valueretrieval & 1) !== 0) {
        return $plain;
    }
    return __elephc_snmp_library_value($type, $text, $label, $quick_print, $numeric_timeticks);
}

function __elephc_snmp_library_value(
    int $type,
    string $text,
    string $label,
    bool $quick_print,
    bool $numeric_timeticks
) {
    if ($type === SNMP_VALUE_NOSUCHOBJECT) {
        return "No Such Object available on this agent at this OID";
    }
    if ($type === SNMP_VALUE_NOSUCHINSTANCE) {
        return "No Such Instance currently exists at this OID";
    }
    if ($type === SNMP_VALUE_ENDOFMIBVIEW) {
        return "No more variables left in this MIB View (It is past the end of the MIB tree)";
    }
    if ($type === SNMP_VALUE_NULL) {
        return "NULL";
    }
    if ($type === SNMP_VALUE_INTEGER) {
        $value = $text;
        if ($label !== "") {
            $value = $quick_print ? $label : $label . "(" . $text . ")";
        }
        return $quick_print ? $value : "INTEGER: " . $value;
    }
    if ($type === SNMP_VALUE_TIMETICKS) {
        if ($numeric_timeticks) {
            return $text;
        }
        $uptime = __elephc_snmp_uptime((int) $text, $quick_print);
        return $quick_print ? $uptime : "Timeticks: (" . $text . ") " . $uptime;
    }
    if ($type === SNMP_VALUE_OCTET_STRING || $type === SNMP_VALUE_OPAQUE) {
        $binary = @\hex2bin($text);
        $value = $binary === false ? $text : $binary;
        return $quick_print ? $value : "STRING: " . $value;
    }
    if ($type === SNMP_VALUE_OBJECT_ID) {
        return $quick_print ? $text : "OID: " . $text;
    }
    if ($type === SNMP_VALUE_IPADDRESS) {
        return $quick_print ? $text : "IpAddress: " . $text;
    }
    if ($type === SNMP_VALUE_COUNTER) {
        return $quick_print ? $text : "Counter32: " . $text;
    }
    if ($type === SNMP_VALUE_GAUGE) {
        return $quick_print ? $text : "Gauge32: " . $text;
    }
    if ($type === SNMP_VALUE_COUNTER64) {
        return $quick_print ? $text : "Counter64: " . $text;
    }
    return $text;
}

function __elephc_snmp_uptime(int $ticks, bool $quick_print): string
{
    $centisecs = $ticks % 100;
    $total_seconds = \intdiv($ticks, 100);
    $days = \intdiv($total_seconds, 86400);
    $remainder = $total_seconds % 86400;
    $hours = \intdiv($remainder, 3600);
    $remainder = $remainder % 3600;
    $minutes = \intdiv($remainder, 60);
    $seconds = $remainder % 60;
    if ($quick_print) {
        return \sprintf("%d:%d:%02d:%02d.%02d", $days, $hours, $minutes, $seconds, $centisecs);
    }
    if ($days === 0) {
        return \sprintf("%d:%02d:%02d.%02d", $hours, $minutes, $seconds, $centisecs);
    }
    if ($days === 1) {
        return \sprintf("1 day, %d:%02d:%02d.%02d", $hours, $minutes, $seconds, $centisecs);
    }
    return \sprintf("%d days, %d:%02d:%02d.%02d", $days, $hours, $minutes, $seconds, $centisecs);
}

function __elephc_snmp_session_get(
    int $session,
    array|string $object_id,
    int $pdu,
    bool $enum_print = false,
    int $valueretrieval = 0,
    bool $quick_print = false,
    bool $numeric_timeticks = false
) {
    if (\is_array($object_id)) {
        $values = [];
        foreach ($object_id as $oid) {
            $entry = __elephc_snmp_session_get($session, $oid, $pdu, $enum_print, $valueretrieval, $quick_print, $numeric_timeticks);
            if ($entry === false) {
                return false;
            }
            $values[] = $entry;
        }
        return $values;
    }
    $resolved = __elephc_snmp_resolve_oid((string) $object_id);
    $response = \elephc_snmp_request($session, $resolved, \strlen($resolved), $pdu);
    if ($response < 0) {
        return false;
    }
    if (\elephc_snmp_response_error_status($response) !== 0
        || \elephc_snmp_response_count($response) < 1) {
        \elephc_snmp_response_free($response);
        return false;
    }
    $value = __elephc_snmp_value($response, 0, $resolved, $enum_print, $valueretrieval, $quick_print, $numeric_timeticks);
    \elephc_snmp_response_free($response);
    return $value;
}

function __elephc_snmp_get(int $version, string $hostname, string $community, array|string $object_id, int $pdu, int $timeout, int $retries)
{
    $session = __elephc_snmp_open($version, $hostname, $community, $timeout, $retries);
    if ($session < 0) {
        return false;
    }
    $value = __elephc_snmp_session_get(
        $session,
        $object_id,
        $pdu,
        __ElephcSnmpConfig::$enum_print,
        __ElephcSnmpConfig::$valueretrieval,
        __ElephcSnmpConfig::$quick_print,
        __ElephcSnmpConfig::$numeric_timeticks
    );
    \elephc_snmp_close($session);
    return $value;
}

function __elephc_snmp_walk(int $version, string $hostname, string $community, array|string $object_id, int $timeout, int $retries): array|false
{
    $session = __elephc_snmp_open($version, $hostname, $community, $timeout, $retries);
    if ($session < 0) {
        return false;
    }
    if ($version === SNMP_VERSION_1) {
        $result = __elephc_snmp_session_walk(
            $session,
            $object_id,
            __ElephcSnmpConfig::$oid_output_format,
            __ElephcSnmpConfig::$enum_print,
            false,
            __ElephcSnmpConfig::$valueretrieval,
            __ElephcSnmpConfig::$quick_print,
            __ElephcSnmpConfig::$numeric_timeticks
        );
    } else {
        $result = __elephc_snmp_session_bulk_walk(
            $session,
            $object_id,
            20,
            0,
            __ElephcSnmpConfig::$oid_output_format,
            __ElephcSnmpConfig::$enum_print,
            false,
            __ElephcSnmpConfig::$valueretrieval,
            __ElephcSnmpConfig::$quick_print,
            __ElephcSnmpConfig::$numeric_timeticks
        );
    }
    \elephc_snmp_close($session);
    return $result;
}

function __elephc_snmp_session_bulk_walk(
    int $session,
    array|string $object_id,
    int $max_repetitions,
    int $non_repeaters,
    int $format = 0,
    bool $enum_print = false,
    bool $suffix_as_key = false,
    int $valueretrieval = 0,
    bool $quick_print = false,
    bool $numeric_timeticks = false
): array|false {
    $root = "." . \ltrim(__elephc_snmp_resolve_oid((string) $object_id), ".");
    $result = [];
    $seen = [];
    $current = $root;
    for ($iteration = 0; $iteration < 4096; $iteration++) {
        $response = \elephc_snmp_request_bulk($session, $current, \strlen($current), $non_repeaters, $max_repetitions);
        if ($response < 0) {
            break;
        }
        $status = \elephc_snmp_response_error_status($response);
        $count = \elephc_snmp_response_count($response);
        if ($status !== 0 || $count < 1) {
            \elephc_snmp_response_free($response);
            break;
        }
        $done = false;
        for ($index = 0; $index < $count; $index++) {
            $oid = __elephc_snmp_text(\elephc_snmp_response_oid($response, $index));
            if ($oid === false) {
                $done = true;
                break;
            }
            if (\strpos($oid, $root) !== 0 || \array_key_exists($oid, $seen)) {
                $done = true;
                break;
            }
            $seen[$oid] = true;
            $key = __elephc_snmp_walk_key($oid, $root, $format, $suffix_as_key);
            $result[$key] = __elephc_snmp_value($response, $index, $oid, $enum_print, $valueretrieval, $quick_print, $numeric_timeticks);
            $current = $oid;
        }
        \elephc_snmp_response_free($response);
        if ($done) {
            break;
        }
    }
    return $result;
}

function __elephc_snmp_session_set(int $session, array|string $object_id, array|string $type, array|string $value): bool
{
    if (\is_array($object_id)) {
        $types = \is_array($type) ? $type : [$type];
        $values = \is_array($value) ? $value : [$value];
        $index = 0;
        foreach ($object_id as $oid) {
            $entry_type = (string) ($types[$index] ?? ($types[0] ?? "s"));
            $entry_value = (string) ($values[$index] ?? ($values[0] ?? ""));
            if (!__elephc_snmp_session_set($session, $oid, $entry_type, $entry_value)) {
                return false;
            }
            $index++;
        }
        return true;
    }
    $oid_text = __elephc_snmp_resolve_oid((string) $object_id);
    $type_text = (string) $type;
    $value_text = (string) $value;
    $response = \elephc_snmp_request_set($session, $oid_text, \strlen($oid_text), $type_text, \strlen($type_text), $value_text, \strlen($value_text));
    if ($response < 0) {
        return false;
    }
    $status = \elephc_snmp_response_error_status($response);
    $count = \elephc_snmp_response_count($response);
    \elephc_snmp_response_free($response);
    return $status === 0 && $count > 0;
}

function __elephc_snmp_session_walk(
    int $session,
    array|string $object_id,
    int $format = 0,
    bool $enum_print = false,
    bool $suffix_as_key = false,
    int $valueretrieval = 0,
    bool $quick_print = false,
    bool $numeric_timeticks = false
): array|false {
    $root = "." . \ltrim(__elephc_snmp_resolve_oid((string) $object_id), ".");
    $result = [];
    $seen = [];
    $current = $root;
    for ($i = 0; $i < 4096; $i++) {
        $response = \elephc_snmp_request($session, $current, \strlen($current), 0xA1);
        if ($response < 0) {
            break;
        }
        if (\elephc_snmp_response_error_status($response) !== 0
            || \elephc_snmp_response_count($response) < 1) {
            \elephc_snmp_response_free($response);
            break;
        }
        $oid = __elephc_snmp_text(\elephc_snmp_response_oid($response, 0));
        if ($oid === false) {
            \elephc_snmp_response_free($response);
            break;
        }
        if (\strpos($oid, $root) !== 0 || \array_key_exists($oid, $seen)) {
            \elephc_snmp_response_free($response);
            break;
        }
        $seen[$oid] = true;
        $key = __elephc_snmp_walk_key($oid, $root, $format, $suffix_as_key);
        $result[$key] = __elephc_snmp_value($response, 0, $oid, $enum_print, $valueretrieval, $quick_print, $numeric_timeticks);
        $current = $oid;
    }
    return $result;
}

function __elephc_snmp_resolve_oid(string $oid): string
{
    $pointer = \elephc_snmp_resolve_oid($oid, \strlen($oid));
    if (\__elephc_ptr_is_null($pointer)) {
        return $oid;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_snmp_string_len());
}

function __elephc_snmp_render_oid(string $oid, int $format): string
{
    if ($format !== 1 && $format !== 2 && $format !== 3 && $format !== 5) {
        return $oid;
    }
    $pointer = \elephc_snmp_render_oid($oid, \strlen($oid), $format);
    if (\__elephc_ptr_is_null($pointer)) {
        return $oid;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_snmp_string_len());
}

function __elephc_snmp_walk_key(string $oid, string $root, int $format, bool $suffix_as_key): string
{
    if ($suffix_as_key) {
        return \substr($oid, \strlen($root));
    }
    return __elephc_snmp_render_oid($oid, $format);
}

final class __ElephcSnmpConfig
{
    public static bool $quick_print = false;
    public static bool $enum_print = false;
    public static bool $numeric_index = false;
    public static bool $numeric_timeticks = false;
    public static bool $extended_index = false;
    public static bool $dont_print_units = false;
    public static bool $escape_quotes = false;
    public static bool $print_hex_text = false;
    public static int $oid_output_format = 0;
    public static int $string_output_format = 0;
    public static int $valueretrieval = 0;
}

function snmp_init_mib(?string $mibdirs = null): void
{
    $dirs = $mibdirs ?? "";
    \elephc_snmp_init_mib($dirs, \strlen($dirs));
}

function snmp_read_mib(string $filename): bool
{
    return \elephc_snmp_read_mib($filename, \strlen($filename)) === 1;
}

function snmp_set_mib_option(\Snmp\Mib $option, bool $enable): void
{
}

function snmp_set_output_option(\Snmp\Output $option, bool $enable): void
{
    if ($option === \Snmp\Output::NumericIndex) {
        __ElephcSnmpConfig::$numeric_index = $enable;
    } elseif ($option === \Snmp\Output::EnumPrint) {
        __ElephcSnmpConfig::$enum_print = $enable;
    } elseif ($option === \Snmp\Output::EscapeQuotes) {
        __ElephcSnmpConfig::$escape_quotes = $enable;
    } elseif ($option === \Snmp\Output::QuickPrint) {
        __ElephcSnmpConfig::$quick_print = $enable;
    } elseif ($option === \Snmp\Output::NumericTimeticks) {
        __ElephcSnmpConfig::$numeric_timeticks = $enable;
    } elseif ($option === \Snmp\Output::HexText) {
        __ElephcSnmpConfig::$print_hex_text = $enable;
    } elseif ($option === \Snmp\Output::DontPrintUnits) {
        __ElephcSnmpConfig::$dont_print_units = $enable;
    } elseif ($option === \Snmp\Output::ExtendedIndex) {
        __ElephcSnmpConfig::$extended_index = $enable;
    }
}

function snmp_set_string_output_format(\Snmp\StringOutput $format): void
{
    __ElephcSnmpConfig::$string_output_format = __elephc_snmp_string_output_format($format);
}

function snmp_set_oid_output_format(\Snmp\OidOutput|int $format): true
{
    if (\is_int($format)) {
        __ElephcSnmpConfig::$oid_output_format = $format;
    } else {
        __ElephcSnmpConfig::$oid_output_format = __elephc_snmp_oid_output_format($format);
    }
    return true;
}

function snmp_set_oid_numeric_print(\Snmp\OidOutput|int $format): true
{
    return \snmp_set_oid_output_format($format);
}

function snmp_set_enum_print(bool $enable): true
{
    __ElephcSnmpConfig::$enum_print = $enable;
    return true;
}

function snmp_set_quick_print(bool $enable): true
{
    __ElephcSnmpConfig::$quick_print = $enable;
    return true;
}

function snmp_get_quick_print(): bool
{
    return __ElephcSnmpConfig::$quick_print;
}

function snmp_set_valueretrieval(int $method): true
{
    __ElephcSnmpConfig::$valueretrieval = $method;
    return true;
}

function snmp_get_valueretrieval(): int
{
    return __ElephcSnmpConfig::$valueretrieval;
}

function __elephc_snmp_oid_output_format(\Snmp\OidOutput $format): int
{
    if ($format === \Snmp\OidOutput::Suffix) {
        return 1;
    }
    if ($format === \Snmp\OidOutput::Module) {
        return 2;
    }
    if ($format === \Snmp\OidOutput::Full) {
        return 3;
    }
    if ($format === \Snmp\OidOutput::Numeric) {
        return 4;
    }
    if ($format === \Snmp\OidOutput::Ucd) {
        return 5;
    }
    return 6;
}

function __elephc_snmp_string_output_format(\Snmp\StringOutput $format): int
{
    if ($format === \Snmp\StringOutput::Ascii) {
        return 1;
    }
    if ($format === \Snmp\StringOutput::Hex) {
        return 2;
    }
    return 0;
}
