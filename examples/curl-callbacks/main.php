<?php
// curl_* callbacks — PHP callables invoked by libcurl from inside curl_exec().
//
// CURLOPT_WRITEFUNCTION, CURLOPT_HEADERFUNCTION, CURLOPT_READFUNCTION,
// CURLOPT_PROGRESSFUNCTION, CURLOPT_XFERINFOFUNCTION, and CURLOPT_DEBUGFUNCTION each take a
// PHP callable that libcurl calls back mid-transfer. In a compiled elephc binary that means
// C code inside curl_easy_perform() re-entering compiled PHP: the callable is decomposed at
// the PHP layer into a descriptor plus a codegen adapter address, and the adapter boxes the
// arguments and invokes it. Nothing about that is visible from PHP — this file is ordinary
// PHP that behaves the way php-src does.
//
// Running the transfer for real needs network access; as in examples/curl-get/main.php the
// program still compiles and reports why nothing was fetched when the network is away.
//
// TYPE HINTS ON THE CALLBACK PARAMETERS ARE NOT DECORATION HERE. An untyped closure
// parameter is `mixed` to elephc's type checker, and `strlen($data)` on a `mixed` is a
// compile error — so write `function (CurlHandle $ch, string $data): int`, exactly the
// signature the PHP manual documents, rather than the manual's untyped shorthand.

$url = "http://example.com/";

// -- CURLOPT_WRITEFUNCTION: the callback owns the body ------------------------------------
//
// Installing a write callback REPLACES the default body destination, so curl_exec() answers
// `true` rather than the body — even if CURLOPT_RETURNTRANSFER was set earlier. The return
// value must equal strlen($data); anything else aborts the transfer with CURLE_WRITE_ERROR.
$body = '';
$ch = curl_init($url);
curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$body): int {
    $body .= $data;
    return strlen($data);
});

// -- CURLOPT_HEADERFUNCTION: one call per response header line ----------------------------
//
// The status line arrives first, then each field, then a bare CRLF. Same return-length rule.
$contentType = '';
curl_setopt($ch, CURLOPT_HEADERFUNCTION, function (CurlHandle $ch, string $header) use (&$contentType): int {
    if (stripos($header, "Content-Type:") === 0) {
        $contentType = trim(substr($header, 13));
    }
    return strlen($header);
});

// -- CURLOPT_PROGRESSFUNCTION: needs CURLOPT_NOPROGRESS turned off by hand ----------------
//
// Setting the callback is NOT enough: libcurl reports progress only while CURLOPT_NOPROGRESS
// is false, and php-src does not flip it for you. Returning a nonzero value aborts the
// transfer with CURLE_ABORTED_BY_CALLBACK (42) — that is how you implement a size cap.
$ticks = 0;
curl_setopt($ch, CURLOPT_NOPROGRESS, false);
curl_setopt($ch, CURLOPT_PROGRESSFUNCTION, function (
    CurlHandle $ch,
    int $downloadTotal,
    int $downloadedSoFar,
    int $uploadTotal,
    int $uploadedSoFar
) use (&$ticks): int {
    $ticks = $ticks + 1;
    // Refuse anything claiming to be larger than 1 MiB.
    return $downloadTotal > 1048576 ? 1 : 0;
});

$ok = curl_exec($ch);

if ($ok === false) {
    echo "GET " . $url . " failed: errno " . curl_errno($ch) . " (" . curl_error($ch) . ")\n";
} else {
    // Note: `true`, not the body — the write callback took it.
    echo "GET " . $url . " -> " . strlen($body) . " bytes";
    echo ", content-type " . ($contentType === '' ? "(none)" : $contentType);
    echo ", " . $ticks . " progress callbacks\n";
}

// -- CURLOPT_READFUNCTION: the callback supplies an upload body ---------------------------
//
// libcurl asks for at most $length bytes; return a string (a longer one is truncated, as in
// php-src) and an empty string to signal end-of-data. $fd is the CURLOPT_INFILE stream,
// which this build does not carry, so it is always null — as it is in php-src when no
// CURLOPT_INFILE was set.
$payload = ["hello ", "from ", "elephc"];
$sent = 0;
$upload = curl_init("http://127.0.0.1:1/");
curl_setopt($upload, CURLOPT_RETURNTRANSFER, true);
curl_setopt($upload, CURLOPT_UPLOAD, true);
curl_setopt($upload, CURLOPT_INFILESIZE, 17);
curl_setopt($upload, CURLOPT_READFUNCTION, function (CurlHandle $ch, $fd, int $length) use ($payload, &$sent): string {
    if ($sent >= 3) {
        return "";
    }
    $chunk = $payload[$sent];
    $sent = $sent + 1;
    return $chunk;
});

// A closed local port proves the failure shape without needing the network.
$result = curl_exec($upload);
echo $result === false
    ? "upload to a closed port refused, as expected (errno " . curl_errno($upload) . ")\n"
    : "unexpectedly succeeded\n";

// -- Lifecycle: null restores the default, curl_reset() clears everything ------------------
//
// Passing null to a callback option restores that option's DEFAULT behaviour. For
// CURLOPT_WRITEFUNCTION the default is stdout — NOT whatever CURLOPT_RETURNTRANSFER was set
// to earlier, because php keeps a single write mode and null selects "write to stdout".
curl_setopt($ch, CURLOPT_WRITEFUNCTION, null);
curl_reset($ch);
echo "callbacks cleared\n";
