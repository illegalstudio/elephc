<?php

const SOAP_1_1 = 1;
const SOAP_1_2 = 2;
const SOAP_RPC = 1;
const SOAP_DOCUMENT = 2;
const SOAP_ENCODED = 1;
const SOAP_LITERAL = 2;
const SOAP_ACTOR_NEXT = 1;
const SOAP_ACTOR_NONE = 2;
const SOAP_ACTOR_UNLIMATERECEIVER = 3;
const SOAP_PERSISTENCE_REQUEST = 2;
const SOAP_PERSISTENCE_SESSION = 1;
const SOAP_FUNCTIONS_ALL = 999;

const SOAP_COMPRESSION_ACCEPT = 32;
const SOAP_COMPRESSION_GZIP = 0;
const SOAP_COMPRESSION_DEFLATE = 16;
const SOAP_AUTHENTICATION_BASIC = 0;
const SOAP_AUTHENTICATION_DIGEST = 1;
const SOAP_SINGLE_ELEMENT_ARRAYS = 1;
const SOAP_WAIT_ONE_WAY_CALLS = 2;
const SOAP_USE_XSI_ARRAY_TYPE = 4;
const SOAP_SSL_METHOD_TLS = 0;
const SOAP_SSL_METHOD_SSLv2 = 1;
const SOAP_SSL_METHOD_SSLv3 = 2;
const SOAP_SSL_METHOD_SSLv23 = 3;
const SOAP_ENC_ARRAY = 300;
const SOAP_ENC_OBJECT = 301;
const UNKNOWN_TYPE = 999998;
const APACHE_MAP = 200;
const WSDL_CACHE_NONE = 0;
const WSDL_CACHE_DISK = 1;
const WSDL_CACHE_MEMORY = 2;
const WSDL_CACHE_BOTH = 3;
const XSD_1999_NAMESPACE = "http://www.w3.org/1999/XMLSchema";
const XSD_NAMESPACE = "http://www.w3.org/2001/XMLSchema";
const XSD_1999_TIMEINSTANT = 401;
const XSD_ANYTYPE = 145;
const XSD_ANYURI = 117;
const XSD_ANYXML = 147;
const XSD_BASE64BINARY = 116;
const XSD_BOOLEAN = 102;
const XSD_BYTE = 137;
const XSD_DATE = 109;
const XSD_DATETIME = 107;
const XSD_DECIMAL = 103;
const XSD_DOUBLE = 105;
const XSD_DURATION = 106;
const XSD_ENTITIES = 130;
const XSD_ENTITY = 129;
const XSD_FLOAT = 104;
const XSD_GDAY = 113;
const XSD_GMONTH = 114;
const XSD_GMONTHDAY = 112;
const XSD_GYEAR = 111;
const XSD_GYEARMONTH = 110;
const XSD_HEXBINARY = 115;
const XSD_ID = 126;
const XSD_IDREF = 127;
const XSD_IDREFS = 128;
const XSD_INT = 135;
const XSD_INTEGER = 131;
const XSD_LANGUAGE = 122;
const XSD_LONG = 134;
const XSD_NAME = 124;
const XSD_NCNAME = 125;
const XSD_NEGATIVEINTEGER = 133;
const XSD_NMTOKEN = 123;
const XSD_NMTOKENS = 144;
const XSD_NONNEGATIVEINTEGER = 138;
const XSD_NONPOSITIVEINTEGER = 132;
const XSD_NORMALIZEDSTRING = 120;
const XSD_NOTATION = 119;
const XSD_POSITIVEINTEGER = 143;
const XSD_QNAME = 118;
const XSD_SHORT = 136;
const XSD_STRING = 101;
const XSD_TIME = 108;
const XSD_TOKEN = 121;
const XSD_UNSIGNEDBYTE = 142;
const XSD_UNSIGNEDINT = 140;
const XSD_UNSIGNEDLONG = 139;
const XSD_UNSIGNEDSHORT = 141;

class SoapFault extends \Exception
{
    public string $faultcode = "";
    public string $faultstring = "";
    public ?string $faultactor = null;
    public mixed $detail = null;
    public ?string $faultname = null;
    public mixed $headerfault = null;

    public function __construct(string $code, string $string, ?string $actor = null, mixed $details = null, ?string $name = null, mixed $headerFault = null)
    {
        parent::__construct($string, 0);
        $this->faultcode = $code;
        $this->faultstring = $string;
        $this->faultactor = $actor;
        $this->detail = $details;
        $this->faultname = $name;
        $this->headerfault = $headerFault;
    }

    public function __toString(): string
    {
        return "SoapFault exception: [" . $this->faultcode . "] " . $this->faultstring;
    }
}

class SoapHeader
{
    public string $namespace;
    public string $name;
    public mixed $data;
    public bool $mustUnderstand;
    public ?string $actor;

    public function __construct(string $namespace, string $name, mixed $data = null, bool $mustUnderstand = false, ?string $actor = null)
    {
        $this->namespace = $namespace;
        $this->name = $name;
        $this->data = $data;
        $this->mustUnderstand = $mustUnderstand;
        $this->actor = $actor;
    }
}

class SoapVar
{
    public mixed $enc_value;
    public ?int $enc_type;

    public function __construct(mixed $data, ?int $encoding = null, ?string $typeName = null, ?string $typeNamespace = null, ?string $nodeName = null, ?string $nodeNamespace = null)
    {
        $this->enc_value = $data;
        $this->enc_type = $encoding;
        unset($typeName, $typeNamespace, $nodeName, $nodeNamespace);
    }
}

class SoapParam
{
    public mixed $param_data;
    public string $param_name;

    public function __construct(mixed $data, string $name)
    {
        $this->param_data = $data;
        $this->param_name = $name;
    }
}

class SoapClient
{
    private string $__location = "";
    private string $__uri = "";
    private int $__style = SOAP_RPC;
    private int $__use = SOAP_ENCODED;
    private int $__version = SOAP_1_1;
    private string $__login = "";
    private string $__password = "";
    private int $__timeout = 30;
    private bool $__trace = false;
    private bool $__exceptions = true;
    private string $__userAgent = "elephc-soap";
    private array $__keepHeaders = [];
    private string $__lastRequest = "";
    private string $__lastResponse = "";
    private string $__lastRequestHeaders = "";
    private string $__lastResponseHeaders = "";
    private array $__cookies = [];
    private array $__wsdlFunctions = [];
    private array $__wsdlTypes = [];
    private bool $__hasWsdl = false;
    private mixed $__headers = null;

    public function __construct(?string $wsdl, array $options = [])
    {
        $this->__location = (string) ($options["location"] ?? "");
        $this->__uri = (string) ($options["uri"] ?? "");
        if (isset($options["style"])) {
            $this->__style = (int) $options["style"];
        }
        if (isset($options["use"])) {
            $this->__use = (int) $options["use"];
        }
        if (isset($options["soap_version"])) {
            $this->__version = (int) $options["soap_version"] === SOAP_1_2 ? SOAP_1_2 : SOAP_1_1;
        }
        $this->__login = (string) ($options["login"] ?? "");
        $this->__password = (string) ($options["password"] ?? "");
        if (isset($options["connection_timeout"])) {
            $this->__timeout = (int) $options["connection_timeout"];
        }
        $this->__trace = (bool) ($options["trace"] ?? false);
        $this->__exceptions = (bool) ($options["exceptions"] ?? true);
        $this->__userAgent = (string) ($options["user_agent"] ?? "elephc-soap");
        $keep = $options["keep_headers"] ?? [];
        if (!\is_array($keep)) {
            throw new \SoapFault("Client", "keep_headers must be an array");
        }
        foreach ($keep as $header) {
            if (!\is_string($header)) {
                throw new \SoapFault("Client", "keep_headers must contain only strings");
            }
            $this->__keepHeaders[] = \strtolower($header);
        }
        if ($wsdl !== null && $wsdl !== "") {
            $headers = __elephc_soap_headers_from_context($options["stream_context"] ?? null);
            if ($this->__login !== "") {
                $headers[] = "Authorization: Basic " . \base64_encode($this->__login . ":" . $this->__password);
            }
            $headers[] = "User-Agent: " . $this->__userAgent;
            $scan = __elephc_soap_fetch_wsdl($wsdl, $headers, $this->__keepHeaders, $wsdl, $this->__timeout, 0);
            $this->__hasWsdl = true;
            $metadata = __elephc_soap_wsdl_metadata((string) ($scan["xml"] ?? ""));
            $this->__wsdlFunctions = $metadata["functions"];
            $this->__wsdlTypes = $metadata["types"];
            if ($this->__location === "") {
                $this->__location = (string) ($scan["address"] ?? "");
            }
            if ($this->__uri === "") {
                $this->__uri = (string) ($scan["target"] ?? "");
            }
        }
        if ($this->__location === "") {
            throw new \SoapFault("Client", "SoapClient requires a location in WSDL-less mode");
        }
        if ($this->__uri === "") {
            throw new \SoapFault("Client", "SoapClient requires a uri in WSDL-less mode");
        }
    }

    public function __soapCall(string $function, array $args, ?array $options = null, mixed $inputHeaders = null, mixed &$outputHeaders = null)
    {
        unset($inputHeaders);
        $location = $this->__location;
        if ($options !== null && isset($options["location"])) {
            $location = (string) $options["location"];
        }
        $uri = $this->__uri;
        if ($options !== null && isset($options["uri"])) {
            $uri = (string) $options["uri"];
        }
        $soapAction = $uri . "#" . $function;
        if ($options !== null && isset($options["soapaction"])) {
            $soapAction = (string) $options["soapaction"];
        }
        $params = "";
        if (\count($args) === 1 && \is_array($args[0])) {
            foreach ($args[0] as $name => $value) {
                $params .= __elephc_soap_value((string) $name, $value, $this->__use);
            }
        } else {
            foreach ($args as $index => $value) {
                $params .= __elephc_soap_value("param" . $index, $value, $this->__use);
            }
        }
        $envelope = $this->__version === SOAP_1_2
            ? "http://www.w3.org/2003/05/soap-envelope"
            : "http://schemas.xmlsoap.org/soap/envelope/";
        $encoded = $this->__use === SOAP_ENCODED;
        $encoding = $this->__version === SOAP_1_2
            ? "http://www.w3.org/2003/05/soap-encoding"
            : "http://schemas.xmlsoap.org/soap/encoding/";
        $request = '<?xml version="1.0" encoding="UTF-8"?>'
            . '<SOAP-ENV:Envelope xmlns:SOAP-ENV="' . $envelope . '"'
            . ' xmlns:ns1="' . __elephc_soap_escape($uri) . '"'
            . ($encoded
                ? ' xmlns:xsd="http://www.w3.org/2001/XMLSchema"'
                    . ' xmlns:SOAP-ENC="' . $encoding . '"'
                    . ' xmlns:ns2="http://xml.apache.org/xml-soap"'
                    . ' SOAP-ENV:encodingStyle="' . $encoding . '"'
                : '')
            . ' xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">'
            . '<SOAP-ENV:Body><ns1:' . __elephc_soap_name($function) . '>'
            . $params
            . '</ns1:' . __elephc_soap_name($function) . '></SOAP-ENV:Body></SOAP-ENV:Envelope>';
        $response = $this->__doRequest($request, $location, $soapAction, $this->__version);
        if ($response === null) {
            return null;
        }
        return __elephc_soap_response($response, $this->__version);
    }

    public function __call(string $name, array $arguments)
    {
        return $this->__soapCall($name, $arguments);
    }

    public function __getFunctions(): ?array
    {
        return $this->__hasWsdl ? $this->__wsdlFunctions : null;
    }

    public function __getTypes(): ?array
    {
        return $this->__hasWsdl ? $this->__wsdlTypes : null;
    }

    public function __getLastRequest(): ?string
    {
        return $this->__trace ? $this->__lastRequest : null;
    }

    public function __getLastResponse(): ?string
    {
        return $this->__trace ? $this->__lastResponse : null;
    }

    public function __setLocation(?string $location = null): ?string
    {
        $old = $this->__location;
        if ($location !== null) {
            $this->__location = $location;
        }
        return $old;
    }

    public function __setSoapHeaders(mixed $headers = null): bool
    {
        $this->__headers = $headers;
        return true;
    }

    public function __doRequest(string $request, string $location, string $action, int $version, bool $oneWay = false, ?string $uriParserClass = null): ?string
    {
        $headers = [];
        if ($version === SOAP_1_2) {
            $headers[] = "Content-Type: application/soap+xml; charset=utf-8; action=\"" . $action . "\"";
        } else {
            $headers[] = "Content-Type: text/xml; charset=utf-8";
            $headers[] = "SOAPAction: \"" . $action . "\"";
        }
        $headers[] = "User-Agent: " . $this->__userAgent;
        if ($this->__login !== "") {
            $headers[] = "Authorization: Basic " . \base64_encode($this->__login . ":" . $this->__password);
        }
        if ($this->__cookies !== []) {
            $pairs = [];
            foreach ($this->__cookies as $name => $value) {
                $pairs[] = $name . "=" . $value;
            }
            $headers[] = "Cookie: " . \implode("; ", $pairs);
        }
        $exchange = __elephc_soap_http_exchange("POST", $location, $request, $headers, $this->__timeout);
        $this->__lastRequest = $request;
        $this->__lastResponse = (string) $exchange["body"];
        $this->__lastRequestHeaders = (string) $exchange["request_headers"];
        $this->__lastResponseHeaders = (string) $exchange["response_headers"];
        $this->__storeCookies((string) $exchange["response_headers"]);
        if ($oneWay) {
            return null;
        }
        return (string) $exchange["body"];
    }

    private function __storeCookies(string $headers): void
    {
        foreach (\explode("\r\n", $headers) as $line) {
            $colon = \strpos($line, ":");
            if ($colon === false) {
                continue;
            }
            if (\strtolower(\substr($line, 0, $colon)) !== "set-cookie") {
                continue;
            }
            $cookie = \trim(\substr($line, $colon + 1));
            $semicolon = \strpos($cookie, ";");
            if ($semicolon !== false) {
                $cookie = \substr($cookie, 0, $semicolon);
            }
            $equals = \strpos($cookie, "=");
            if ($equals === false) {
                continue;
            }
            $name = \trim(\substr($cookie, 0, $equals));
            $value = \trim(\substr($cookie, $equals + 1));
            if ($name !== "") {
                $this->__cookies[$name] = $value;
            }
        }
    }

    public function __getLastRequestHeaders(): ?string
    {
        return $this->__trace ? $this->__lastRequestHeaders : null;
    }

    public function __getLastResponseHeaders(): ?string
    {
        return $this->__trace ? $this->__lastResponseHeaders : null;
    }

    public function __getCookies(): array
    {
        return $this->__cookies;
    }

    public function __setCookie(string $name, ?string $value = null): void
    {
        if ($value === null) {
            unset($this->__cookies[$name]);
        } else {
            $this->__cookies[$name] = $value;
        }
    }
}


class SoapServer
{
    private string $__uri = "";
    private int $__version = SOAP_1_1;
    private mixed $__object = null;
    private array $__functions = [];
    private bool $__allFunctions = false;
    private array $__headers = [];
    private ?SoapFault $__fault = null;
    private int $__persistence = SOAP_PERSISTENCE_REQUEST;

    public function __construct(?string $wsdl, array $options = [])
    {
        $this->__uri = (string) ($options["uri"] ?? "");
        if (isset($options["soap_version"])) {
            $this->__version = (int) $options["soap_version"] === SOAP_1_2 ? SOAP_1_2 : SOAP_1_1;
        }
        if ($wsdl !== null && $wsdl !== "") {
            $xml = @\file_get_contents($wsdl);
            if (\is_string($xml) && $xml !== "") {
                $scan = __elephc_soap_scan_wsdl($xml);
                if ($this->__uri === "") {
                    $this->__uri = (string) ($scan["target"] ?? "");
                }
            }
        }
    }

    public function setClass(string $class, mixed ...$args): void
    {
        $this->__object = new $class(...$args);
    }

    public function setObject(object $object): void
    {
        $this->__object = $object;
    }

    public function addFunction(mixed $functions): void
    {
        if ($functions === SOAP_FUNCTIONS_ALL) {
            $this->__allFunctions = true;
            return;
        }
        if (\is_array($functions)) {
            foreach ($functions as $function) {
                $this->addFunction($function);
            }
            return;
        }
        if (!\is_string($functions)) {
            throw new \SoapFault("Client", "Invalid function name");
        }
        $this->__functions[] = $functions;
    }

    public function getFunctions(): array
    {
        return $this->__functions;
    }

    public function setPersistence(int $mode): void
    {
        $this->__persistence = $mode;
    }

    public function addSoapHeader(SoapHeader $header): void
    {
        $this->__headers[] = $header;
    }

    public function fault(string $code, string $string, ?string $actor = null, mixed $details = null, ?string $name = null): void
    {
        $this->__fault = new \SoapFault($code, $string, $actor, $details, $name);
    }

    public function handle(?string $request = null): void
    {
        if ($this->__fault !== null) {
            $this->__emitFault($this->__fault);
            return;
        }
        $payload = "";
        if ($request !== null) {
            $payload = $request;
        } else {
            $loaded = @\file_get_contents("php://input");
            if (\is_string($loaded)) {
                $payload = $loaded;
            }
        }
        $root = __elephc_soap_element((string) $payload, 0);
        if ($root[0] === "" || !\is_array($root[1]) || !\array_key_exists("Body", $root[1])) {
            $this->__emitFault(new \SoapFault("Client", "Invalid request"));
            return;
        }
        $body = $root[1]["Body"];
        if (!\is_array($body)) {
            $this->__emitFault(new \SoapFault("Client", "Invalid request"));
            return;
        }
        $operation = null;
        foreach ($body as $childName => $childValue) {
            if ($childName === "__text") {
                continue;
            }
            $operation = [(string) $childName, $childValue];
            break;
        }
        if ($operation === null) {
            $this->__emitFault(new \SoapFault("Client", "Invalid request"));
            return;
        }
        $name = (string) $operation[0];
        $params = [];
        if (\is_array($operation[1])) {
            foreach ($operation[1] as $parameterName => $parameterValue) {
                if ((string) $parameterName !== "__text") {
                    $params[(string) $parameterName] = $parameterValue;
                }
            }
        }
        try {
            $result = null;
            $resolved = "";
            if ($this->__object !== null) {
                $object = $this->__object;
                $methods = \get_class_methods(\get_class($object));
                foreach ($methods as $method) {
                    if (\strcasecmp($method, $name) === 0) {
                        $resolved = $method;
                        break;
                    }
                }
                if ($resolved === "") {
                    throw new \SoapFault("Server", "Function ('" . $name . "') is not a valid method for this service");
                }
                $result = \call_user_func_array([$object, $resolved], $params);
            } elseif ($this->__allFunctions) {
                $result = \call_user_func_array($name, $params);
            } else {
                foreach ($this->__functions as $function) {
                    if (\strcasecmp($function, $name) === 0) {
                        $resolved = $function;
                        break;
                    }
                }
                if ($resolved === "") {
                    throw new \SoapFault("Server", "Function ('" . $name . "') is not a valid method for this service");
                }
                $result = \call_user_func_array($resolved, $params);
            }
            if ($result instanceof \SoapFault) {
                $this->__emitFault($result);
                return;
            }
            $this->__emitResponse($name, $result);
        } catch (\SoapFault $fault) {
            $this->__emitFault($fault);
        }
    }

    private function __emitResponse(string $name, mixed $result): void
    {
        $tag = __elephc_soap_name($name) . "Response";
        $namespace = $this->__uri === "" ? "" : ' xmlns:ns1="' . __elephc_soap_escape($this->__uri) . '"';
        $body = "";
        if ($result !== null) {
            $body = __elephc_soap_value("return", $result);
        }
        $headerXml = "";
        foreach ($this->__headers as $header) {
            $headerName = __elephc_soap_name($header->name);
            $headerXml .= '<ns2:' . $headerName . ' xmlns:ns2="' . __elephc_soap_escape($header->namespace) . '">'
                . __elephc_soap_xml_value($header->data) . '</ns2:' . $headerName . '>';
        }
        $headerBlock = $headerXml === "" ? "" : '<SOAP-ENV:Header>' . $headerXml . '</SOAP-ENV:Header>';
        $envelope = $this->__version === SOAP_1_2
            ? "http://www.w3.org/2003/05/soap-envelope"
            : "http://schemas.xmlsoap.org/soap/envelope/";
        $xml = '<?xml version="1.0" encoding="UTF-8"?>'
            . '<SOAP-ENV:Envelope xmlns:SOAP-ENV="' . $envelope . '"'
            . ' xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">'
            . $headerBlock
            . '<SOAP-ENV:Body><ns1:' . $tag . $namespace . '>'
            . $body
            . '</ns1:' . $tag . '></SOAP-ENV:Body></SOAP-ENV:Envelope>';
        $this->__sendHeaders($xml);
        echo $xml;
    }

    private function __emitFault(\SoapFault $fault): void
    {
        $envelope = $this->__version === SOAP_1_2
            ? "http://www.w3.org/2003/05/soap-envelope"
            : "http://schemas.xmlsoap.org/soap/envelope/";
        if ($this->__version === SOAP_1_2) {
            $faultXml = '<SOAP-ENV:Fault><SOAP-ENV:Code><SOAP-ENV:Value>'
                . __elephc_soap_escape($fault->faultcode)
                . '</SOAP-ENV:Value></SOAP-ENV:Code><SOAP-ENV:Reason><SOAP-ENV:Text xml:lang="en">'
                . __elephc_soap_escape($fault->faultstring)
                . '</SOAP-ENV:Text></SOAP-ENV:Reason></SOAP-ENV:Fault>';
        } else {
            $faultXml = '<SOAP-ENV:Fault><faultcode>' . __elephc_soap_escape($fault->faultcode)
                . '</faultcode><faultstring>' . __elephc_soap_escape($fault->faultstring)
                . '</faultstring></SOAP-ENV:Fault>';
        }
        $xml = '<?xml version="1.0" encoding="UTF-8"?>'
            . '<SOAP-ENV:Envelope xmlns:SOAP-ENV="' . $envelope . '">'
            . '<SOAP-ENV:Body>' . $faultXml . '</SOAP-ENV:Body></SOAP-ENV:Envelope>';
        @\header("HTTP/1.1 500 Internal Server Error");
        $this->__sendHeaders($xml);
        echo $xml;
    }

    private function __sendHeaders(string $xml): void
    {
        if ($this->__version === SOAP_1_2) {
            @\header("Content-Type: application/soap+xml; charset=utf-8");
        } else {
            @\header("Content-Type: text/xml; charset=utf-8");
        }
        @\header("Content-Length: " . \strlen($xml));
    }
}

function is_soap_fault(mixed $object): bool
{
    return $object instanceof \SoapFault;
}

function use_soap_error_handler(bool $enable = true): bool
{
    unset($enable);
    return false;
}

function __elephc_soap_escape(string $value): string
{
    return \htmlspecialchars($value, ENT_QUOTES, "UTF-8");
}

function __elephc_soap_name(string $name): string
{
    $clean = "";
    $length = \strlen($name);
    for ($i = 0; $i < $length; $i++) {
        $char = $name[$i];
        if (($char >= "a" && $char <= "z") || ($char >= "A" && $char <= "Z")
            || ($char >= "0" && $char <= "9") || $char === "_" || $char === "-" || $char === ".") {
            $clean .= $char;
        }
    }
    return $clean === "" ? "item" : $clean;
}

function __elephc_soap_value(string $name, mixed $value, int $use = SOAP_LITERAL): string
{
    $tag = __elephc_soap_name($name);
    if ($use === SOAP_ENCODED) {
        return __elephc_soap_encoded_value($tag, $value);
    }
    if ($value === null) {
        return "<ns1:" . $tag . " xsi:nil=\"true\"/>";
    }
    if (\is_array($value)) {
        $body = "";
        foreach ($value as $key => $item) {
            $body .= __elephc_soap_value((string) $key, $item);
        }
        return "<ns1:" . $tag . ">" . $body . "</ns1:" . $tag . ">";
    }
    if (\is_object($value)) {
        return __elephc_soap_value($tag, \get_object_vars($value));
    }
    if (\is_bool($value)) {
        $text = $value ? "true" : "false";
    } else {
        $text = (string) $value;
    }
    return "<ns1:" . $tag . ">" . __elephc_soap_escape($text) . "</ns1:" . $tag . ">";
}

function __elephc_soap_encoded_value(string $tag, mixed $value): string
{
    if ($value === null) {
        return "<ns1:" . $tag . " xsi:nil=\"true\"/>";
    }
    if (\is_array($value)) {
        return __elephc_soap_encoded_array($tag, $value);
    }
    if (\is_object($value)) {
        return __elephc_soap_encoded_array($tag, \get_object_vars($value));
    }
    return __elephc_soap_encoded_scalar($tag, $value);
}

function __elephc_soap_encoded_type(mixed $value): string
{
    if (\is_bool($value)) {
        return "xsd:boolean";
    }
    if (\is_int($value)) {
        return "xsd:int";
    }
    if (\is_float($value)) {
        return "xsd:float";
    }
    if (\is_string($value)) {
        return "xsd:string";
    }
    return "xsd:anyType";
}

function __elephc_soap_encoded_scalar(string $tag, mixed $value): string
{
    $type = __elephc_soap_encoded_type($value);
    if (\is_bool($value)) {
        $text = $value ? "true" : "false";
    } else {
        $text = (string) $value;
    }
    return "<" . $tag . " xsi:type=\"" . $type . "\">" . __elephc_soap_escape($text) . "</" . $tag . ">";
}

function __elephc_soap_encoded_array(string $tag, mixed $value): string
{
    if (!\is_array($value)) {
        return __elephc_soap_encoded_scalar($tag, $value);
    }
    $items = (array) $value;
    $list = true;
    $index = 0;
    foreach ($items as $key => $item) {
        if ($key !== $index) {
            $list = false;
            break;
        }
        $index++;
    }
    if ($list) {
        $count = \count($items);
        $item_type = "xsd:anyType";
        if ($count > 0) {
            $first = true;
            foreach ($items as $item) {
                $type = __elephc_soap_encoded_type($item);
                if ($first) {
                    $item_type = $type;
                    $first = false;
                } elseif ($type !== $item_type) {
                    $item_type = "xsd:anyType";
                    break;
                }
            }
        }
        $body = "";
        foreach ($items as $item) {
            if (\is_array($item)) {
                $body .= __elephc_soap_encoded_array("item", (array) $item);
            } elseif (\is_object($item)) {
                $body .= __elephc_soap_encoded_array("item", \get_object_vars($item));
            } else {
                $body .= __elephc_soap_encoded_scalar("item", $item);
            }
        }
        return "<ns1:" . $tag . " SOAP-ENC:arrayType=\"" . $item_type . "[" . $count . "]\" xsi:type=\"SOAP-ENC:Array\">"
            . $body . "</ns1:" . $tag . ">";
    }
    $body = "";
    foreach ($items as $key => $item) {
        $key_xml = __elephc_soap_encoded_scalar("key", (string) $key);
        if (\is_array($item)) {
            $value_xml = __elephc_soap_encoded_array("value", (array) $item);
        } elseif (\is_object($item)) {
            $value_xml = __elephc_soap_encoded_array("value", \get_object_vars($item));
        } else {
            $value_xml = __elephc_soap_encoded_scalar("value", $item);
        }
        $body .= "<item>" . $key_xml . $value_xml . "</item>";
    }
    return "<ns1:" . $tag . " xsi:type=\"ns2:Map\">" . $body . "</ns1:" . $tag . ">";
}

function __elephc_soap_xml_value(mixed $value): string
{
    if ($value === null) {
        return "";
    }
    if (\is_array($value)) {
        $body = "";
        foreach ($value as $key => $item) {
            $body .= __elephc_soap_value((string) $key, $item);
        }
        return $body;
    }
    if (\is_object($value)) {
        return __elephc_soap_xml_value(\get_object_vars($value));
    }
    if (\is_bool($value)) {
        return $value ? "true" : "false";
    }
    return __elephc_soap_escape((string) $value);
}

function __elephc_soap_local(string $name): string
{
    $colon = \strrpos($name, ":");
    return $colon === false ? $name : \substr($name, $colon + 1);
}

function __elephc_soap_decode(string $text): string
{
    $text = \str_replace("&lt;", "<", $text);
    $text = \str_replace("&gt;", ">", $text);
    $text = \str_replace("&quot;", "\"", $text);
    $text = \str_replace("&apos;", "'", $text);
    return \str_replace("&amp;", "&", $text);
}

function __elephc_soap_element(string $xml, int $position): array
{
    $length = \strlen($xml);
    while ($position < $length) {
        if (\substr($xml, $position, 4) === "<!--") {
            $comment_end = \strpos($xml, "-->", $position);
            if ($comment_end === false) {
                $position = $length;
            } else {
                $position = $comment_end + 3;
            }
            continue;
        }
        if (\substr($xml, $position, 2) === "<?") {
            $declaration_end = \strpos($xml, "?>", $position);
            if ($declaration_end === false) {
                $position = $length;
            } else {
                $position = $declaration_end + 2;
            }
            continue;
        }
        if (\substr($xml, $position, 1) === "<") {
            break;
        }
        $position++;
    }
    if ($position >= $length || \substr($xml, $position, 1) !== "<") {
        return ["", "", $position];
    }
    $start = $position + 1;
    $end = $start;
    while ($end < $length) {
        $char = \substr($xml, $end, 1);
        if ($char === ">" || $char === " " || $char === "\t" || $char === "\r" || $char === "\n") {
            break;
        }
        $end++;
    }
    $name = \substr($xml, $start, $end - $start);
    $self_closing = false;
    while ($end < $length) {
        $char = \substr($xml, $end, 1);
        if ($char === "/" && \substr($xml, $end + 1, 1) === ">") {
            $self_closing = true;
            break;
        }
        if ($char === ">") {
            break;
        }
        $end++;
    }
    $position = $end + ($self_closing ? 2 : 1);
    if ($self_closing) {
        return [$name, "", $position];
    }
    $children = [];
    $text = "";
    while ($position < $length) {
        if (\substr($xml, $position, 2) === "</") {
            $close_end = \strpos($xml, ">", $position);
            if ($close_end === false) {
                $position = $length;
            } else {
                $position = $close_end + 1;
            }
            break;
        }
        if (\substr($xml, $position, 9) === "<![CDATA[") {
            $cdata_end = \strpos($xml, "]]>", $position);
            if ($cdata_end === false) {
                $text .= \substr($xml, $position + 9);
                $position = $length;
            } else {
                $text .= \substr($xml, $position + 9, $cdata_end - $position - 9);
                $position = $cdata_end + 3;
            }
            continue;
        }
        if (\substr($xml, $position, 1) === "<") {
            $child = __elephc_soap_element($xml, $position);
            $position = (int) $child[2];
            if ($child[0] !== "") {
                $local = __elephc_soap_local((string) $child[0]);
                if (\array_key_exists($local, $children)) {
                    if (!\is_array($children[$local]) || !\array_key_exists(0, $children[$local])) {
                        $children[$local] = [$children[$local]];
                    }
                    $children[$local][] = $child[1];
                } else {
                    $children[$local] = $child[1];
                }
            }
            continue;
        }
        $next = \strpos($xml, "<", $position);
        if ($next === false) {
            $text .= \substr($xml, $position);
            $position = $length;
        } else {
            $text .= \substr($xml, $position, $next - $position);
            $position = $next;
        }
    }
    $text = __elephc_soap_decode($text);
    if ($children !== []) {
        if (\trim($text) !== "") {
            $children["__text"] = $text;
        }
        return [$name, $children, $position];
    }
    return [$name, $text, $position];
}

function __elephc_soap_same_origin(string $left, string $right): bool
{
    $a = \parse_url($left);
    $b = \parse_url($right);
    if (!\is_array($a) || !\is_array($b)) {
        return false;
    }
    $schemeA = \strtolower((string) ($a["scheme"] ?? "http"));
    $schemeB = \strtolower((string) ($b["scheme"] ?? "http"));
    $portA = (int) ($a["port"] ?? ($schemeA === "https" ? 443 : 80));
    $portB = (int) ($b["port"] ?? ($schemeB === "https" ? 443 : 80));
    return $schemeA === $schemeB
        && \strcasecmp((string) ($a["host"] ?? ""), (string) ($b["host"] ?? "")) === 0
        && $portA === $portB;
}

function __elephc_soap_header_name(string $header): string
{
    $colon = \strpos($header, ":");
    return $colon === false ? "" : \strtolower(\trim(\substr($header, 0, $colon)));
}

function __elephc_soap_headers_from_context(mixed $context): array
{
    if (!\is_resource($context)) {
        return [];
    }
    $options = @\stream_context_get_options($context);
    if (!\is_array($options) || !isset($options["http"])) {
        return [];
    }
    $http = $options["http"];
    if (!\is_array($http)) {
        return [];
    }
    $header = $http["header"] ?? null;
    $lines = [];
    if (\is_array($header)) {
        $lines = $header;
    } elseif (\is_string($header)) {
        $lines = \explode("\r\n", $header);
        if (\count($lines) === 1) {
            $lines = \explode("\n", $header);
        }
    }
    $headers = [];
    foreach ($lines as $line) {
        $line = \trim((string) $line);
        if ($line !== "") {
            $headers[] = $line;
        }
    }
    return $headers;
}

function __elephc_soap_filter_headers(array $headers, array $keep, bool $sameOrigin): array
{
    if ($sameOrigin) {
        return $headers;
    }
    $filtered = [];
    foreach ($headers as $header) {
        $name = __elephc_soap_header_name($header);
        if ($name === "authorization" || $name === "proxy-authorization" || $name === "cookie") {
            if (!\in_array($name, $keep, true)) {
                continue;
            }
        }
        $filtered[] = $header;
    }
    return $filtered;
}

function __elephc_soap_resolve_url(string $base, string $relative): string
{
    if (\strpos($relative, "http://") === 0 || \strpos($relative, "https://") === 0) {
        return $relative;
    }
    $parts = \parse_url($base);
    if (!\is_array($parts) || !isset($parts["host"])) {
        return $relative;
    }
    $scheme = (string) ($parts["scheme"] ?? "http");
    $host = (string) $parts["host"];
    $port = isset($parts["port"]) ? ":" . (string) $parts["port"] : "";
    if (\substr($relative, 0, 1) === "/") {
        return $scheme . "://" . $host . $port . $relative;
    }
    $path = (string) ($parts["path"] ?? "/");
    $slash = \strrpos($path, "/");
    $directory = $slash === false ? "/" : \substr($path, 0, $slash + 1);
    return $scheme . "://" . $host . $port . $directory . $relative;
}

function __elephc_soap_attributes(string $tag): array
{
    $attributes = [];
    $length = \strlen($tag);
    $i = 0;
    while ($i < $length) {
        while ($i < $length) {
            $char = \substr($tag, $i, 1);
            if ($char !== " " && $char !== "\t" && $char !== "\r" && $char !== "\n") {
                break;
            }
            $i++;
        }
        $name_start = $i;
        while ($i < $length) {
            $char = \substr($tag, $i, 1);
            if ($char === "=" || $char === " " || $char === "\t" || $char === "\r" || $char === "\n" || $char === "/" || $char === ">") {
                break;
            }
            $i++;
        }
        if ($i === $name_start) {
            $i++;
            continue;
        }
        $name = \substr($tag, $name_start, $i - $name_start);
        while ($i < $length && \substr($tag, $i, 1) === " ") {
            $i++;
        }
        if ($i >= $length || \substr($tag, $i, 1) !== "=") {
            continue;
        }
        $i++;
        while ($i < $length) {
            $char = \substr($tag, $i, 1);
            if ($char !== " " && $char !== "\t" && $char !== "\r" && $char !== "\n") {
                break;
            }
            $i++;
        }
        if ($i >= $length) {
            break;
        }
        $quote = \substr($tag, $i, 1);
        if ($quote !== "\"" && $quote !== "'") {
            continue;
        }
        $i++;
        $value_end = \strpos($tag, $quote, $i);
        if ($value_end === false) {
            break;
        }
        $attributes[$name] = \substr($tag, $i, $value_end - $i);
        $i = $value_end + 1;
    }
    return $attributes;
}

function __elephc_soap_scan_wsdl(string $xml): array
{
    $imports = [];
    $address = "";
    $target = "";
    $position = 0;
    $length = \strlen($xml);
    while ($position < $length) {
        $start = \strpos($xml, "<", $position);
        if ($start === false) {
            break;
        }
        if (\substr($xml, $start, 4) === "<!--") {
            $comment_end = \strpos($xml, "-->", $start);
            $position = $comment_end === false ? $length : $comment_end + 3;
            continue;
        }
        if (\substr($xml, $start, 2) === "<?") {
            $declaration_end = \strpos($xml, "?>", $start);
            $position = $declaration_end === false ? $length : $declaration_end + 2;
            continue;
        }
        if (\substr($xml, $start, 2) === "</") {
            $position = $start + 2;
            continue;
        }
        $tag_end = \strpos($xml, ">", $start);
        if ($tag_end === false) {
            break;
        }
        $tag = \substr($xml, $start + 1, $tag_end - $start - 1);
        if (\substr($tag, -1) === "/") {
            $tag = \substr($tag, 0, -1);
        }
        $space = \strpos($tag, " ");
        $name = $space === false ? $tag : \substr($tag, 0, $space);
        $local = __elephc_soap_local($name);
        $attributes = __elephc_soap_attributes($tag);
        if ($local === "import" || $local === "include") {
            if (isset($attributes["location"])) {
                $imports[] = $attributes["location"];
            }
            if (isset($attributes["schemaLocation"])) {
                $imports[] = $attributes["schemaLocation"];
            }
        }
        if ($local === "address" && $address === "" && isset($attributes["location"])) {
            $address = $attributes["location"];
        }
        if ($local === "definitions" && $target === "" && isset($attributes["targetNamespace"])) {
            $target = $attributes["targetNamespace"];
        }
        $position = $tag_end + 1;
    }
    return ["imports" => $imports, "address" => $address, "target" => $target];
}

function __elephc_soap_fetch_wsdl(string $url, array $headers, array $keep, string $origin, int $timeout, int $depth): array
{
    if ($depth > 5) {
        return ["imports" => [], "address" => "", "target" => "", "xml" => ""];
    }
    $same = __elephc_soap_same_origin($url, $origin);
    $send = __elephc_soap_filter_headers($headers, $keep, $same);
    if (\strpos($url, "http://") !== 0 && \strpos($url, "https://") !== 0) {
        $loaded = @\file_get_contents($url);
        if (!\is_string($loaded)) {
            throw new \SoapFault("HTTP", "Could not read WSDL");
        }
        $xml = $loaded;
    } else {
        $xml = __elephc_soap_http_request("GET", $url, "", $send, $timeout);
    }
    $scan = __elephc_soap_scan_wsdl($xml);
    $address = (string) ($scan["address"] ?? "");
    $target = (string) ($scan["target"] ?? "");
    foreach ($scan["imports"] as $import) {
        $resolved = __elephc_soap_resolve_url($url, (string) $import);
        $child = __elephc_soap_fetch_wsdl($resolved, $headers, $keep, $origin, $timeout, $depth + 1);
        if ($address === "" && ($child["address"] ?? "") !== "") {
            $address = (string) $child["address"];
        }
        if ($target === "" && ($child["target"] ?? "") !== "") {
            $target = (string) $child["target"];
        }
    }
    return ["imports" => $scan["imports"], "address" => $address, "target" => $target, "xml" => $xml];
}

function __elephc_soap_stack_find(array $stack, string $prefix): string
{
    for ($index = \count($stack) - 1; $index >= 0; $index--) {
        $entry = $stack[$index];
        if (\strpos($entry, $prefix) === 0) {
            return \substr($entry, \strlen($prefix));
        }
    }
    return "";
}

function __elephc_soap_xsd_type(string $type): string
{
    $local = __elephc_soap_local($type);
    if ($local === "int" || $local === "integer" || $local === "long" || $local === "short"
        || $local === "byte" || $local === "unsignedInt" || $local === "unsignedLong"
        || $local === "unsignedShort" || $local === "unsignedByte"
        || $local === "nonNegativeInteger" || $local === "positiveInteger"
        || $local === "nonPositiveInteger" || $local === "negativeInteger") {
        return "int";
    }
    if ($local === "boolean" || $local === "bool") {
        return "bool";
    }
    if ($local === "float" || $local === "double" || $local === "decimal") {
        return "float";
    }
    if ($local === "string" || $local === "normalizedString" || $local === "token"
        || $local === "anyURI" || $local === "QName" || $local === "base64Binary"
        || $local === "hexBinary") {
        return "string";
    }
    return $local;
}

function __elephc_soap_wsdl_metadata(string $xml): array
{
    $field_text = [];
    $message_text = [];
    $operations_text = "";
    $current_operation_name = "";
    $current_operation_input = "";
    $current_operation_output = "";
    $current_operation_port = false;
    $stack = [];
    $position = 0;
    $length = \strlen($xml);
    while ($position < $length) {
        $start = \strpos($xml, "<", $position);
        if ($start === false) {
            break;
        }
        if (\substr($xml, $start, 4) === "<!--") {
            $comment_end = \strpos($xml, "-->", $start);
            $position = $comment_end === false ? $length : $comment_end + 3;
            continue;
        }
        if (\substr($xml, $start, 2) === "<?") {
            $declaration_end = \strpos($xml, "?>", $start);
            $position = $declaration_end === false ? $length : $declaration_end + 2;
            continue;
        }
        $end = \strpos($xml, ">", $start);
        if ($end === false) {
            break;
        }
        $tag = \substr($xml, $start + 1, $end - $start - 1);
        $position = $end + 1;
        if (\substr($tag, 0, 1) === "/") {
            $close_name = \substr($tag, 1);
            $close_space = \strpos($close_name, " ");
            if ($close_space !== false) {
                $close_name = \substr($close_name, 0, $close_space);
            }
            $close_local = __elephc_soap_local($close_name);
            for ($index = \count($stack) - 1; $index >= 0; $index--) {
                $entry = $stack[$index];
                if ($entry === $close_local || \strpos($entry, $close_local . ":") === 0) {
                    $remaining = [];
                    foreach ($stack as $stack_index => $stack_entry) {
                        if ($stack_index !== $index) {
                            $remaining[] = $stack_entry;
                        }
                    }
                    $stack = $remaining;
                    if ($close_local === "operation") {
                        if ($current_operation_port) {
                            $operations_text .= $current_operation_name . "|"
                                . $current_operation_input . "|"
                                . $current_operation_output . "\n";
                        }
                        $current_operation_port = false;
                    }
                    break;
                }
            }
            continue;
        }
        $self_closing = \substr($tag, -1) === "/";
        if ($self_closing) {
            $tag = \substr($tag, 0, -1);
        }
        $tag_space = \strpos($tag, " ");
        $name = $tag_space === false ? $tag : \substr($tag, 0, $tag_space);
        $local = __elephc_soap_local($name);
        $attributes = __elephc_soap_attributes($tag);
        if ($local === "complexType") {
            $type_name = (string) ($attributes["name"] ?? "");
            if ($type_name !== "") {
                if (!isset($field_text[$type_name])) {
                    $field_text[$type_name] = "";
                }
                if (!$self_closing) {
                    $stack[] = "element:" . $type_name;
                }
            }
            if (!$self_closing) {
                $stack[] = "complexType";
            }
            continue;
        }
        if ($local === "element" && isset($attributes["name"])) {
            $field_name = (string) $attributes["name"];
            $field_type = (string) ($attributes["type"] ?? "");
            if ($field_type !== "") {
                $owner = __elephc_soap_stack_find($stack, "element:");
                if ($owner !== "") {
                    $field_text[$owner] = ($field_text[$owner] ?? "")
                        . __elephc_soap_xsd_type($field_type) . " " . $field_name . ";\n";
                }
                continue;
            }
            if (!isset($field_text[$field_name])) {
                $field_text[$field_name] = "";
            }
            if (!$self_closing) {
                $stack[] = "element:" . $field_name;
            }
            continue;
        }
        if ($local === "message" && isset($attributes["name"])) {
            $message_name = (string) $attributes["name"];
            if (!isset($message_text[$message_name])) {
                $message_text[$message_name] = "";
            }
            if (!$self_closing) {
                $stack[] = "message:" . $message_name;
            }
            continue;
        }
        if ($local === "part") {
            $message_name = __elephc_soap_stack_find($stack, "message:");
            if ($message_name !== "") {
                $part_type = "string";
                if (($attributes["element"] ?? "") !== "") {
                    $part_type = __elephc_soap_local((string) $attributes["element"]);
                } elseif (($attributes["type"] ?? "") !== "") {
                    $part_type = __elephc_soap_xsd_type((string) $attributes["type"]);
                }
                $part_name = (string) ($attributes["name"] ?? "parameters");
                if ($part_name === "") {
                    $part_name = "parameters";
                }
                $message_text[$message_name] = ($message_text[$message_name] ?? "")
                    . $part_type . "|" . $part_name . "\n";
            }
            continue;
        }
        if (($local === "portType" || $local === "binding") && !$self_closing) {
            $stack[] = $local;
            continue;
        }
        if ($local === "operation" && isset($attributes["name"])) {
            $current_operation_name = (string) $attributes["name"];
            $current_operation_input = "";
            $current_operation_output = "";
            $current_operation_port = \in_array("portType", $stack, true);
            if (!$self_closing) {
                $stack[] = "operation";
            }
            continue;
        }
        if ($local === "input" || $local === "output") {
            if ($current_operation_port && isset($attributes["message"])) {
                if ($local === "input") {
                    $current_operation_input = (string) $attributes["message"];
                } else {
                    $current_operation_output = (string) $attributes["message"];
                }
            }
            continue;
        }
    }

    $functions = [];
    $operation_lines = \explode("\n", \trim($operations_text));
    foreach ($operation_lines as $operation_line) {
        if ($operation_line === "") {
            continue;
        }
        $operation_parts = \explode("|", $operation_line);
        if (\count($operation_parts) < 3) {
            continue;
        }
        $operation_name = $operation_parts[0];
        $input_name = __elephc_soap_local($operation_parts[1]);
        $output_name = __elephc_soap_local($operation_parts[2]);
        $return_type = "void";
        if ($output_name !== "" && isset($message_text[$output_name]) && $message_text[$output_name] !== "") {
            $output_parts = \explode("\n", \trim($message_text[$output_name]));
            if (\count($output_parts) > 0) {
                $output_pair = \explode("|", $output_parts[0]);
                if (\count($output_pair) > 0) {
                    $return_type = $output_pair[0];
                }
            }
        }
        $parameters = [];
        if ($input_name !== "" && isset($message_text[$input_name]) && $message_text[$input_name] !== "") {
            foreach (\explode("\n", \trim($message_text[$input_name])) as $part) {
                $pair = \explode("|", $part);
                if (\count($pair) >= 2) {
                    $parameters[] = $pair[0] . " $" . $pair[1];
                }
            }
        }
        $functions[] = $return_type . " " . $operation_name . "(" . \implode(", ", $parameters) . ")";
    }

    $types = [];
    foreach ($field_text as $element_name => $lines) {
        $trimmed = \trim($lines);
        if ($trimmed === "") {
            continue;
        }
        $text = "struct " . $element_name . " {";
        foreach (\explode("\n", $trimmed) as $line) {
            $text .= "\n " . $line;
        }
        $text .= "\n}";
        $types[] = $text;
    }

    return ["functions" => $functions, "types" => $types];
}

function __elephc_soap_http_request(string $method, string $location, string $payload, array $headers, int $timeout): string
{
    $exchange = __elephc_soap_http_exchange($method, $location, $payload, $headers, $timeout);
    return (string) $exchange["body"];
}

function __elephc_soap_http_exchange(string $method, string $location, string $payload, array $headers, int $timeout): array
{
    $parts = \parse_url($location);
    if (!\is_array($parts) || !isset($parts["host"])) {
        throw new \SoapFault("HTTP", "Invalid SOAP location");
    }
    $scheme = \strtolower((string) ($parts["scheme"] ?? "http"));
    $host = (string) $parts["host"];
    $port = (int) ($parts["port"] ?? ($scheme === "https" ? 443 : 80));
    $path = (string) ($parts["path"] ?? "/");
    if (isset($parts["query"])) {
        $path .= "?" . (string) $parts["query"];
    }
    $remote = ($scheme === "https" ? "tls://" : "tcp://") . $host . ":" . $port;
    $socket = @\stream_socket_client($remote);
    if (!\is_resource($socket)) {
        throw new \SoapFault("HTTP", "Could not connect to SOAP server");
    }
    @\stream_set_timeout($socket, $timeout);
    $head = $method . " " . $path . " HTTP/1.1\r\n"
        . "Host: " . $host . "\r\n"
        . "Connection: close\r\n";
    if ($payload !== "") {
        $head .= "Content-Length: " . \strlen($payload) . "\r\n";
    }
    $head .= \implode("\r\n", $headers) . "\r\n\r\n";
    if (@\fwrite($socket, $head . $payload) === false) {
        \fclose($socket);
        throw new \SoapFault("HTTP", "Could not send the SOAP request");
    }
    $response = \stream_get_contents($socket);
    \fclose($socket);
    if (!\is_string($response)) {
        throw new \SoapFault("HTTP", "Could not read the SOAP response");
    }
    $header_end = \strpos($response, "\r\n\r\n");
    if ($header_end === false) {
        throw new \SoapFault("HTTP", "Malformed SOAP HTTP response");
    }
    $body = \substr($response, $header_end + 4);
    $header_block = \substr($response, 0, $header_end);
    foreach (\explode("\r\n", $header_block) as $line) {
        $colon = \strpos($line, ":");
        if ($colon === false) {
            continue;
        }
        if (\strtolower(\substr($line, 0, $colon)) === "content-length") {
            $length = (int) \trim(\substr($line, $colon + 1));
            if ($length >= 0 && \strlen($body) > $length) {
                $body = \substr($body, 0, $length);
            }
        }
    }
    return [
        "body" => $body,
        "request_headers" => $head,
        "response_headers" => $header_block . "\r\n\r\n",
    ];
}

function __elephc_soap_decode_value(mixed $value)
{
    if (!\is_array($value) || !\array_key_exists("item", $value)) {
        return $value;
    }
    $items = $value["item"];
    if (!\is_array($items)) {
        return [$items];
    }
    if (\array_key_exists("key", $items) && \array_key_exists("value", $items)) {
        return [(string) $items["key"] => $items["value"]];
    }
    $is_map = true;
    foreach ($items as $item) {
        if (!\is_array($item) || !\array_key_exists("key", $item) || !\array_key_exists("value", $item)) {
            $is_map = false;
            break;
        }
    }
    if ($is_map) {
        $map = [];
        foreach ($items as $item) {
            $map[(string) $item["key"]] = $item["value"];
        }
        return $map;
    }
    return $items;
}

function __elephc_soap_response(string $xml, int $version = SOAP_1_1)
{
    $root = __elephc_soap_element($xml, 0);
    if ($root[0] === "" || !\is_array($root[1]) || !\array_key_exists("Body", $root[1])) {
        throw new \SoapFault("Client", "SOAP response has no Body");
    }
    $body = $root[1]["Body"];
    if (!\is_array($body)) {
        throw new \SoapFault("Client", "SOAP response Body is empty");
    }
    $first = null;
    foreach ($body as $name => $value) {
        if ($name === "__text") {
            continue;
        }
        $first = [$name, $value];
        break;
    }
    if ($first === null) {
        throw new \SoapFault("Client", "SOAP response Body is empty");
    }
    if ($first[0] === "Fault") {
        $fault = $first[1];
        if ($version === SOAP_1_2 && \is_array($fault)) {
            $code = "SOAP-ENV:Server";
            $codeNode = $fault["Code"] ?? null;
            if (\is_array($codeNode)) {
                $code = (string) ($codeNode["Value"] ?? $code);
            }
            $string = "SOAP fault";
            $reasonNode = $fault["Reason"] ?? null;
            if (\is_array($reasonNode)) {
                $string = (string) ($reasonNode["Text"] ?? $string);
            }
            throw new \SoapFault($code, $string);
        }
        $code = \is_array($fault) ? (string) ($fault["faultcode"] ?? "SOAP-ENV:Server") : "SOAP-ENV:Server";
        $string = \is_array($fault) ? (string) ($fault["faultstring"] ?? "SOAP fault") : (string) $fault;
        throw new \SoapFault($code, $string);
    }
    if (\is_array($first[1]) && \count($first[1]) === 1) {
        if (\array_key_exists("result", $first[1])) {
            return __elephc_soap_decode_value($first[1]["result"]);
        }
        if (\array_key_exists("return", $first[1])) {
            return __elephc_soap_decode_value($first[1]["return"]);
        }
    }
    return __elephc_soap_decode_value($first[1]);
}
