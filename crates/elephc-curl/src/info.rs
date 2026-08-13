//! Purpose:
//! `curl_getinfo()`'s data shapes: the NUL-framed blob a `CURLINFO_SLIST` field becomes,
//! the JSON blob `CURLINFO_CERTINFO` becomes, and the JSON blob the no-`$option`
//! associative-array form becomes. Everything here produces BYTES for the PHP side to
//! decode; nothing here knows about PHP values.
//!
//! Called from:
//! - `crate::abi::elephc_curl_easy_str_op`, the one entry point behind every
//!   string-shaped `curl_getinfo()` answer.
//!
//! Key details:
//! - THE ARRAY FORM IS JSON, NOT A BESPOKE ENCODING, for the same reason
//!   `curl_version()` is (`crate::abi::build_global_info_json`): the prelude decodes it
//!   with the ordinary `json_decode` builtin, so no PHP array has to be built in
//!   hand-written assembly, and the key set stays readable in one place.
//! - THE KEY LIST AND THE VALUE TYPES ARE php-src's, field for field
//!   (`ext/curl/interface.c`, `curl_getinfo` with no `$option`). A key libcurl cannot
//!   answer for is OMITTED, exactly as php-src omits it — never filled in with a
//!   fabricated zero. `request_header` is absent for a different reason, recorded at its
//!   place below.
//! - SIX KEYS READ THE `_T` (`curl_off_t`) FIELD AND REPORT A FLOAT. php-src does the
//!   same: `size_upload`/`size_download`/`speed_*`/`*_content_length` were `double`
//!   fields in libcurl 7.x, so php-src reads the modern `_T` field and re-widens it to a
//!   double to keep the array's value types stable across libcurl versions. The `*_us`
//!   keys are the same `_T` fields reported as integers, which is also php-src's shape.

use crate::easy::{self, CURL};

/// Joins byte items into the NUL-TERMINATED framing the elephc side splits on: each
/// item's bytes followed by one `0`. Empty input is an empty blob, so an absent list and
/// an empty list are the same thing — which is what PHP's `[]` for both is too.
pub(crate) fn frame_items(items: &[Vec<u8>]) -> Vec<u8> {
    let mut blob = Vec::new();
    for item in items {
        blob.extend_from_slice(item);
        blob.push(0);
    }
    blob
}

/// Encodes `CURLINFO_CERTINFO` as PHP's shape: a JSON array with one object per
/// certificate, each mapping the `"Name"` of a `"Name:Value"` line to its `"Value"`.
/// A line with no `:` is kept under its whole text as the key with an empty value, which
/// is what php-src's `php_curl_certinfo` does with an unsplittable line.
///
/// # Safety
/// `curl` must be a still-live libcurl easy handle.
pub(crate) unsafe fn certinfo_json(curl: *mut CURL) -> Vec<u8> {
    let certs = unsafe { easy::getinfo_certinfo(curl) }.unwrap_or_default();
    let mut out = Vec::new();
    for lines in certs {
        let mut cert = serde_json::Map::new();
        for line in lines {
            let text = String::from_utf8_lossy(&line).into_owned();
            match text.split_once(':') {
                Some((name, value)) => {
                    cert.insert(name.to_string(), value.to_string().into());
                }
                None => {
                    cert.insert(text, String::new().into());
                }
            }
        }
        out.push(serde_json::Value::Object(cert));
    }
    serde_json::Value::Array(out).to_string().into_bytes()
}

/// Builds the no-`$option` `curl_getinfo()` associative array as JSON, with php-src's own
/// key names and value types.
///
/// # Safety
/// `curl` must be a still-live libcurl easy handle.
pub(crate) unsafe fn getinfo_all_json(curl: *mut CURL) -> Vec<u8> {
    /// `CURLINFO_*` numbers, named here rather than in `crate::easy` because this is the
    /// only place that reads them and the list IS the documented key set.
    mod info {
        pub(super) const EFFECTIVE_URL: i32 = 0x10_0000 + 1;
        pub(super) const CONTENT_TYPE: i32 = 0x10_0000 + 18;
        pub(super) const REDIRECT_URL: i32 = 0x10_0000 + 31;
        pub(super) const PRIMARY_IP: i32 = 0x10_0000 + 32;
        pub(super) const LOCAL_IP: i32 = 0x10_0000 + 41;
        pub(super) const SCHEME: i32 = 0x10_0000 + 49;
        pub(super) const EFFECTIVE_METHOD: i32 = 0x10_0000 + 58;
        pub(super) const CAINFO: i32 = 0x10_0000 + 61;
        pub(super) const CAPATH: i32 = 0x10_0000 + 62;

        pub(super) const RESPONSE_CODE: i32 = 0x20_0000 + 2;
        pub(super) const HEADER_SIZE: i32 = 0x20_0000 + 11;
        pub(super) const REQUEST_SIZE: i32 = 0x20_0000 + 12;
        pub(super) const SSL_VERIFYRESULT: i32 = 0x20_0000 + 13;
        pub(super) const FILETIME: i32 = 0x20_0000 + 14;
        pub(super) const REDIRECT_COUNT: i32 = 0x20_0000 + 20;
        pub(super) const PRIMARY_PORT: i32 = 0x20_0000 + 40;
        pub(super) const LOCAL_PORT: i32 = 0x20_0000 + 42;
        pub(super) const HTTP_VERSION: i32 = 0x20_0000 + 46;
        pub(super) const PROXY_SSL_VERIFYRESULT: i32 = 0x20_0000 + 47;
        pub(super) const PROTOCOL: i32 = 0x20_0000 + 48;

        pub(super) const TOTAL_TIME: i32 = 0x30_0000 + 3;
        pub(super) const NAMELOOKUP_TIME: i32 = 0x30_0000 + 4;
        pub(super) const CONNECT_TIME: i32 = 0x30_0000 + 5;
        pub(super) const PRETRANSFER_TIME: i32 = 0x30_0000 + 6;
        pub(super) const STARTTRANSFER_TIME: i32 = 0x30_0000 + 17;
        pub(super) const REDIRECT_TIME: i32 = 0x30_0000 + 19;

        pub(super) const SIZE_UPLOAD_T: i32 = 0x60_0000 + 7;
        pub(super) const SIZE_DOWNLOAD_T: i32 = 0x60_0000 + 8;
        pub(super) const SPEED_DOWNLOAD_T: i32 = 0x60_0000 + 9;
        pub(super) const SPEED_UPLOAD_T: i32 = 0x60_0000 + 10;
        pub(super) const CONTENT_LENGTH_DOWNLOAD_T: i32 = 0x60_0000 + 15;
        pub(super) const CONTENT_LENGTH_UPLOAD_T: i32 = 0x60_0000 + 16;
        pub(super) const APPCONNECT_TIME_T: i32 = 0x60_0000 + 56;
        pub(super) const CONNECT_TIME_T: i32 = 0x60_0000 + 55;
        pub(super) const NAMELOOKUP_TIME_T: i32 = 0x60_0000 + 54;
        pub(super) const PRETRANSFER_TIME_T: i32 = 0x60_0000 + 57;
        pub(super) const REDIRECT_TIME_T: i32 = 0x60_0000 + 59;
        pub(super) const STARTTRANSFER_TIME_T: i32 = 0x60_0000 + 60;
        pub(super) const TOTAL_TIME_T: i32 = 0x60_0000 + 50;
    }

    let mut map = serde_json::Map::new();
    // A string field libcurl answers with a NULL `char *` becomes `""` here. php-src's
    // array builder passes that same pointer straight to `add_assoc_string`, so it has no
    // defined answer for it; the empty string is the closest non-crashing equivalent and
    // is what libcurl itself means by "no value". `content_type` is the ONE key php-src
    // handles explicitly, and it gets its own writer below.
    let string_key = |map: &mut serde_json::Map<String, serde_json::Value>,
                      key: &str,
                      number: i32| {
        if let Some(value) = unsafe { easy::getinfo_str(curl, number) } {
            let text = value
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default();
            map.insert(key.to_string(), text.into());
        }
    };
    string_key(&mut map, "url", info::EFFECTIVE_URL);

    // `content_type` IS EXPLICITLY NULLABLE, matching php-src:
    //     if (s_code != NULL) { CAAS("content_type", s_code); } else { CAAZ("content_type"); }
    // `CAAZ` adds a NULL zval, so a response with no `Content-Type` header answers
    // `null` — not `""`, which is what a server that sent an EMPTY `Content-Type` would
    // produce and is therefore a different fact.
    match unsafe { easy::getinfo_str(curl, info::CONTENT_TYPE) } {
        Some(Some(bytes)) => {
            map.insert(
                "content_type".to_string(),
                String::from_utf8_lossy(&bytes).into_owned().into(),
            );
        }
        Some(None) => {
            map.insert("content_type".to_string(), serde_json::Value::Null);
        }
        None => {}
    }

    let long_key = |map: &mut serde_json::Map<String, serde_json::Value>,
                        key: &str,
                        number: i32| {
        if let Some(value) = unsafe { easy::getinfo_long(curl, number) } {
            map.insert(key.to_string(), value.into());
        }
    };
    long_key(&mut map, "http_code", info::RESPONSE_CODE);
    long_key(&mut map, "header_size", info::HEADER_SIZE);
    long_key(&mut map, "request_size", info::REQUEST_SIZE);
    long_key(&mut map, "filetime", info::FILETIME);
    long_key(&mut map, "ssl_verify_result", info::SSL_VERIFYRESULT);
    long_key(&mut map, "redirect_count", info::REDIRECT_COUNT);

    let double_key = |map: &mut serde_json::Map<String, serde_json::Value>,
                          key: &str,
                          number: i32| {
        if let Some(value) = unsafe { easy::getinfo_double(curl, number) } {
            map.insert(key.to_string(), json_number(value));
        }
    };
    double_key(&mut map, "total_time", info::TOTAL_TIME);
    double_key(&mut map, "namelookup_time", info::NAMELOOKUP_TIME);
    double_key(&mut map, "connect_time", info::CONNECT_TIME);
    double_key(&mut map, "pretransfer_time", info::PRETRANSFER_TIME);

    // php-src reads the modern `_T` field and reports a DOUBLE for these six, keeping the
    // array's value types the same as they were when libcurl exposed them as doubles.
    let off_t_as_double = |map: &mut serde_json::Map<String, serde_json::Value>,
                               key: &str,
                               number: i32| {
        if let Some(value) = unsafe { easy::getinfo_long(curl, number) } {
            map.insert(key.to_string(), json_number(value as f64));
        }
    };
    off_t_as_double(&mut map, "size_upload", info::SIZE_UPLOAD_T);
    off_t_as_double(&mut map, "size_download", info::SIZE_DOWNLOAD_T);
    off_t_as_double(&mut map, "speed_download", info::SPEED_DOWNLOAD_T);
    off_t_as_double(&mut map, "speed_upload", info::SPEED_UPLOAD_T);
    off_t_as_double(
        &mut map,
        "download_content_length",
        info::CONTENT_LENGTH_DOWNLOAD_T,
    );
    off_t_as_double(
        &mut map,
        "upload_content_length",
        info::CONTENT_LENGTH_UPLOAD_T,
    );

    double_key(&mut map, "starttransfer_time", info::STARTTRANSFER_TIME);
    double_key(&mut map, "redirect_time", info::REDIRECT_TIME);
    string_key(&mut map, "redirect_url", info::REDIRECT_URL);
    string_key(&mut map, "primary_ip", info::PRIMARY_IP);

    // `certinfo` is always present in php-src's array (an empty array when libcurl has no
    // chain, e.g. a plaintext transfer or `CURLOPT_CERTINFO` never enabled).
    let certs = unsafe { easy::getinfo_certinfo(curl) }.unwrap_or_default();
    let mut certinfo = Vec::new();
    for lines in certs {
        let mut cert = serde_json::Map::new();
        for line in lines {
            let text = String::from_utf8_lossy(&line).into_owned();
            match text.split_once(':') {
                Some((name, value)) => cert.insert(name.to_string(), value.to_string().into()),
                None => cert.insert(text, String::new().into()),
            };
        }
        certinfo.push(serde_json::Value::Object(cert));
    }
    map.insert("certinfo".to_string(), serde_json::Value::Array(certinfo));

    long_key(&mut map, "primary_port", info::PRIMARY_PORT);
    string_key(&mut map, "local_ip", info::LOCAL_IP);
    long_key(&mut map, "local_port", info::LOCAL_PORT);
    long_key(&mut map, "http_version", info::HTTP_VERSION);
    long_key(&mut map, "protocol", info::PROTOCOL);
    long_key(&mut map, "ssl_verifyresult", info::PROXY_SSL_VERIFYRESULT);
    string_key(&mut map, "scheme", info::SCHEME);

    let off_t_key = |map: &mut serde_json::Map<String, serde_json::Value>,
                         key: &str,
                         number: i32| {
        if let Some(value) = unsafe { easy::getinfo_long(curl, number) } {
            map.insert(key.to_string(), value.into());
        }
    };
    off_t_key(&mut map, "appconnect_time_us", info::APPCONNECT_TIME_T);
    off_t_key(&mut map, "connect_time_us", info::CONNECT_TIME_T);
    off_t_key(&mut map, "namelookup_time_us", info::NAMELOOKUP_TIME_T);
    off_t_key(&mut map, "pretransfer_time_us", info::PRETRANSFER_TIME_T);
    off_t_key(&mut map, "redirect_time_us", info::REDIRECT_TIME_T);
    off_t_key(&mut map, "starttransfer_time_us", info::STARTTRANSFER_TIME_T);
    off_t_key(&mut map, "total_time_us", info::TOTAL_TIME_T);

    string_key(&mut map, "effective_method", info::EFFECTIVE_METHOD);
    string_key(&mut map, "capath", info::CAPATH);
    string_key(&mut map, "cainfo", info::CAINFO);

    // `request_header` is ABSENT, and that is php-src's own behaviour for a handle that
    // never had `CURLINFO_HEADER_OUT` turned on — which no elephc handle can, because
    // that option needs the debug-callback plumbing (Task 12). php-src adds the key only
    // when it has captured text, so an absent key here is indistinguishable from php-src
    // with the option off, rather than a fabricated empty string.

    serde_json::Value::Object(map).to_string().into_bytes()
}

/// Encodes an `f64` for JSON, mapping the non-finite values (which JSON cannot express)
/// to `0` rather than emitting invalid JSON that `json_decode` would reject outright.
/// libcurl only produces these for a transfer that never started.
fn json_number(value: f64) -> serde_json::Value {
    serde_json::Number::from_f64(value)
        .map(serde_json::Value::Number)
        .unwrap_or_else(|| serde_json::Value::Number(0.into()))
}
