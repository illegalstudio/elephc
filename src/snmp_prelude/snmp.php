<?php

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
    public const int VERSION_3 = 3;

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

    public function __construct(int $version, string $hostname, string $community, int $timeout = -1, int $retries = -1)
    {
        if ($version !== self::VERSION_1 && $version !== self::VERSION_2C
            && $version !== self::VERSION_3) {
            throw new \SNMPException("Unsupported SNMP version");
        }
        $this->__version = $version;
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
        return 0;
    }

    public function getError(): string
    {
        return "";
    }

    public function get(array|string $object_id, bool $preserveKeys = false)
    {
        return __elephc_snmp_session_get($this->__session, $object_id, 0xA0);
    }

    public function getnext(array|string $object_id)
    {
        return __elephc_snmp_session_get($this->__session, $object_id, 0xA1);
    }

    public function walk(array|string $object_id, bool $suffixAsKey = false, int $maxRepetitions = -1, int $nonRepeaters = -1): array|false
    {
        return __elephc_snmp_session_walk($this->__session, $object_id);
    }

    public function set(array|string $object_id, array|string $type, array|string $value): bool
    {
        return false;
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
    $result = __elephc_snmp_session_walk($session, $object_id);
    \elephc_snmp_close($session);
    return $result;
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
    $value = __elephc_snmp_session_get($session, $object_id, $pdu);
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

function __elephc_snmp_value(int $response, int $index)
{
    $type = \elephc_snmp_response_type($response, $index);
    $pointer = \elephc_snmp_response_value($response, $index);
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    $text = \__elephc_ptr_read_string($pointer, \elephc_snmp_string_len());
    if ($type === SNMP_VALUE_OCTET_STRING || $type === SNMP_VALUE_OPAQUE) {
        $binary = @\hex2bin($text);
        return $binary === false ? $text : $binary;
    }
    if ($type === SNMP_VALUE_INTEGER || $type === SNMP_VALUE_COUNTER
        || $type === SNMP_VALUE_GAUGE || $type === SNMP_VALUE_TIMETICKS
        || $type === SNMP_VALUE_COUNTER64) {
        return (int) $text;
    }
    if ($type === SNMP_VALUE_OBJECT_ID || $type === SNMP_VALUE_IPADDRESS) {
        return $text;
    }
    return null;
}

function __elephc_snmp_session_get(int $session, array|string $object_id, int $pdu)
{
    if (\is_array($object_id)) {
        $values = [];
        foreach ($object_id as $oid) {
            $entry = __elephc_snmp_session_get($session, $oid, $pdu);
            if ($entry === false) {
                return false;
            }
            $values[] = $entry;
        }
        return $values;
    }
    $response = \elephc_snmp_request($session, $object_id, \strlen($object_id), $pdu);
    if ($response < 0) {
        return false;
    }
    if (\elephc_snmp_response_error_status($response) !== 0
        || \elephc_snmp_response_count($response) < 1) {
        \elephc_snmp_response_free($response);
        return false;
    }
    $value = __elephc_snmp_value($response, 0);
    \elephc_snmp_response_free($response);
    return $value;
}

function __elephc_snmp_get(int $version, string $hostname, string $community, array|string $object_id, int $pdu, int $timeout, int $retries)
{
    $session = __elephc_snmp_open($version, $hostname, $community, $timeout, $retries);
    if ($session < 0) {
        return false;
    }
    $value = __elephc_snmp_session_get($session, $object_id, $pdu);
    \elephc_snmp_close($session);
    return $value;
}

function __elephc_snmp_walk(int $version, string $hostname, string $community, array|string $object_id, int $timeout, int $retries): array|false
{
    $root = "." . \ltrim($object_id, ".");
    $session = __elephc_snmp_open($version, $hostname, $community, $timeout, $retries);
    if ($session < 0) {
        return false;
    }
    $result = [];
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
        if (\strpos($oid, $root) !== 0 || \array_key_exists($oid, $result)) {
            \elephc_snmp_response_free($response);
            break;
        }
        $value = __elephc_snmp_value($response, 0);
        \elephc_snmp_response_free($response);
        $result[$oid] = $value;
        $current = $oid;
    }
    \elephc_snmp_close($session);
    return $result;
}

function __elephc_snmp_session_walk(int $session, array|string $object_id): array|false
{
    $root = "." . \ltrim($object_id, ".");
    $result = [];
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
        if (\strpos($oid, $root) !== 0 || \array_key_exists($oid, $result)) {
            \elephc_snmp_response_free($response);
            break;
        }
        $value = __elephc_snmp_value($response, 0);
        \elephc_snmp_response_free($response);
        $result[$oid] = $value;
        $current = $oid;
    }
    return $result;
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
