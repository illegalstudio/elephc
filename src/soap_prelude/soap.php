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
    private string $__login = "";
    private string $__password = "";
    private int $__timeout = 30;
    private bool $__trace = false;
    private bool $__exceptions = true;
    private string $__userAgent = "elephc-soap";
    private array $__keepHeaders = [];
    private string $__lastRequest = "";
    private string $__lastResponse = "";
    private mixed $__headers = null;

    public function __construct(?string $wsdl, array $options = [])
    {
        if ($wsdl !== null && $wsdl !== "") {
            throw new \SoapFault("Client", "WSDL mode is not implemented");
        }
        $this->__location = (string) ($options["location"] ?? "");
        $this->__uri = (string) ($options["uri"] ?? "");
        if (isset($options["style"])) {
            $this->__style = (int) $options["style"];
        }
        if (isset($options["use"])) {
            $this->__use = (int) $options["use"];
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
                $params .= __elephc_soap_value((string) $name, $value);
            }
        } else {
            foreach ($args as $index => $value) {
                $params .= __elephc_soap_value("param" . $index, $value);
            }
        }
        $request = '<?xml version="1.0" encoding="UTF-8"?>'
            . '<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"'
            . ' xmlns:ns1="' . __elephc_soap_escape($uri) . '"'
            . ' xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">'
            . '<SOAP-ENV:Body><ns1:' . __elephc_soap_name($function) . '>'
            . $params
            . '</ns1:' . __elephc_soap_name($function) . '></SOAP-ENV:Body></SOAP-ENV:Envelope>';
        $headers = [
            "Content-Type: text/xml; charset=utf-8",
            "SOAPAction: \"" . $soapAction . "\"",
            "Connection: close",
            "User-Agent: " . $this->__userAgent,
        ];
        if ($this->__login !== "") {
            $headers[] = "Authorization: Basic " . \base64_encode($this->__login . ":" . $this->__password);
        }
        $response = __elephc_soap_http_post($location, $request, $headers, $this->__timeout);
        $this->__lastRequest = $request;
        $this->__lastResponse = $response;
        return __elephc_soap_response($response);
    }

    public function __call(string $name, array $arguments)
    {
        return $this->__soapCall($name, $arguments);
    }

    public function __getFunctions(): array
    {
        return [];
    }

    public function __getTypes(): array
    {
        return [];
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

    public function __getCookies(): array
    {
        return [];
    }

    public function __setCookie(string $name, ?string $value = null): void
    {
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

function __elephc_soap_value(string $name, mixed $value): string
{
    $tag = __elephc_soap_name($name);
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

function __elephc_soap_http_post(string $location, string $payload, array $headers, int $timeout): string
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
    $head = "POST " . $path . " HTTP/1.1\r\n"
        . "Host: " . $host . "\r\n"
        . "Content-Length: " . \strlen($payload) . "\r\n"
        . "Connection: close\r\n"
        . \implode("\r\n", $headers) . "\r\n\r\n";
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
    return $body;
}

function __elephc_soap_response(string $xml)
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
        $code = \is_array($fault) ? (string) ($fault["faultcode"] ?? "SOAP-ENV:Server") : "SOAP-ENV:Server";
        $string = \is_array($fault) ? (string) ($fault["faultstring"] ?? "SOAP fault") : (string) $fault;
        throw new \SoapFault($code, $string);
    }
    if (\is_array($first[1]) && \count($first[1]) === 1 && \array_key_exists("result", $first[1])) {
        return $first[1]["result"];
    }
    return $first[1];
}
