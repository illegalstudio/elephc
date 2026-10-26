//! Purpose:
//! Reads SimpleXML document URLs through PHP streams before native parsing.
//!
//! Called from:
//! - The shared SimpleXML constructor and file factory prelude.
//!
//! Key details:
//! - Native file parsing retains local-path diagnostics and relative-entity resolution.
//! - URI bytes are passed with their original URL to a per-context libxml input loader.

pub(super) const SOURCE: &str = r###"
function __elephc_simplexml_file_handle(string $filename, int $options, string $ns, bool $prefix, string $caller): int {
    $separator = strpos($filename, '://');
    if ($separator === false || str_starts_with(strtolower($filename), 'file://')) {
        return elephc_xml_sx_load_file($filename, $options, $ns, (int)$prefix);
    }
    $data = __elephc_simplexml_stream_bytes($filename, $caller);
    if ($data === false) { return elephc_xml_sx_load_stream($filename, '', -1, $options, $ns, (int)$prefix); }
    return elephc_xml_sx_load_stream($filename, $data, strlen($data), $options, $ns, (int)$prefix);
}

function __elephc_simplexml_stream_bytes(string $uri, string $caller): string|false {
    $separator = strpos($uri, '://');
    if ($separator === false) { return file_get_contents($uri); }
    $scheme = strtolower(substr($uri, 0, (int)$separator));
    if ($scheme === 'data') { return __elephc_simplexml_data_bytes($uri, $caller); }
    if ($scheme === 'php') {
        if ($uri === 'php://input') { return file_get_contents('php://input'); }
        if ($uri === 'php://stdin') {
            $stream = fopen('php://stdin', 'rb');
            if ($stream === false) { return false; }
            return stream_get_contents($stream);
        }
        if ($uri === 'php://memory' || $uri === 'php://temp' || str_starts_with($uri, 'php://temp/')) { return ''; }
        if (str_starts_with($uri, 'php://filter/')) { return __elephc_simplexml_filter_bytes($uri, $caller); }
        return false;
    }
    if ($scheme === 'http' || $scheme === 'https' || $scheme === 'ftp' || $scheme === 'ftps' || $scheme === 'phar') {
        $normalized = $scheme . substr($uri, (int)$separator);
        return file_get_contents($normalized);
    }
    if ($scheme === 'compress.zlib' || $scheme === 'compress.bzip2') { return false; }
    if (!in_array($scheme, stream_get_wrappers(), true)) {
        __elephc_simplexml_warning('Warning: ' . $caller . ': Unable to find the wrapper "' . $scheme . '" - did you forget to enable it when you configured PHP?' . "\n");
        return false;
    }
    if (!file_exists($uri)) { return false; }
    $stream = fopen($uri, 'rb');
    if ($stream === false) { return false; }
    $data = stream_get_contents($stream);
    fclose($stream);
    return $data;
}

function __elephc_simplexml_data_failure(string $uri, string $caller, string $reason): false {
    $function = substr($caller, 0, strlen($caller) - 2);
    __elephc_simplexml_warning('Warning: ' . $function . '(' . $uri . '): Failed to open stream: rfc2397: ' . $reason . "\n");
    return false;
}

function __elephc_simplexml_data_bytes(string $uri, string $caller): string|false {
    if (!str_starts_with($uri, 'data://')) { return __elephc_simplexml_data_failure($uri, $caller, 'operation failed'); }
    $rest = substr($uri, 7);
    $comma = strpos($rest, ',');
    if ($comma === false) { return __elephc_simplexml_data_failure($uri, $caller, 'no comma in URL'); }
    $metadata = substr($rest, 0, (int)$comma);
    $payload = substr($rest, (int)$comma + 1);
    $base64 = false;
    if ($metadata !== '') {
        $semi = strpos($metadata, ';');
        $slash = strpos($metadata, '/');
        if ($semi === false) {
            if ($slash === false) { return __elephc_simplexml_data_failure($uri, $caller, 'illegal media type'); }
        } else {
            if (($slash === false || $slash > $semi) && $metadata !== ';base64') { return __elephc_simplexml_data_failure($uri, $caller, 'illegal media type'); }
            foreach (explode(';', substr($metadata, (int)$semi + 1)) as $parameter) {
                if ($base64) { return __elephc_simplexml_data_failure($uri, $caller, 'illegal parameter'); }
                if (strpos($parameter, '=') === false) {
                    if ($parameter !== 'base64') { return __elephc_simplexml_data_failure($uri, $caller, 'illegal parameter'); }
                    $base64 = true;
                }
            }
        }
    }
    if ($base64) {
        $decoded = base64_decode($payload, true);
        if ($decoded === false) { return __elephc_simplexml_data_failure($uri, $caller, 'unable to decode'); }
        return $decoded;
    }
    return urldecode($payload);
}

function __elephc_simplexml_filter_bytes(string $uri, string $caller): string|false {
    $spec = substr($uri, 13);
    if (str_starts_with($spec, 'resource=')) { return __elephc_simplexml_stream_bytes(substr($spec, 9), $caller); }
    $resource = strpos($spec, '/resource=');
    if ($resource === false) { return false; }
    $filters = substr($spec, 0, (int)$resource);
    $source = substr($spec, (int)$resource + 10);
    $data = __elephc_simplexml_stream_bytes($source, $caller);
    if ($data === false) { return false; }
    $stream = fopen('php://temp', 'w+');
    if ($stream === false) { return false; }
    fwrite($stream, $data);
    rewind($stream);
    foreach (explode('/', $filters) as $filter_list) {
        if (str_starts_with($filter_list, 'write=')) { continue; }
        if (str_starts_with($filter_list, 'read=')) { $filter_list = substr($filter_list, 5); }
        foreach (explode('|', $filter_list) as $filter) { stream_filter_append($stream, $filter, STREAM_FILTER_READ); }
    }
    $filtered = stream_get_contents($stream);
    fclose($stream);
    return $filtered;
}
"###;
