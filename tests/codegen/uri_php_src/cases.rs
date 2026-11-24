//! Purpose:
//! Rust-native table of the php-src `ext/uri` compliance cases run by
//! `tests/codegen/uri_php_src.rs`.
//!
//! Called from:
//! - `tests/codegen/uri_php_src.rs` through `super::cases::CASES`.
//!
//! Key details:
//! - Sources and expectations are derived from php/php-src's `ext/uri/tests` suite, which is
//!   distributed under the PHP License 3.01.
//! - `isolate`, `compile_failure`, `heap_size`, and `zend_test` preserve the harness split:
//!   identity/serialization/redeclaration cases compile alone, OOM cases use their INI heap,
//!   and `zend_test` cases receive the harness's `zend_test_uri_parser()` stand-in.

/// One php-src-derived URI compliance case.
pub(super) struct UriCase {
    /// Stable relative case identifier, used in failure messages.
    pub(super) name: &'static str,
    /// The embedded PHP source.
    pub(super) source: &'static str,
    /// The expected output.
    pub(super) expected: &'static str,
    /// Whether `expected` uses php-src run-tests wildcards.
    pub(super) expectf: bool,
    /// Whether the case must compile and run on its own.
    pub(super) isolate: bool,
    /// Whether the case is expected to fail during compilation.
    pub(super) compile_failure: bool,
    /// The OOM case heap size, in bytes.
    pub(super) heap_size: Option<usize>,
    /// Whether the source needs the `zend_test_uri_parser()` stand-in.
    pub(super) zend_test: bool,
}

/// All cases in stable relative-path order.
pub(super) const CASES: &[UriCase] = &[
    UriCase {
        name: "003",
        source: r#"<?php

var_dump(Uri\Rfc3986\Uri::parse("http://username:password@héééostname:9090/gah/../path?arg=vaéue#anchor"));
var_dump(Uri\WhatWg\Url::parse("http://username:password@héééostname:9090/gah/../path?arg=vaéue#anchor"));

var_dump(Uri\Rfc3986\Uri::parse("//host123/"));
var_dump(Uri\Rfc3986\Uri::parse("///foo/"));
var_dump(Uri\Rfc3986\Uri::parse("/page:1"));
var_dump(Uri\WhatWg\Url::parse("/page:1"));

"#,
        expected: r#"NULL
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  string(8) "username"
  ["password"]=>
  string(8) "password"
  ["host"]=>
  string(18) "xn--hostname-b1aaa"
  ["port"]=>
  int(9090)
  ["path"]=>
  string(5) "/path"
  ["query"]=>
  string(14) "arg=va%C3%A9ue"
  ["fragment"]=>
  string(6) "anchor"
}
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(7) "host123"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(0) ""
  ["port"]=>
  NULL
  ["path"]=>
  string(5) "/foo/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(7) "/page:1"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
NULL
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "004",
        source: r#"<?php

var_dump(new Uri\Rfc3986\Uri(""));
var_dump(Uri\Rfc3986\Uri::parse(""));

try {
    new Uri\WhatWg\Url("");
} catch (Uri\WhatWg\InvalidUrlException $e) {
    echo $e->getMessage() . "\n";
}

var_dump(Uri\WhatWg\Url::parse(""));

var_dump(Uri\Rfc3986\Uri::parse("192.168/contact.html"));
var_dump(Uri\WhatWg\Url::parse("192.168/contact.html", null));

var_dump(Uri\Rfc3986\Uri::parse("http://RuPaul's Drag Race All Stars 7 Winners Cast on This Season's"));
var_dump(Uri\WhatWg\Url::parse("http://RuPaul's Drag Race All Stars 7 Winners Cast on This Season's", null));

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
The specified URI is malformed (MissingSchemeNonRelativeUrl)
NULL
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(20) "192.168/contact.html"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
NULL
NULL
NULL
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "005",
        source: r#"<?php

var_dump(Uri\Rfc3986\Uri::parse("http://username:password@héééostname:9090/gah/../path?arg=vaéue#anchor"));

$url = Uri\WhatWg\Url::parse("http://username:password@héééostname:9090/gah/../path?arg=vaéue#anchor");
var_dump($url);
var_dump($url->getAsciiHost());
var_dump($url->getUnicodeHost());
var_dump($url->toAsciiString());
var_dump($url->toUnicodeString());

"#,
        expected: r#"NULL
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  string(8) "username"
  ["password"]=>
  string(8) "password"
  ["host"]=>
  string(18) "xn--hostname-b1aaa"
  ["port"]=>
  int(9090)
  ["path"]=>
  string(5) "/path"
  ["query"]=>
  string(14) "arg=va%C3%A9ue"
  ["fragment"]=>
  string(6) "anchor"
}
string(18) "xn--hostname-b1aaa"
string(14) "héééostname"
string(75) "http://username:password@xn--hostname-b1aaa:9090/path?arg=va%C3%A9ue#anchor"
string(71) "http://username:password@héééostname:9090/path?arg=va%C3%A9ue#anchor"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "006",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("https://username:password@example.com:8080/path?q=r#fragment");
var_dump($uri);

$url = new Uri\WhatWg\Url("https://username:password@example.com:8080/path?q=r#fragment");
var_dump($url);

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(8) "username"
  ["password"]=>
  string(8) "password"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  int(8080)
  ["path"]=>
  string(5) "/path"
  ["query"]=>
  string(3) "q=r"
  ["fragment"]=>
  string(8) "fragment"
}
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(8) "username"
  ["password"]=>
  string(8) "password"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  int(8080)
  ["path"]=>
  string(5) "/path"
  ["query"]=>
  string(3) "q=r"
  ["fragment"]=>
  string(8) "fragment"
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "007",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("https://example.com:8080@username:password/path?q=r#fragment");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

try {
    new Uri\WhatWg\Url("https://example.com:8080@username:password/path?q=r#fragment");
} catch (Uri\WhatWg\InvalidUrlException $e) {
    echo $e->getMessage() . "\n";
    var_dump($e->errors);
}

$failures = [];
$url = new Uri\WhatWg\Url(" https://example.org ", null, $failures);
var_dump($url->toAsciiString());
var_dump($failures);

"#,
        expected: r#"The specified URI is malformed
The specified URI is malformed (PortInvalid)
array(%d) {
  [0]=>
  object(Uri\WhatWg\UrlValidationError)#%d (%d) {
    ["context"]=>
    string(26) "password/path?q=r#fragment"
    ["type"]=>
    enum(Uri\WhatWg\UrlValidationErrorType::PortInvalid)
    ["failure"]=>
    bool(true)
  }
  [1]=>
  object(Uri\WhatWg\UrlValidationError)#%d (%d) {
    ["context"]=>
    string(36) "@username:password/path?q=r#fragment"
    ["type"]=>
    enum(Uri\WhatWg\UrlValidationErrorType::InvalidCredentials)
    ["failure"]=>
    bool(false)
  }
}
string(20) "https://example.org/"
array(2) {
  [0]=>
  object(Uri\WhatWg\UrlValidationError)#%d (%d) {
    ["context"]=>
    string(1) " "
    ["type"]=>
    enum(Uri\WhatWg\UrlValidationErrorType::InvalidUrlUnit)
    ["failure"]=>
    bool(false)
  }
  [1]=>
  object(Uri\WhatWg\UrlValidationError)#%d (%d) {
    ["context"]=>
    string(21) " https://example.org "
    ["type"]=>
    enum(Uri\WhatWg\UrlValidationErrorType::InvalidUrlUnit)
    ["failure"]=>
    bool(false)
  }
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "008",
        source: r#"<?php

function callRfc3986Getters($uri)
{
    var_dump($uri->getScheme());
    var_dump($uri->getRawScheme());
    var_dump($uri->getUsername());
    var_dump($uri->getRawUsername());
    var_dump($uri->getPassword());
    var_dump($uri->getRawPassword());
    var_dump($uri->getUserInfo());
    var_dump($uri->getRawUserInfo());
    var_dump($uri->getHost());
    var_dump($uri->getRawHost());
    var_dump($uri->getPort());
    var_dump($uri->getPath());
    var_dump($uri->getRawPath());
    var_dump($uri->getQuery());
    var_dump($uri->getRawQuery());
    var_dump($uri->getFragment());
    var_dump($uri->getRawFragment());
}

function callWhatWgGetters($url)
{
    var_dump($url->getScheme());
    var_dump($url->getUsername());
    var_dump($url->getPassword());
    var_dump($url->getAsciiHost());
    var_dump($url->getUnicodeHost());
    var_dump($url->getPort());
    var_dump($url->getPath());
    var_dump($url->getQuery());
    var_dump($url->getFragment());
}

$uri = Uri\Rfc3986\Uri::parse("https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists");
callRfc3986Getters($uri);

echo "\n";

$url = Uri\WhatWg\Url::parse("https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists");
callWhatWgGetters($url);

"#,
        expected: r#"string(5) "https"
string(5) "https"
string(8) "username"
string(8) "username"
string(8) "password"
string(8) "password"
string(17) "username:password"
string(17) "username:password"
string(14) "www.google.com"
string(14) "www.google.com"
int(8080)
string(30) "/pathname1/pathname2/pathname3"
string(30) "/pathname1/pathname2/pathname3"
string(10) "query=true"
string(10) "query=true"
string(11) "hash-exists"
string(11) "hash-exists"

string(5) "https"
string(8) "username"
string(8) "password"
string(14) "www.google.com"
string(14) "www.google.com"
int(8080)
string(30) "/pathname1/pathname2/pathname3"
string(10) "query=true"
string(11) "hash-exists"
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "009",
        source: r#"<?php

var_dump(Uri\Rfc3986\Uri::parse("chrome-extension://example.com"));
var_dump(Uri\WhatWg\Url::parse("chrome-extension://example.com"));

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(16) "chrome-extension"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(16) "chrome-extension"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "010",
        source: r#"<?php

var_dump(Uri\Rfc3986\Uri::parse("http://example.com/path/to/file2", new Uri\Rfc3986\Uri("https://test.com")));
var_dump(Uri\Rfc3986\Uri::parse("/path/to/file2", new Uri\Rfc3986\Uri("https://test.com")));
var_dump(Uri\WhatWg\Url::parse("http://example.com/path/to/file1", new Uri\WhatWg\Url("https://test.com")));
var_dump(Uri\WhatWg\Url::parse("/path/to/file1", new Uri\WhatWg\Url("https://test.com")));

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(14) "/path/to/file2"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(8) "test.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(14) "/path/to/file2"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(14) "/path/to/file1"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(8) "test.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(14) "/path/to/file1"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "011",
        source: r#"<?php

var_dump(Uri\Rfc3986\Uri::parse("http://////www.EXAMPLE.com:80")->toRawString());
var_dump(Uri\Rfc3986\Uri::parse("https://www.example.com/dir1/../dir2")->toRawString());
var_dump(Uri\Rfc3986\Uri::parse("https://你好你好"));
var_dump(Uri\Rfc3986\Uri::parse("https://０Ｘｃ０．０２５０．０１"));
var_dump(Uri\Rfc3986\Uri::parse("HttPs://0300.0250.0000.0001/path?query=foo%20bar")->toRawString());

var_dump(Uri\WhatWg\Url::parse("http://////www.EXAMPLE.com:80")->toAsciiString());
var_dump(Uri\WhatWg\Url::parse("https://www.example.com:443/dir1/../dir2")->toAsciiString());
var_dump(Uri\WhatWg\Url::parse("https://你好你好")->toAsciiString());
var_dump(Uri\WhatWg\Url::parse("https://你好你好")->toUnicodeString());
var_dump(Uri\WhatWg\Url::parse("https://０Ｘｃ０．０２５０．０１")->toAsciiString());
var_dump(Uri\WhatWg\Url::parse("HttPs://0300.0250.0000.0001/path?query=foo%20bar")->toAsciiString());

"#,
        expected: r#"string(29) "http://////www.EXAMPLE.com:80"
string(36) "https://www.example.com/dir1/../dir2"
NULL
NULL
string(48) "HttPs://0300.0250.0000.0001/path?query=foo%20bar"
string(23) "http://www.example.com/"
string(28) "https://www.example.com/dir2"
string(23) "https://xn--6qqa088eba/"
string(21) "https://你好你好/"
string(20) "https://192.168.0.1/"
string(40) "https://192.168.0.1/path?query=foo%20bar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "012",
        source: r#"<?php

var_dump(Uri\Rfc3986\Uri::parse("mailto:Joe@Example.COM"));
var_dump(Uri\WhatWg\Url::parse("mailto:Joe@Example.COM"));

var_dump(Uri\Rfc3986\Uri::parse("file:///E:/Documents%20and%20Settings"));
var_dump(Uri\WhatWg\Url::parse("file:///E:\\Documents and Settings"));

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(6) "mailto"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(15) "Joe@Example.COM"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(6) "mailto"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(15) "Joe@Example.COM"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(4) "file"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(0) ""
  ["port"]=>
  NULL
  ["path"]=>
  string(30) "/E:/Documents%20and%20Settings"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "file"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(0) ""
  ["port"]=>
  NULL
  ["path"]=>
  string(30) "/E:/Documents%20and%20Settings"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "013",
        source: r#"<?php

var_dump(Uri\Rfc3986\Uri::parse("http://example.com?foo=Hell%C3%B3+W%C3%B6rld"));
var_dump(Uri\Rfc3986\Uri::parse("http://example.com?foo=Helló Wörld"));

var_dump(Uri\WhatWg\Url::parse("http://example.com?foo=Hell%C3%B3+W%C3%B6rld"));
var_dump(Uri\WhatWg\Url::parse("http://example.com?foo=Helló Wörld"));

var_dump(Uri\Rfc3986\Uri::parse("http://example.com?foobar=%27%3Cscript%3E+%2B+%40"));
var_dump(Uri\Rfc3986\Uri::parse("http://example.com?foobar='<script> + @"));

var_dump(Uri\WhatWg\Url::parse("http://example.com?foobar=%27%3Cscript%3E+%2B+%40"));
var_dump(Uri\WhatWg\Url::parse("http://example.com?foobar='<script> + @"));

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  string(25) "foo=Hell%C3%B3+W%C3%B6rld"
  ["fragment"]=>
  NULL
}
NULL
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  string(25) "foo=Hell%C3%B3+W%C3%B6rld"
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  string(27) "foo=Hell%C3%B3%20W%C3%B6rld"
  ["fragment"]=>
  NULL
}
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  string(30) "foobar=%27%3Cscript%3E+%2B+%40"
  ["fragment"]=>
  NULL
}
NULL
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  string(30) "foobar=%27%3Cscript%3E+%2B+%40"
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  string(30) "foobar=%27%3Cscript%3E%20+%20@"
  ["fragment"]=>
  NULL
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "014",
        source: r#"<?php

var_dump(Uri\Rfc3986\Uri::parse("http://example.com?foo=Hell%C3%B3+W%C3%B6rld")->toRawString());

var_dump(Uri\WhatWg\Url::parse("http://example.com?foo=Hell%C3%B3+W%C3%B6rld")->toAsciiString());

"#,
        expected: r#"string(44) "http://example.com?foo=Hell%C3%B3+W%C3%B6rld"
string(45) "http://example.com/?foo=Hell%C3%B3+W%C3%B6rld"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "015",
        source: r#"<?php

try {
    $reflectionClass = new ReflectionClass(Uri\Rfc3986\Uri::class);
    $reflectionClass->newInstanceWithoutConstructor();
} catch (ReflectionException $e) {
    echo $e->getMessage() . "\n";
}

try {
    $reflectionClass = new ReflectionClass(Uri\WhatWg\Url::class);
    $reflectionClass->newInstanceWithoutConstructor();
} catch (ReflectionException $e) {
    echo $e->getMessage() . "\n";
}

"#,
        expected: r#"Class Uri\Rfc3986\Uri is an internal class marked as final that cannot be instantiated without invoking its constructor
Class Uri\WhatWg\Url is an internal class marked as final that cannot be instantiated without invoking its constructor
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "018",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = clone $uri1;
var_dump($uri1);
var_dump($uri2);

$url3 = Uri\WhatWg\Url::parse("https://example.com");
$url4 = clone $url3;
var_dump($url3);
var_dump($url4);

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#1 (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\Rfc3986\Uri)#2 (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#3 (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#4 (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "019",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("🐘");
var_dump($uri);

$errors = [];
$url = Uri\WhatWg\Url::parse("🐘", null, $errors);
var_dump($url);
var_dump($errors);

$uri = Uri\Rfc3986\Uri::parse("https://🐘.com/🐘?🐘=🐘");
var_dump($uri);

$url = Uri\WhatWg\Url::parse("https://🐘.com/🐘?🐘=🐘", null);
var_dump($url);
var_dump($url->getAsciiHost());
var_dump($url->getUnicodeHost());
var_dump($url->toAsciiString());
var_dump($url->toUnicodeString());

"#,
        expected: r#"NULL
NULL
array(1) {
  [0]=>
  object(Uri\WhatWg\UrlValidationError)#%d (%d) {
    ["context"]=>
    string(4) "🐘"
    ["type"]=>
    enum(Uri\WhatWg\UrlValidationErrorType::MissingSchemeNonRelativeUrl)
    ["failure"]=>
    bool(true)
  }
}
NULL
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(12) "xn--go8h.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(13) "/%F0%9F%90%98"
  ["query"]=>
  string(25) "%F0%9F%90%98=%F0%9F%90%98"
  ["fragment"]=>
  NULL
}
string(12) "xn--go8h.com"
string(8) "🐘.com"
string(59) "https://xn--go8h.com/%F0%9F%90%98?%F0%9F%90%98=%F0%9F%90%98"
string(55) "https://🐘.com/%F0%9F%90%98?%F0%9F%90%98=%F0%9F%90%98"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "021",
        source: r#"<?php

readonly class MyRfc3986Uri extends Uri\Rfc3986\Uri
{
}

"#,
        expected: r#"Fatal error: Class MyRfc3986Uri cannot extend final class Uri\Rfc3986\Uri in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: true,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "022",
        source: r#"<?php

readonly class MyWhatWgUri extends Uri\WhatWg\Url
{
}

"#,
        expected: r#"Fatal error: Class MyWhatWgUri cannot extend final class Uri\WhatWg\Url in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: true,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "023",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
var_dump($uri1->getRawScheme());
var_dump($uri1->getScheme());

$uri2 = $uri1->withScheme("http");
var_dump($uri2->getRawScheme());
var_dump($uri2->getScheme());

$uri3 = $uri2->withScheme(null);
var_dump($uri3->getRawScheme());
var_dump($uri3->getScheme());

try {
    $uri3->withScheme("");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

try {
    $uri3->withScheme("http%73");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withScheme("http");

var_dump($url1->getScheme());
var_dump($url2->getScheme());

try {
    $url2->withScheme("");
} catch (Uri\WhatWg\InvalidUrlException $e) {
    echo $e->getMessage() . "\n";
}

try {
    $url2->withScheme("http%73");
} catch (Uri\WhatWg\InvalidUrlException $e) {
    echo $e->getMessage() . "\n";
}

"#,
        expected: r#"string(5) "https"
string(5) "https"
string(4) "http"
string(4) "http"
NULL
NULL
The specified scheme is malformed
The specified scheme is malformed
string(5) "https"
string(4) "http"
The specified scheme is malformed
The specified scheme is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "024",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withUsername("user");
$url3 = $url2->withUsername(null);
$url4 = $url3->withUsername("%75s%2Fr"); // us/r
$url5 = $url4->withUsername("u:s/r");

$url6 = Uri\WhatWg\Url::parse("file:///foo/bar/");
$url6 = $url6->withUsername("user");

var_dump($url2->getUsername());
var_dump($url3->getUsername());
var_dump($url4->getUsername());
var_dump($url5->getUsername());
var_dump($url6->getUsername());

"#,
        expected: r#"string(4) "user"
NULL
string(8) "%75s%2Fr"
string(9) "u%3As%2Fr"
NULL
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "025",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withPassword("pass");
$url3 = $url2->withPassword(null);
$url4 = $url3->withPassword("p%61ss");
$url5 = $url4->withPassword("p:s/");

$url6 = Uri\WhatWg\Url::parse("file:///foo/bar/");
$url6 = $url6->withUsername("pass");

var_dump($url2->getPassword());
var_dump($url3->getPassword());
var_dump($url4->getPassword());
var_dump($url5->getPassword());
var_dump($url6->getPassword());

"#,
        expected: r#"string(4) "pass"
NULL
string(6) "p%61ss"
string(8) "p%3As%2F"
NULL
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "026",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
var_dump($uri1->getRawHost());
var_dump($uri1->getHost());

$uri2 = $uri1->withHost("test.com");
var_dump($uri2->getRawHost());
var_dump($uri2->getHost());

$uri3 = $uri2->withHost("t%65st.com"); // test.com
var_dump($uri3->getRawHost());
var_dump($uri3->getHost());

$uri4 = $uri3->withHost(null);
var_dump($uri4->getRawHost());
var_dump($uri4->getHost());

$uri5 = $uri4->withHost("192.168.0.1");
var_dump($uri5->getRawHost());
var_dump($uri5->getHost());

$uri6 = $uri5->withHost("[2001:db8:3333:4444:5555:6666:7777:8888]");
var_dump($uri6->getRawHost());
var_dump($uri6->getHost());

try {
    $uri3->withHost("test.com:8080");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

try {
    $uri3->withHost("t%3As%2Ft.com"); // t:s/t.com
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

try {
    $uri3->withHost("t:s/t.com");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

try {
    $uri2->withHost("");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

$uri1 = Uri\Rfc3986\Uri::parse("ftp://user:pass@foo.com?query=abc#foo");
$uri2 = $uri1->withHost("test.com");

var_dump($uri1->getHost());
var_dump($uri2->getHost());

try {
    $uri1->withHost(null);
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withHost("test.com");
$url3 = $url2->withHost("t%65st.com"); // test.com
$url4 = $url3->withHost("192.168.0.1");
$url5 = $url4->withHost("[2001:db8:3333:4444:5555:6666:7777:8888]");

var_dump($url1->getAsciiHost());
var_dump($url2->getAsciiHost());
var_dump($url3->getAsciiHost());
var_dump($url4->getAsciiHost());
var_dump($url5->getAsciiHost());

try {
    $url3->withHost("test.com:8080");
} catch (Uri\WhatWg\InvalidUrlException $e) {
    echo $e->getMessage() . "\n";
}

try {
    $url3->withHost("t%3As%2Ft.com"); // t:s/t.com
} catch (Uri\WhatWg\InvalidUrlException $e) {
    echo $e->getMessage() . "\n";
}

try {
    $url3->withHost("t:s/t.com");     // t:s/t.com
} catch (Uri\WhatWg\InvalidUrlException $e) {
    echo $e->getMessage() . "\n";
}

try {
    $url2->withHost(null);
} catch (Uri\WhatWg\InvalidUrlException $e) {
    echo $e->getMessage() . "\n";
}

$url1 = Uri\WhatWg\Url::parse("ftp://foo.com?query=abc#foo");
$url2 = $url1->withHost("test.com");

var_dump($url1->getAsciiHost());
var_dump($url2->getAsciiHost());

"#,
        expected: r#"string(11) "example.com"
string(11) "example.com"
string(8) "test.com"
string(8) "test.com"
string(10) "t%65st.com"
string(8) "test.com"
NULL
NULL
string(11) "192.168.0.1"
string(11) "192.168.0.1"
string(40) "[2001:db8:3333:4444:5555:6666:7777:8888]"
string(40) "[2001:db8:3333:4444:5555:6666:7777:8888]"
The specified host is malformed
The specified host is malformed
string(7) "foo.com"
string(8) "test.com"
Cannot remove the host from a URI that has a userinfo
string(11) "example.com"
string(8) "test.com"
string(8) "test.com"
string(11) "192.168.0.1"
string(40) "[2001:db8:3333:4444:5555:6666:7777:8888]"
The specified host is malformed
The specified host is malformed (DomainInvalidCodePoint)
The specified host is malformed
The specified host is malformed (HostMissing)
string(7) "foo.com"
string(8) "test.com"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "026_userinfo",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
var_dump($uri1->getRawUserInfo());
var_dump($uri1->getUserInfo());

$uri2 = $uri1->withUserInfo("user");
var_dump($uri2->getRawUserInfo());
var_dump($uri2->getUserInfo());

$uri3 = $uri2->withUserInfo(null);
var_dump($uri3->getRawUserInfo());
var_dump($uri3->getUserInfo());

$uri4 = $uri3->withUserInfo("%75s%2Fr:pass"); // us/r:pass
var_dump($uri4->getRawUserInfo());
var_dump($uri4->getUserInfo());

$uri1 = Uri\Rfc3986\Uri::parse("/foo");
$uri2 = $uri1->withUserInfo(null);
var_dump($uri2->getPort());

try {
    $uri4->withUserInfo("u:s/r");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

$uri5 = Uri\Rfc3986\Uri::parse("file:///foo/bar/");
$uri6 = $uri5->withUserinfo("user:pass");

var_dump($uri5->getUserInfo());
var_dump($uri6->getUserInfo());

"#,
        expected: r#"NULL
NULL
string(4) "user"
string(4) "user"
NULL
NULL
string(13) "%75s%2Fr:pass"
string(11) "us%2Fr:pass"
NULL
The specified userinfo is malformed
NULL
string(9) "user:pass"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "027",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com:8080");
$uri2 = $uri1->withPort(22);
$uri3 = $uri2->withPort(null);

var_dump($uri1->getPort());
var_dump($uri2->getPort());
var_dump($uri3->getPort());

$uri1 = Uri\Rfc3986\Uri::parse("ftp://foo.com:443?query=abc#foo");
$uri2 = $uri1->withPort(8080);

var_dump($uri1->getPort());
var_dump($uri2->getPort());

$uri1 = Uri\Rfc3986\Uri::parse("file:///foo/bar");
$uri2 = $uri1->withPort(80);

var_dump($uri1->getPort());
var_dump($uri2->getPort());

$uri1 = Uri\Rfc3986\Uri::parse("/foo");
$uri2 = $uri1->withPort(null);
var_dump($uri2->getPort());

try {
    $uri1->withPort(1);
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

$url1 = Uri\WhatWg\Url::parse("https://example.com:8080");
$url2 = $url1->withPort(22);
$url3 = $url2->withPort(null);

var_dump($url1->getPort());
var_dump($url2->getPort());
var_dump($url3->getPort());

$url1 = Uri\WhatWg\Url::parse("ftp://foo.com:443?query=abc#foo");
$url2 = $url1->withPort(8080);

var_dump($url1->getPort());
var_dump($url2->getPort());

$url1 = Uri\WhatWg\Url::parse("file:///foo/bar");
$url2 = $url1->withPort(80);

var_dump($url1->getPort());
var_dump($url2->getPort());

"#,
        expected: r#"int(8080)
int(22)
NULL
int(443)
int(8080)
NULL
int(80)
NULL
Cannot set a port without having a host
int(8080)
int(22)
NULL
int(443)
int(8080)
NULL
NULL
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "028",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com/foo/bar/");
var_dump($uri1->getRawPath());
var_dump($uri1->getPath());

$uri2 = $uri1->withPath("/foo");
var_dump($uri2->getRawPath());
var_dump($uri2->getPath());

$uri3 = $uri2->withPath("/t%65st");
var_dump($uri3->getRawPath());
var_dump($uri3->getPath());

$uri4 = $uri3->withPath("/foo%2Fbar");
var_dump($uri4->getRawPath());
var_dump($uri4->getPath());

$uri5 = $uri4->withPath("");
var_dump($uri5->getRawPath());
var_dump($uri5->getPath());

try {
    $uri5->withPath("test");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

try {
    $uri5->withPath("/#");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

$uri1 = Uri\Rfc3986\Uri::parse("/foo");
$uri2 = $uri1->withPath("bar");

var_dump($uri1->getPath());
var_dump($uri2->getPath());

$url1 = Uri\WhatWg\Url::parse("https://example.com/foo/bar/");
$url2 = $url1->withPath("/foo");
$url3 = $url2->withPath("");
$url4 = $url3->withPath("t%65st");
$url5 = $url4->withPath("/foo%2Fbar");
$url6 = $url5->withPath("/#");

var_dump($url1->getPath());
var_dump($url2->getPath());
var_dump($url3->getPath());
var_dump($url4->getPath());
var_dump($url5->getPath());
var_dump($url6->getPath());

$url1 = Uri\WhatWg\Url::parse("https://example.com/");
$uri2 = $url1->withPath("/foo");

var_dump($url1->getPath());
var_dump($url2->getPath());

"#,
        expected: r#"string(9) "/foo/bar/"
string(9) "/foo/bar/"
string(4) "/foo"
string(4) "/foo"
string(7) "/t%65st"
string(5) "/test"
string(10) "/foo%2Fbar"
string(10) "/foo%2Fbar"
string(0) ""
string(0) ""
The specified path is malformed
The specified path is malformed
string(4) "/foo"
string(3) "bar"
string(9) "/foo/bar/"
string(4) "/foo"
string(1) "/"
string(7) "/t%65st"
string(10) "/foo%2Fbar"
string(4) "/%23"
string(1) "/"
string(4) "/foo"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "029",
        source: r##"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com?foo=bar");
var_dump($uri1->getRawQuery());
var_dump($uri1->getQuery());

$uri2 = $uri1->withQuery("foo=baz");
var_dump($uri2->getRawQuery());
var_dump($uri2->getQuery());

$uri3 = $uri2->withQuery(null);
var_dump($uri3->getRawQuery());
var_dump($uri3->getQuery());

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
var_dump($uri1->getRawQuery());
var_dump($uri1->getQuery());

$uri2 = $uri1->withQuery("?foo=bar&foo=baz");
var_dump($uri2->getRawQuery());
var_dump($uri2->getQuery());

$uri3 = $uri1->withQuery("foo=bar&foo=baz");
var_dump($uri3->getRawQuery());
var_dump($uri3->getQuery());

$uri4 = $uri3->withQuery("t%65st");
var_dump($uri4->getRawQuery());
var_dump($uri4->getQuery());

$uri5 = $uri4->withQuery("foo=foo%26bar&baz=/qux%3D");
var_dump($uri5->getRawQuery());
var_dump($uri5->getQuery());

try {
    $uri5->withQuery("#");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

$url1 = Uri\WhatWg\Url::parse("https://example.com?foo=bar");
$url2 = $url1->withQuery("?foo=baz");
$url3 = $url2->withQuery(null);

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url3->getQuery());

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withQuery("?foo=bar&foo=baz");
$url3 = $url1->withQuery("foo=bar&foo=baz");
$url4 = $url3->withQuery("t%65st");
$url5 = $url4->withQuery("foo=foo%26bar&baz=/qux%3D");
$url6 = $url5->withQuery("#");

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url3->getQuery());
var_dump($url4->getQuery());
var_dump($url5->getQuery());
var_dump($url6->getQuery());

"##,
        expected: r#"string(7) "foo=bar"
string(7) "foo=bar"
string(7) "foo=baz"
string(7) "foo=baz"
NULL
NULL
NULL
NULL
string(16) "?foo=bar&foo=baz"
string(16) "?foo=bar&foo=baz"
string(15) "foo=bar&foo=baz"
string(15) "foo=bar&foo=baz"
string(6) "t%65st"
string(4) "test"
string(25) "foo=foo%26bar&baz=/qux%3D"
string(25) "foo=foo%26bar&baz=/qux%3D"
The specified query is malformed
string(7) "foo=bar"
string(7) "foo=baz"
NULL
NULL
string(15) "foo=bar&foo=baz"
string(15) "foo=bar&foo=baz"
string(6) "t%65st"
string(25) "foo=foo%26bar&baz=/qux%3D"
string(3) "%23"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "030",
        source: r##"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com#fragment1");
var_dump($uri1->getRawFragment());
var_dump($uri1->getFragment());

$uri2 = $uri1->withFragment("fragment2");
var_dump($uri2->getRawFragment());
var_dump($uri2->getFragment());

$uri3 = $uri2->withFragment(null);
var_dump($uri3->getRawFragment());
var_dump($uri3->getFragment());

try {
    $uri3->withFragment(" ");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

try {
    $uri1->withFragment("#fragment2");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com?abc=def");
$uri2 = $uri1->withFragment("fragment");

var_dump($uri1->getFragment());
var_dump($uri2->getFragment());

$url1 = Uri\WhatWg\Url::parse("https://example.com#fragment1");
$url2 = $url1->withFragment("#fragment2");
$url3 = $url2->withFragment(null);
$url4 = $url3->withFragment(" ");

var_dump($url1->getFragment());
var_dump($url2->getFragment());
var_dump($url3->getFragment());
var_dump($url4->getFragment());

$url1 = Uri\WhatWg\Url::parse("https://example.com?abc=def");
$url2 = $url1->withFragment("#fragment");

var_dump($url1->getFragment());
var_dump($url2->getFragment());

"##,
        expected: r#"string(9) "fragment1"
string(9) "fragment1"
string(9) "fragment2"
string(9) "fragment2"
NULL
NULL
The specified fragment is malformed
The specified fragment is malformed
NULL
string(8) "fragment"
string(9) "fragment1"
string(9) "fragment2"
NULL
string(3) "%20"
NULL
string(8) "fragment"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "031",
        source: r#"<?php

$uri1 = new Uri\Rfc3986\Uri("https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists");
$serializedUri1 = serialize($uri1);
$uri2 = unserialize($serializedUri1);

var_dump($serializedUri1);
var_dump($uri2);

try {
    unserialize('O:15:"Uri\Rfc3986\Uri":1:{i:0;a:0:{}}'); // less than 2 items
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:15:"Uri\Rfc3986\Uri":3:{i:0;a:0:{}i:1;a:0:{}i:2;a:0:{}}'); // more than 2 items
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:15:"Uri\Rfc3986\Uri":2:{i:0;N;i:1;a:0:{}}'); // first item is not an array
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:15:"Uri\Rfc3986\Uri":2:{i:0;a:0:{}i:1;a:0:{}}'); // first array is empty
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:15:"Uri\Rfc3986\Uri":2:{i:0;a:1:{s:3:"uri";i:1;}i:1;a:0:{}}'); // "uri" key in first array is not a string
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:15:"Uri\Rfc3986\Uri":2:{i:0;a:1:{s:3:"uri";s:2:"%1";}i:1;a:0:{}}'); // "uri" key in first array contains invalid URI
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:15:"Uri\Rfc3986\Uri":2:{i:0;a:1:{s:3:"uri";s:4:"/uri";}i:1;s:0:"";}'); // second item in not an array
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:15:"Uri\Rfc3986\Uri":2:{i:0;a:1:{s:3:"uri";s:4:"/uri";}i:1;a:1:{s:5:"prop1";i:123;}}'); // second array contains a property
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

$url1 = new Uri\WhatWg\Url("https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists");
$serializedUrl1 = serialize($url1);
$url2 = unserialize($serializedUrl1);

var_dump($serializedUrl1);
var_dump($url2);

try {
    unserialize('O:14:"Uri\WhatWg\Url":1:{i:0;a:0:{}}'); // less than 2 items
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:14:"Uri\WhatWg\Url":3:{i:0;a:0:{}i:1;a:0:{}i:2;a:0:{}}'); // more than 2 items
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:14:"Uri\WhatWg\Url":2:{i:0;N;i:1;a:0:{}}'); // first item is not an array
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:14:"Uri\WhatWg\Url":2:{i:0;a:0:{}i:1;a:0:{}}'); // first array is empty
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:14:"Uri\WhatWg\Url":2:{i:0;a:2:{s:3:"uri";s:19:"https://example.com";s:1:"a";i:1;}i:1;a:0:{}}'); // "uri" key in first array contains more than 1 item
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:14:"Uri\WhatWg\Url":2:{i:0;a:1:{s:3:"uri";i:1;}i:1;a:0:{}}'); // "uri" key in first array is not a string
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:14:"Uri\WhatWg\Url":2:{i:0;a:1:{s:3:"uri";s:11:"invalid-url";}i:1;a:0:{}}'); // "uri" key in first array contains invalid URL
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:14:"Uri\WhatWg\Url":2:{i:0;a:1:{s:3:"uri";s:19:"https://example.com";}i:1;s:0:"";}'); // second item in not an array
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

try {
    unserialize('O:14:"Uri\WhatWg\Url":2:{i:0;a:1:{s:3:"uri";s:19:"https://example.com";}i:1;a:1:{s:5:"prop1";i:123;}}'); // second array contains property
} catch (Exception $e) {
    echo $e->getMessage() . "\n";
}

"#,
        expected: r#"string(163) "O:15:"Uri\Rfc3986\Uri":2:{i:0;a:1:{s:3:"uri";s:98:"https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists";}i:1;a:0:{}}"
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(8) "username"
  ["password"]=>
  string(8) "password"
  ["host"]=>
  string(14) "www.google.com"
  ["port"]=>
  int(8080)
  ["path"]=>
  string(30) "/pathname1/pathname2/pathname3"
  ["query"]=>
  string(10) "query=true"
  ["fragment"]=>
  string(11) "hash-exists"
}
Invalid serialization data for Uri\Rfc3986\Uri object
Invalid serialization data for Uri\Rfc3986\Uri object
Invalid serialization data for Uri\Rfc3986\Uri object
Invalid serialization data for Uri\Rfc3986\Uri object
Invalid serialization data for Uri\Rfc3986\Uri object
Invalid serialization data for Uri\Rfc3986\Uri object
Invalid serialization data for Uri\Rfc3986\Uri object
Invalid serialization data for Uri\Rfc3986\Uri object
string(162) "O:14:"Uri\WhatWg\Url":2:{i:0;a:1:{s:3:"uri";s:98:"https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists";}i:1;a:0:{}}"
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(8) "username"
  ["password"]=>
  string(8) "password"
  ["host"]=>
  string(14) "www.google.com"
  ["port"]=>
  int(8080)
  ["path"]=>
  string(30) "/pathname1/pathname2/pathname3"
  ["query"]=>
  string(10) "query=true"
  ["fragment"]=>
  string(11) "hash-exists"
}
Invalid serialization data for Uri\WhatWg\Url object
Invalid serialization data for Uri\WhatWg\Url object
Invalid serialization data for Uri\WhatWg\Url object
Invalid serialization data for Uri\WhatWg\Url object
Invalid serialization data for Uri\WhatWg\Url object
Invalid serialization data for Uri\WhatWg\Url object
Invalid serialization data for Uri\WhatWg\Url object
Invalid serialization data for Uri\WhatWg\Url object
Invalid serialization data for Uri\WhatWg\Url object
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "032",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists");
var_dump(json_encode($uri));

$url = new Uri\WhatWg\Url("https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists");
var_dump(json_encode($url));

"#,
        expected: r#"string(2) "{}"
string(2) "{}"
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "033",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists");
var_export($uri);
echo "\n";

$url = new Uri\WhatWg\Url("https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists");
var_export($url);
echo "\n";

"#,
        expected: r#"\Uri\Rfc3986\Uri::__set_state(array(
))
\Uri\WhatWg\Url::__set_state(array(
))
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "034",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists");
var_dump((array) $uri);

$url = new Uri\WhatWg\Url("https://username:password@www.google.com:8080/pathname1/pathname2/pathname3?query=true#hash-exists");
var_dump((array) $url);

"#,
        expected: r#"array(%d) {
}
array(%d) {
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "038",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("HTTPS://////EXAMPLE.com");
$uri2 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri3 = Uri\Rfc3986\Uri::parse("https://example.com/foo/..");
var_dump($uri1->toRawString());
var_dump($uri1->toString());
var_dump($uri2->toRawString());
var_dump($uri2->toString());
var_dump($uri3->toRawString());
var_dump($uri3->toString());

$url1 = Uri\WhatWg\Url::parse("HTTPS://////EXAMPLE.com");
$url2 = Uri\WhatWg\Url::parse("https://example.com");
$url3 = Uri\WhatWg\Url::parse("https://example.com/foo/..");

var_dump($url1->toUnicodeString());
var_dump($url1->toAsciiString());
var_dump($url2->toUnicodeString());
var_dump($url2->toAsciiString());
var_dump($url3->toUnicodeString());
var_dump($url3->toAsciiString());

"#,
        expected: r#"string(23) "HTTPS://////EXAMPLE.com"
string(23) "https://////EXAMPLE.com"
string(19) "https://example.com"
string(19) "https://example.com"
string(26) "https://example.com/foo/.."
string(20) "https://example.com/"
string(20) "https://example.com/"
string(20) "https://example.com/"
string(20) "https://example.com/"
string(20) "https://example.com/"
string(20) "https://example.com/"
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "039",
        source: r#"<?php

function callRfc3986Getters($uri)
{
    var_dump($uri->getScheme());
    var_dump($uri->getRawScheme());
    var_dump($uri->getUsername());
    var_dump($uri->getRawUsername());
    var_dump($uri->getPassword());
    var_dump($uri->getRawPassword());
    var_dump($uri->getUserInfo());
    var_dump($uri->getRawUserInfo());
    var_dump($uri->getHost());
    var_dump($uri->getRawHost());
    var_dump($uri->getPort());
    var_dump($uri->getPath());
    var_dump($uri->getRawPath());
    var_dump($uri->getQuery());
    var_dump($uri->getRawQuery());
    var_dump($uri->getFragment());
    var_dump($uri->getRawFragment());
}

function callWhatWgGetters($url)
{
    var_dump($url->getScheme());
    var_dump($url->getUsername());
    var_dump($url->getPassword());
    var_dump($url->getAsciiHost());
    var_dump($url->getUnicodeHost());
    var_dump($url->getPort());
    var_dump($url->getPath());
    var_dump($url->getQuery());
    var_dump($url->getFragment());
}

$uri = Uri\Rfc3986\Uri::parse("http://%61pple:p%61ss@ex%61mple.com/foob%61r?%61bc=%61bc#%61bc");
callRfc3986Getters($uri);

echo "\n";

$url = Uri\WhatWg\Url::parse("http://%61pple:p%61ss@ex%61mple.com/foob%61r?%61bc=%61bc#%61bc");
callWhatWgGetters($url);

"#,
        expected: r#"string(4) "http"
string(4) "http"
string(5) "apple"
string(7) "%61pple"
string(4) "pass"
string(6) "p%61ss"
string(10) "apple:pass"
string(14) "%61pple:p%61ss"
string(11) "example.com"
string(13) "ex%61mple.com"
NULL
string(7) "/foobar"
string(9) "/foob%61r"
string(7) "abc=abc"
string(11) "%61bc=%61bc"
string(3) "abc"
string(5) "%61bc"

string(4) "http"
string(7) "%61pple"
string(6) "p%61ss"
string(11) "example.com"
string(11) "example.com"
NULL
string(9) "/foob%61r"
string(11) "%61bc=%61bc"
string(5) "%61bc"
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "040",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https:example.com");
var_dump($uri);
var_dump($uri->toRawString());

$url = Uri\WhatWg\Url::parse("https:example.com");
var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(11) "example.com"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(17) "https:example.com"
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(20) "https://example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "041",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("?query#fragment");
var_dump($uri);

$errors = [];
$url = Uri\WhatWg\Url::parse("?query#fragment", null, $errors);
var_dump($url);
var_dump($errors);

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  string(5) "query"
  ["fragment"]=>
  string(8) "fragment"
}
NULL
array(%d) {
  [0]=>
  object(Uri\WhatWg\UrlValidationError)#%d (%d) {
    ["context"]=>
    string(15) "?query#fragment"
    ["type"]=>
    enum(Uri\WhatWg\UrlValidationErrorType::MissingSchemeNonRelativeUrl)
    ["failure"]=>
    bool(true)
  }
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "042",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("urn:uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66");
var_dump($uri);

$url = Uri\WhatWg\Url::parse("urn:uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66");
var_dump($url);
"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(3) "urn"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(41) "uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(3) "urn"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(41) "uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "043",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com/without-base");
var_dump($uri);

$uri = Uri\Rfc3986\Uri::parse("/with-base", new Uri\Rfc3986\Uri("https://example.com"));
var_dump($uri);

$uri = Uri\Rfc3986\Uri::parse("https://test.com/with-base-in-vain", new Uri\Rfc3986\Uri("https://example.com"));
var_dump($uri);

$uri = Uri\Rfc3986\Uri::parse("HTTPS://TEST.com/with-base-in-vain", Uri\Rfc3986\Uri::parse("https://example.com"));
var_dump($uri);
var_dump($uri->toString());

$url = Uri\WhatWg\Url::parse("https://example.com/without-base/");
var_dump($url);

$url = Uri\WhatWg\Url::parse("/with-base", new Uri\WhatWg\Url("https://example.com"));
var_dump($url);

$url = Uri\WhatWg\Url::parse("https://test.com/with-base-in-vain", Uri\WhatWg\Url::parse("https://example.com/"));
var_dump($url);
"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(13) "/without-base"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(10) "/with-base"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(8) "test.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(18) "/with-base-in-vain"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "HTTPS"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(8) "TEST.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(18) "/with-base-in-vain"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(34) "https://test.com/with-base-in-vain"
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(14) "/without-base/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(10) "/with-base"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(8) "test.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(18) "/with-base-in-vain"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "045",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("https://example.com/foo/bar%2Fbaz");

var_dump($uri->toRawString());
var_dump($uri->getPath());
var_dump($uri->getRawPath());

$url = new Uri\Whatwg\Url("https://example.com/foo/bar%2Fbaz");

var_dump($url->toAsciiString());
var_dump($url->getPath());

"#,
        expected: r#"string(33) "https://example.com/foo/bar%2Fbaz"
string(14) "/foo/bar%2Fbaz"
string(14) "/foo/bar%2Fbaz"
string(33) "https://example.com/foo/bar%2Fbaz"
string(14) "/foo/bar%2Fbaz"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "046",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("mailto:johndoe@example.com");

var_dump($uri->toRawString());
var_dump($uri->getPath());
var_dump($uri->getRawPath());

$uri = new Uri\Rfc3986\Uri("foo/bar");
var_dump($uri->toRawString());
var_dump($uri->getPath());
var_dump($uri->getRawPath());

$uri = new Uri\Rfc3986\Uri("/foo/bar");
var_dump($uri->toRawString());
var_dump($uri->getPath());
var_dump($uri->getRawPath());

$uri = new Uri\Rfc3986\Uri("//");
var_dump($uri->toRawString());
var_dump($uri->getPath());
var_dump($uri->getRawPath());

$uri = new Uri\Rfc3986\Uri("///");
var_dump($uri->toRawString());
var_dump($uri->getPath());
var_dump($uri->getRawPath());

$uri = new Uri\Rfc3986\Uri("https://example.com");

var_dump($uri->toRawString());
var_dump($uri->getPath());
var_dump($uri->getRawPath());

$uri = new Uri\Rfc3986\Uri("https://example.com/");

var_dump($uri->toRawString());
var_dump($uri->getPath());
var_dump($uri->getRawPath());

$url = new Uri\Whatwg\Url("mailto:johndoe@example.com");

var_dump($url->toAsciiString());
var_dump($url->getPath());

$url = new Uri\Whatwg\Url("https://example.com");

var_dump($url->toAsciiString());
var_dump($url->getPath());

$url = new Uri\Whatwg\Url("https://example.com/");

var_dump($url->toAsciiString());
var_dump($url->getPath());

"#,
        expected: r#"string(26) "mailto:johndoe@example.com"
string(19) "johndoe@example.com"
string(19) "johndoe@example.com"
string(7) "foo/bar"
string(7) "foo/bar"
string(7) "foo/bar"
string(8) "/foo/bar"
string(8) "/foo/bar"
string(8) "/foo/bar"
string(2) "//"
string(0) ""
string(0) ""
string(3) "///"
string(1) "/"
string(1) "/"
string(19) "https://example.com"
string(0) ""
string(0) ""
string(20) "https://example.com/"
string(1) "/"
string(1) "/"
string(26) "mailto:johndoe@example.com"
string(19) "johndoe@example.com"
string(20) "https://example.com/"
string(1) "/"
string(20) "https://example.com/"
string(1) "/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "047",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("https://192.168.0.1");
var_dump($uri->getRawHost());
var_dump($uri->getHost());
var_dump($uri->toRawString());
var_dump($uri->toString());

$uri = new Uri\Rfc3986\Uri("https://[2001:0db8:0001:0000:0000:0ab9:C0A8:0102]");
var_dump($uri->getRawHost());
var_dump($uri->getHost());
var_dump($uri->toRawString());
var_dump($uri->toString());

$uri = new Uri\Rfc3986\Uri("//[v7.host]/source");
var_dump($uri->getRawHost());
var_dump($uri->getHost());
var_dump($uri->toRawString());
var_dump($uri->toString());

$url = new Uri\WhatWg\Url("https://192.168.0.1");
var_dump($url->getAsciiHost());
var_dump($url->toAsciiString());

$url = new Uri\WhatWg\Url("https://[2001:0db8:0001:0000:0000:0ab9:C0A8:0102]");
var_dump($url->getAsciiHost());
var_dump($url->toAsciiString());

$url = new Uri\WhatWg\Url("https://[0:0::1]");
var_dump($url->getAsciiHost());
var_dump($url->toAsciiString());

"#,
        expected: r#"string(11) "192.168.0.1"
string(11) "192.168.0.1"
string(19) "https://192.168.0.1"
string(19) "https://192.168.0.1"
string(41) "[2001:0db8:0001:0000:0000:0ab9:C0A8:0102]"
string(41) "[2001:0db8:0001:0000:0000:0ab9:c0a8:0102]"
string(49) "https://[2001:0db8:0001:0000:0000:0ab9:c0a8:0102]"
string(49) "https://[2001:0db8:0001:0000:0000:0ab9:c0a8:0102]"
string(9) "[v7.host]"
string(9) "[v7.host]"
string(18) "//[v7.host]/source"
string(18) "//[v7.host]/source"
string(11) "192.168.0.1"
string(20) "https://192.168.0.1/"
string(26) "[2001:db8:1::ab9:c0a8:102]"
string(35) "https://[2001:db8:1::ab9:c0a8:102]/"
string(5) "[::1]"
string(14) "https://[::1]/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "048",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://%e4%bd%a0%e5%a5%bd%e4%bd%a0%e5%a5%bd.com");         // percent-encoded form of https://你好你好.com
var_dump($uri->toString());

$uri = Uri\Rfc3986\Uri::parse("https://example.com/%e4%bd%a0%e5%a5%bd%e4%bd%a0%e5%a5%bd"); // percent-encoded form of https://example.com/你好你好.com
var_dump($uri->toString());

"#,
        expected: r#"string(48) "https://%E4%BD%A0%E5%A5%BD%E4%BD%A0%E5%A5%BD.com"
string(56) "https://example.com/%E4%BD%A0%E5%A5%BD%E4%BD%A0%E5%A5%BD"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "049",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("https://example.com/foo/bar%2Fbaz");
var_dump($uri->getRawPath());
var_dump($uri->getPath());

$url = new Uri\WhatWg\Url("https://example.com/foo/bar%2Fbaz");
var_dump($url->getPath());

"#,
        expected: r#"string(14) "/foo/bar%2Fbaz"
string(14) "/foo/bar%2Fbaz"
string(14) "/foo/bar%2Fbaz"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "050",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("https://example.com");

var_dump($uri->resolve("/foo/")->toString());
var_dump($uri->resolve("https://test.com/foo")->toString());

$url = new Uri\WhatWg\Url("https://example.com");

var_dump($url->resolve("/foo/")->toAsciiString());
var_dump($url->resolve("https://test.com/foo")->toAsciiString());

"#,
        expected: r#"string(24) "https://example.com/foo/"
string(20) "https://test.com/foo"
string(24) "https://example.com/foo/"
string(20) "https://test.com/foo"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "051",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("https://example.com");

try {
    $uri->resolve("á");
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}

$url = new Uri\WhatWg\Url("https://example.com");

try {
    $url->resolve("https://1.2.3.4.5");
} catch (Uri\WhatWg\InvalidUrlException $e) {
    echo $e->getMessage() . "\n";
}

$softErrors = [];

var_dump($url->resolve(" /foo", $softErrors)->toAsciiString());
var_dump($softErrors);

"#,
        expected: r#"The specified URI is malformed
The specified URI is malformed (Ipv4TooManyParts)
string(23) "https://example.com/foo"
array(%d) {
  [0]=>
  object(Uri\WhatWg\UrlValidationError)#%d (%d) {
    ["context"]=>
    string(5) " /foo"
    ["type"]=>
    enum(Uri\WhatWg\UrlValidationErrorType::InvalidUrlUnit)
    ["failure"]=>
    bool(false)
  }
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "052",
        source: r#"<?php

$r = new Uri\WhatWg\UrlValidationError('foo', Uri\WhatWg\UrlValidationErrorType::DomainInvalidCodePoint, true);

try {
    $r->__construct('bar', Uri\WhatWg\UrlValidationErrorType::HostMissing, false);
} catch (Error $e) {
    echo $e->getMessage() . "\n";
}

var_dump($r);

"#,
        expected: r#"Cannot modify readonly property Uri\WhatWg\UrlValidationError::$context
object(Uri\WhatWg\UrlValidationError)#%d (%d) {
  ["context"]=>
  string(3) "foo"
  ["type"]=>
  enum(Uri\WhatWg\UrlValidationErrorType::DomainInvalidCodePoint)
  ["failure"]=>
  bool(true)
}
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "053",
        source: r#"<?php

$r = new Uri\WhatWg\InvalidUrlException(
    "",
    [new Uri\WhatWg\UrlValidationError("abc", Uri\WhatWg\UrlValidationErrorType::DomainInvalidCodePoint, true)],
    1,
    new Exception()
);

try {
    $r->__construct("foo");
} catch (Error $e) {
    echo $e->getMessage() . "\n";
}

try {
    $r->__construct("bar", []);
} catch (Error $e) {
    echo $e->getMessage() . "\n";
}

try {
    $r->__construct("baz", [], 0);
} catch (Error $e) {
    echo $e->getMessage() . "\n";
}

try {
    $r->__construct("qax", [], 0, null);
} catch (Error $e) {
    echo $e->getMessage() . "\n";
}

var_dump($r->getMessage());
var_dump($r->errors);
var_dump($r->getCode());
var_dump($r->getPrevious()::class);

"#,
        expected: r#"Cannot modify readonly property Uri\WhatWg\InvalidUrlException::$errors
Cannot modify readonly property Uri\WhatWg\InvalidUrlException::$errors
Cannot modify readonly property Uri\WhatWg\InvalidUrlException::$errors
Cannot modify readonly property Uri\WhatWg\InvalidUrlException::$errors
string(3) "qax"
array(%d) {
  [%d]=>
  object(Uri\WhatWg\UrlValidationError)#%d (%d) {
    ["context"]=>
    string(3) "abc"
    ["type"]=>
    enum(Uri\WhatWg\UrlValidationErrorType::DomainInvalidCodePoint)
    ["failure"]=>
    bool(true)
  }
}
int(1)
string(9) "Exception"
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "054",
        source: r#"<?php

try {
    new \Uri\WhatWg\Url('http://localhost:99999');
} catch (Uri\WhatWg\InvalidUrlException $e) {
    echo $e->getMessage() . "\n";
    var_dump($e->errors[0]->type === \Uri\WhatWg\UrlValidationErrorType::PortOutOfRange);
}

"#,
        expected: r#"The specified URI is malformed (PortOutOfRange)
bool(true)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "055",
        source: r#"<?php

try {
    var_dump(new Uri\Rfc3986\Uri('foo', new Uri\Rfc3986\Uri('bar')));
} catch (Uri\InvalidUriException $e) {
    echo $e->getMessage() . "\n";
}
"#,
        expected: r#"The specified base URI must be absolute
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "056",
        source: r#"<?php

$urls = [];

for ($i = 0; $i < 1000; $i++) {
    $urls[] = Uri\WhatWg\Url::parse("https://example.com/{$i}/");
}

for ($i = 0; $i < 1000; $i++) {
    if ($urls[$i]->toAsciiString() !== "https://example.com/{$i}/") {
        exit(1);
    }
}

echo "==DONE==\n";
"#,
        expected: r#"==DONE==
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "057",
        source: r#"<?php

class Foo {
	public string $x = '';
}

$f = new Foo();
try {
    Uri\WhatWg\Url::parse(" https://example.org ", errors: $f->x);
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"TypeError: Cannot assign array to reference held by property Foo::$x of type string
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "058",
        source: r#"<?php

if (PHP_INT_SIZE == 8) {
    $uri = new \Uri\Rfc3986\Uri('https://example.com:9223372036854775807');
    echo $uri->getPort(), PHP_EOL;
    echo "2147483647", PHP_EOL;
} else {
    $uri = new \Uri\Rfc3986\Uri('https://example.com:2147483647');
    echo "9223372036854775807", PHP_EOL;
    echo $uri->getPort(), PHP_EOL;
}

try {
    if (PHP_INT_SIZE == 8) {
        new \Uri\Rfc3986\Uri('https://example.com:9223372036854775808');
    } else {
        new \Uri\Rfc3986\Uri('https://example.com:2147483648');
    }
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"9223372036854775807
2147483647
Uri\InvalidUriException: The port is out of range
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "059",
        source: r#"<?php

$uri = new \Uri\Rfc3986\Uri('https://example.com:');
var_dump($uri, $uri->getPort());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
NULL
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "065",
        source: r#"<?php

$uri = new Uri\WhatWg\Url('https://example.com');
try {
  $uri->__construct('ftp://example.org');
} catch (Throwable $e) {
  echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}
var_dump($uri);

$uri = new Uri\WhatWg\Url('https://example.com');
try {
  $uri->__unserialize([['uri' => 'ftp://example.org'], []]);
} catch (Throwable $e) {
  echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}
var_dump($uri);

$uri = new Uri\Rfc3986\Uri('https://example.com');
try {
  $uri->__construct('ftp://example.org');
} catch (Throwable $e) {
  echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}
var_dump($uri);

$uri = new Uri\Rfc3986\Uri('https://example.com');
try {
  $uri->__unserialize([['uri' => 'ftp://example.org'], []]);
} catch (Throwable $e) {
  echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}
var_dump($uri);

"#,
        expected: r#"Error: Cannot modify readonly object of class Uri\WhatWg\Url
object(Uri\WhatWg\Url)#%d (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
Exception: Invalid serialization data for Uri\WhatWg\Url object
object(Uri\WhatWg\Url)#%d (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
Error: Cannot modify readonly object of class Uri\Rfc3986\Uri
object(Uri\Rfc3986\Uri)#%d (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
Exception: Invalid serialization data for Uri\Rfc3986\Uri object
object(Uri\Rfc3986\Uri)#%d (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "100",
        source: r#"<?php

var_dump(zend_test_uri_parser("https://%65xample:%65xample@%65xample.com:123/%65xample.html?%65xample=%65xample#%65xample", "parse_url"));

"#,
        expected: r#"array(3) {
  ["normalized"]=>
  array(8) {
    ["scheme"]=>
    string(5) "https"
    ["username"]=>
    string(7) "example"
    ["password"]=>
    string(7) "example"
    ["host"]=>
    string(11) "example.com"
    ["port"]=>
    int(123)
    ["path"]=>
    string(13) "/example.html"
    ["query"]=>
    string(15) "example=example"
    ["fragment"]=>
    string(7) "example"
  }
  ["raw"]=>
  array(8) {
    ["scheme"]=>
    string(5) "https"
    ["username"]=>
    string(9) "%65xample"
    ["password"]=>
    string(9) "%65xample"
    ["host"]=>
    string(13) "%65xample.com"
    ["port"]=>
    int(123)
    ["path"]=>
    string(15) "/%65xample.html"
    ["query"]=>
    string(19) "%65xample=%65xample"
    ["fragment"]=>
    string(9) "%65xample"
  }
  ["struct"]=>
  array(8) {
    ["scheme"]=>
    string(5) "https"
    ["username"]=>
    string(9) "%65xample"
    ["password"]=>
    string(9) "%65xample"
    ["host"]=>
    string(13) "%65xample.com"
    ["port"]=>
    int(123)
    ["path"]=>
    string(15) "/%65xample.html"
    ["query"]=>
    string(19) "%65xample=%65xample"
    ["fragment"]=>
    string(9) "%65xample"
  }
}
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: true,
    },
    UriCase {
        name: "101",
        source: r#"<?php

try {
  var_dump(zend_test_uri_parser('invalid uri', "Uri\\WhatWg\\Url"));
} catch (\Uri\WhatWg\InvalidUrlException $e) {
  echo $e->getMessage(), PHP_EOL;
  var_dump($e->errors);
}

"#,
        expected: r#"The specified URI is malformed (MissingSchemeNonRelativeUrl)
array(1) {
  [0]=>
  object(Uri\WhatWg\UrlValidationError)#%d (3) {
    ["context"]=>
    string(11) "invalid uri"
    ["type"]=>
    enum(Uri\WhatWg\UrlValidationErrorType::MissingSchemeNonRelativeUrl)
    ["failure"]=>
    bool(true)
  }
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: true,
    },
    UriCase {
        name: "102",
        source: r#"<?php

var_dump(zend_test_uri_parser("https://example.com:42424242", "Uri\\Rfc3986\\Uri"));

"#,
        expected: r#"array(3) {
  ["normalized"]=>
  array(8) {
    ["scheme"]=>
    string(5) "https"
    ["username"]=>
    NULL
    ["password"]=>
    NULL
    ["host"]=>
    string(11) "example.com"
    ["port"]=>
    int(42424242)
    ["path"]=>
    string(0) ""
    ["query"]=>
    NULL
    ["fragment"]=>
    NULL
  }
  ["raw"]=>
  array(8) {
    ["scheme"]=>
    string(5) "https"
    ["username"]=>
    NULL
    ["password"]=>
    NULL
    ["host"]=>
    string(11) "example.com"
    ["port"]=>
    int(42424242)
    ["path"]=>
    string(0) ""
    ["query"]=>
    NULL
    ["fragment"]=>
    NULL
  }
  ["struct"]=>
  array(8) {
    ["scheme"]=>
    string(5) "https"
    ["username"]=>
    NULL
    ["password"]=>
    NULL
    ["host"]=>
    string(11) "example.com"
    ["port"]=>
    int(42424242)
    ["path"]=>
    string(0) ""
    ["query"]=>
    NULL
    ["fragment"]=>
    NULL
  }
}
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: true,
    },
    UriCase {
        name: "gh19780",
        source: r#"<?php

use Uri\WhatWg\InvalidUrlException;
use Uri\WhatWg\UrlValidationError;
use Uri\WhatWg\UrlValidationErrorType;

try {
    new InvalidUrlException('message', ['foo']);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

try {
    new InvalidUrlException('message', [
        1 => new UrlValidationError('context', UrlValidationErrorType::HostMissing, true)
    ]);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException::__construct(): Argument #2 ($errors) must be a list of Uri\WhatWg\UrlValidationError
Uri\WhatWg\InvalidUrlException::__construct(): Argument #2 ($errors) must be a list of Uri\WhatWg\UrlValidationError
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "gh19892",
        source: r#"<?php
$errors = null;

\Uri\WhatWg\Url::parse('https://example.com', errors: $errors);
var_dump($errors);

"#,
        expected: r#"array(0) {
}
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "gh19979",
        source: r#"<?php

$baseUrl = \Uri\WhatWg\Url::parse('https://example.com/path?query');
var_dump(\Uri\WhatWg\Url::parse('relative', $baseUrl));

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(9) "/relative"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "oom_rfc3986_component_read",
        source: r#"<?php
$uri = new Uri\Rfc3986\Uri("http://EXAMPLE.COM/" . str_repeat("a/../", 100000));

$uri->getPath();

echo "not reached\n";
"#,
        expected: r#"Fatal error: Allowed memory size of 8388608 bytes exhausted %S(tried to allocate %d bytes) in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: Some(8388608),
        zend_test: false,
    },
    UriCase {
        name: "oom_rfc3986_equals",
        source: r#"<?php
$a = new Uri\Rfc3986\Uri("http://EXAMPLE.COM/" . str_repeat("a/../", 60000));
$b = clone $a;

$a->equals($b);

echo "not reached\n";
"#,
        expected: r#"Fatal error: Allowed memory size of 8388608 bytes exhausted %S(tried to allocate %d bytes) in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: Some(8388608),
        zend_test: false,
    },
    UriCase {
        name: "oom_rfc3986_parse",
        source: r#"<?php
Uri\Rfc3986\Uri::parse(str_repeat("/", 1024 * 1024));

echo "not reached\n";
"#,
        expected: r#"Fatal error: Allowed memory size of 8388608 bytes exhausted %S(tried to allocate %d bytes) in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: Some(8388608),
        zend_test: false,
    },
    UriCase {
        name: "oom_rfc3986_path_segments",
        source: r#"<?php
new Uri\Rfc3986\Uri(str_repeat("/", 1024 * 1024));

echo "not reached\n";
"#,
        expected: r#"Fatal error: Allowed memory size of 8388608 bytes exhausted %S(tried to allocate %d bytes) in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: Some(8388608),
        zend_test: false,
    },
    UriCase {
        name: "oom_rfc3986_resolve",
        source: r#"<?php
$base = new Uri\Rfc3986\Uri("http://example.com/a/b");

$base->resolve(str_repeat("/", 1024 * 1024));

echo "not reached\n";
"#,
        expected: r#"Fatal error: Allowed memory size of 8388608 bytes exhausted %S(tried to allocate %d bytes) in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: Some(8388608),
        zend_test: false,
    },
    UriCase {
        name: "oom_rfc3986_to_string",
        source: r#"<?php
$uri = new Uri\Rfc3986\Uri("http://EXAMPLE.COM/" . str_repeat("a/../", 100000));

$uri->toString();

echo "not reached\n";
"#,
        expected: r#"Fatal error: Allowed memory size of 8388608 bytes exhausted %S(tried to allocate %d bytes) in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: Some(8388608),
        zend_test: false,
    },
    UriCase {
        name: "oom_rfc3986_with_path",
        source: r#"<?php
$uri = new Uri\Rfc3986\Uri("http://example.com/");

$uri->withPath("/" . str_repeat("a/", 300000));

echo "not reached\n";
"#,
        expected: r#"Fatal error: Allowed memory size of 8388608 bytes exhausted %S(tried to allocate %d bytes) in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: Some(8388608),
        zend_test: false,
    },
    UriCase {
        name: "oom_whatwg_idna_host",
        source: r#"<?php
/* A non-ASCII host goes through IDNA */
$host = str_repeat("\u{e4}", (int) (1.5 * 1024 * 1024));

Uri\WhatWg\Url::parse("http://" . $host . "/");

echo "not reached\n";
"#,
        expected: r#"Fatal error: Allowed memory size of 8388608 bytes exhausted %S(tried to allocate %d bytes) in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: Some(8388608),
        zend_test: false,
    },
    UriCase {
        name: "oom_whatwg_idna_host_write",
        source: r#"<?php
$host = str_repeat("\u{e4}", (int) (1.5 * 1024 * 1024));

$url = Uri\WhatWg\Url::parse("http://example.com/");
$url->withHost($host);

echo "not reached\n";
"#,
        expected: r#"Fatal error: Allowed memory size of 8388608 bytes exhausted %S(tried to allocate %d bytes) in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: Some(8388608),
        zend_test: false,
    },
    UriCase {
        name: "oom_whatwg_resolve",
        source: r#"<?php
/* The relative reference carries the host, so resolving it runs IDNA while
 * the base URL is still held. */
$base = Uri\WhatWg\Url::parse("http://example.com/");

$base->resolve("//" . str_repeat("\u{e4}", 1200 * 1024) . "/");

echo "not reached\n";
"#,
        expected: r#"Fatal error: Allowed memory size of 8388608 bytes exhausted %S(tried to allocate %d bytes) in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: Some(8388608),
        zend_test: false,
    },
    UriCase {
        name: "oom_whatwg_to_unicode_string",
        source: r#"<?php
$url = Uri\WhatWg\Url::parse("http://" . str_repeat("\u{e4}", 1200 * 1024) . "/");

$url->toUnicodeString();

echo "not reached\n";
"#,
        expected: r#"Fatal error: Allowed memory size of 8388608 bytes exhausted %S(tried to allocate %d bytes) in %s on line %d
"#,
        expectf: true,
        isolate: true,
        compile_failure: false,
        heap_size: Some(8388608),
        zend_test: false,
    },
    UriCase {
        // php-src exhausts memory while building the soft-error list for this input. elephc
        // keeps the parse linear and only materializes the list the caller can observe.
        name: "whatwg_validation_errors_without_soft_errors",
        source: r#"<?php
$url = new Uri\WhatWg\Url("https://example.com/" . str_repeat("%", 100 * 1024));

echo $url->getAsciiHost(), "|parsed\n";
"#,
        expected: r#"example.com|parsed
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/equivalence/equals_false_host",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$uri2 = Uri\Rfc3986\Uri::parse("https://user:info@192.168.0.1:443/foo/bar?abc=123&def=ghi#hashmark");

var_dump($uri1->equals($uri2));
var_dump($uri2->equals($uri1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/equivalence/equals_false_path",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$uri2 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar/baz?abc=123&def=ghi#hashmark");

var_dump($uri1->equals($uri2));
var_dump($uri2->equals($uri1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/equivalence/equals_false_port",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$uri2 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:442/foo/bar?abc=123&def=ghi#hashmark");

var_dump($uri1->equals($uri2));
var_dump($uri2->equals($uri1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/equivalence/equals_false_query",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$uri2 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123#hashmark");

var_dump($uri1->equals($uri2));
var_dump($uri2->equals($uri1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/equivalence/equals_false_query2",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$uri2 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?ABC=123&def=ghi#hashmark");

var_dump($uri1->equals($uri2));
var_dump($uri2->equals($uri1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/equivalence/equals_false_scheme",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$uri2 = Uri\Rfc3986\Uri::parse("http://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");

var_dump($uri1->equals($uri2));
var_dump($uri2->equals($uri1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/equivalence/equals_false_userinfo",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$uri2 = Uri\Rfc3986\Uri::parse("https://user@example.com:443/foo/bar?abc=123&def=ghi#hashmark");

var_dump($uri1->equals($uri2));
var_dump($uri2->equals($uri1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/equivalence/equals_fragment_different",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$uri2 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hash");

var_dump($uri1->equals($uri2, Uri\UriComparisonMode::IncludeFragment));
var_dump($uri2->equals($uri1, Uri\UriComparisonMode::IncludeFragment));

var_dump($uri1->equals($uri2, Uri\UriComparisonMode::ExcludeFragment));
var_dump($uri2->equals($uri1, Uri\UriComparisonMode::ExcludeFragment));

"#,
        expected: r#"bool(false)
bool(false)
bool(true)
bool(true)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/equivalence/equals_true_identical",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$uri2 = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");

var_dump($uri1->equals($uri2, Uri\UriComparisonMode::IncludeFragment));
var_dump($uri2->equals($uri1, Uri\UriComparisonMode::IncludeFragment));

var_dump($uri1->equals($uri2, Uri\UriComparisonMode::ExcludeFragment));
var_dump($uri2->equals($uri1, Uri\UriComparisonMode::ExcludeFragment));

"#,
        expected: r#"bool(true)
bool(true)
bool(true)
bool(true)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/equivalence/equals_true_normalization1",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://user:info@example%2ecom:443/foo/bar?abc=123&def=ghi#hashmark");
$uri2 = Uri\Rfc3986\Uri::parse("HTTPS://user:info@EXAMPLE.COM:0443/../foo/b%61r?abc=123&def=ghi#hashmark");

var_dump($uri1->equals($uri2));
var_dump($uri2->equals($uri1));

"#,
        expected: r#"bool(true)
bool(true)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/equivalence/equals_true_normalization2",
        source: r#"<?php

$uri1 = new Uri\Rfc3986\Uri("https://[2001:0db8:0001:0000:0000:0ab9:C0A8:0102]");
$uri2 = new Uri\Rfc3986\Uri("https://[2001:DB8:1::AB9:C0A8:102]");

var_dump($uri1->equals($uri2));
var_dump($uri2->equals($uri1));

"#,
        expected: r#"bool(true)
bool(true)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/fragment_error_reserved",
        source: r##"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withFragment("#fragment");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"##,
        expected: r#"Uri\InvalidUriException: The specified fragment is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/fragment_error_unicode",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withFragment("ő");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified fragment is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/fragment_success_empty",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withFragment("");

var_dump($uri1->getRawFragment());
var_dump($uri2->getRawFragment());
var_dump($uri2->toRawString());
var_dump($uri2->getFragment());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
string(0) ""
string(20) "https://example.com#"
string(0) ""
string(20) "https://example.com#"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/fragment_success_encoded",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withFragment("foo%3db%61r"); // foo=bar

var_dump($uri1->getRawFragment());
var_dump($uri2->getRawFragment());
var_dump($uri2->toRawString());
var_dump($uri2->getFragment());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
string(11) "foo%3db%61r"
string(31) "https://example.com#foo%3db%61r"
string(9) "foo%3Dbar"
string(29) "https://example.com#foo%3Dbar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/fragment_success_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com#foo");
$uri2 = $uri1->withFragment("bar");

var_dump($uri1->getRawFragment());
var_dump($uri2->getRawFragment());
var_dump($uri2->toRawString());
var_dump($uri2->getFragment());
var_dump($uri2->toString());

"#,
        expected: r#"string(3) "foo"
string(3) "bar"
string(23) "https://example.com#bar"
string(3) "bar"
string(23) "https://example.com#bar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/fragment_success_unset_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com#foo");
$uri2 = $uri1->withFragment(null);

var_dump($uri1->getRawFragment());
var_dump($uri2->getRawFragment());
var_dump($uri2->toRawString());
var_dump($uri2->getFragment());
var_dump($uri2->toString());

"#,
        expected: r#"string(3) "foo"
NULL
string(19) "https://example.com"
NULL
string(19) "https://example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/fragment_success_unset_non_existent",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withFragment(null);

var_dump($uri1->getRawFragment());
var_dump($uri2->getRawFragment());
var_dump($uri2->toRawString());
var_dump($uri2->getFragment());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
NULL
string(19) "https://example.com"
NULL
string(19) "https://example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/host_error_reserved",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withHost("ex#mple.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified host is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/host_success_empty",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withHost("");

var_dump($uri1->getRawHost());
var_dump($uri2->getRawHost());
var_dump($uri2->toRawString());
var_dump($uri2->getHost());
var_dump($uri2->toString());

"#,
        expected: r#"string(11) "example.com"
string(0) ""
string(8) "https://"
string(0) ""
string(8) "https://"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/host_success_encoded",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withHost("%65xample.net"); // example.net

var_dump($uri1->getRawHost());
var_dump($uri2->getRawHost());
var_dump($uri2->toRawString());
var_dump($uri2->getHost());
var_dump($uri2->toString());

"#,
        expected: r#"string(11) "example.com"
string(13) "%65xample.net"
string(21) "https://%65xample.net"
string(11) "example.net"
string(19) "https://example.net"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/host_success_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withHost("example.net");

var_dump($uri1->getRawHost());
var_dump($uri2->getRawHost());
var_dump($uri2->toRawString());
var_dump($uri2->getHost());
var_dump($uri2->toString());

"#,
        expected: r#"string(11) "example.com"
string(11) "example.net"
string(19) "https://example.net"
string(11) "example.net"
string(19) "https://example.net"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/host_success_ip_future",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withHost("[vF.addr]");

var_dump($uri1->getRawHost());
var_dump($uri2->getRawHost());
var_dump($uri2->toRawString());
var_dump($uri2->getHost());
var_dump($uri2->toString());

"#,
        expected: r#"string(11) "example.com"
string(9) "[vF.addr]"
string(17) "https://[vF.addr]"
string(9) "[vf.addr]"
string(17) "https://[vf.addr]"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/host_success_ipv4",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withHost("192.168.0.1");

var_dump($uri1->getRawHost());
var_dump($uri2->getRawHost());
var_dump($uri2->toRawString());
var_dump($uri2->getHost());
var_dump($uri2->toString());

"#,
        expected: r#"string(11) "example.com"
string(11) "192.168.0.1"
string(19) "https://192.168.0.1"
string(11) "192.168.0.1"
string(19) "https://192.168.0.1"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/host_success_ipv6",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withHost("[2001:0db8:3333:4444:5555:6666:7777:8888]");

var_dump($uri1->getRawHost());
var_dump($uri2->getRawHost());
var_dump($uri2->toRawString());
var_dump($uri2->getHost());
var_dump($uri2->toString());

"#,
        expected: r#"string(11) "example.com"
string(41) "[2001:0db8:3333:4444:5555:6666:7777:8888]"
string(49) "https://[2001:0db8:3333:4444:5555:6666:7777:8888]"
string(41) "[2001:0db8:3333:4444:5555:6666:7777:8888]"
string(49) "https://[2001:0db8:3333:4444:5555:6666:7777:8888]"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/host_success_new",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("/foo/bar");
$uri2 = $uri1->withHost("example.com");

var_dump($uri1->getRawHost());
var_dump($uri2->getRawHost());
var_dump($uri2->toRawString());
var_dump($uri2->getHost());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
string(11) "example.com"
string(21) "//example.com/foo/bar"
string(11) "example.com"
string(21) "//example.com/foo/bar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/host_success_unset_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withHost(null);

var_dump($uri1->getRawHost());
var_dump($uri2->getRawHost());
var_dump($uri2->toRawString());
var_dump($uri2->getHost());
var_dump($uri2->toString());

"#,
        expected: r#"string(11) "example.com"
NULL
string(6) "https:"
NULL
string(6) "https:"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/host_success_unset_non_existent",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("/foo/bar");
$uri2 = $uri1->withHost(null);

var_dump($uri1->getRawHost());
var_dump($uri2->getRawHost());
var_dump($uri2->toRawString());
var_dump($uri2->getHost());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
NULL
string(8) "/foo/bar"
NULL
string(8) "/foo/bar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/path_error_null_byte",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withPath("/\0foo");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified path is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/path_error_reserved",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withPath("/[foo]");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified path is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/path_error_unicode",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withPath("/ő");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified path is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/path_error_without_leading_slash",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withPath("foo");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified path is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/path_success_email",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("");
$uri2 = $uri1->withPath("john.doe@example.com");

var_dump($uri1->getRawPath());
var_dump($uri2->getRawPath());
var_dump($uri2->toRawString());
var_dump($uri2->getPath());
var_dump($uri2->toString());

"#,
        expected: r#"string(0) ""
string(20) "john.doe@example.com"
string(20) "john.doe@example.com"
string(20) "john.doe@example.com"
string(20) "john.doe@example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/path_success_empty",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withPath("");

var_dump($uri1->getRawPath());
var_dump($uri2->getRawPath());
var_dump($uri2->toRawString());
var_dump($uri2->getPath());
var_dump($uri2->toString());

"#,
        expected: r#"string(0) ""
string(0) ""
string(19) "https://example.com"
string(0) ""
string(19) "https://example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/path_success_encoded",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withPath("/foo%2Fb%61r"); // /foo/bar

var_dump($uri1->getRawPath());
var_dump($uri2->getRawPath());
var_dump($uri2->toRawString());
var_dump($uri2->getPath());
var_dump($uri2->toString());

"#,
        expected: r#"string(0) ""
string(12) "/foo%2Fb%61r"
string(31) "https://example.com/foo%2Fb%61r"
string(10) "/foo%2Fbar"
string(29) "https://example.com/foo%2Fbar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/path_success_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com/foo/bar");
$uri2 = $uri1->withPath("/baz");

var_dump($uri1->getRawPath());
var_dump($uri2->getRawPath());
var_dump($uri2->toRawString());
var_dump($uri2->getPath());
var_dump($uri2->toString());

"#,
        expected: r#"string(8) "/foo/bar"
string(4) "/baz"
string(23) "https://example.com/baz"
string(4) "/baz"
string(23) "https://example.com/baz"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/port_error_negative",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withPort(-1);
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified port is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/port_success_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com:80");
$uri2 = $uri1->withPort(443);

var_dump($uri1->getPort());
var_dump($uri2->getPort());
var_dump($uri2->toRawString());

"#,
        expected: r#"int(80)
int(443)
string(23) "https://example.com:443"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/port_success_new",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withPort(443);

var_dump($uri1->getPort());
var_dump($uri2->getPort());
var_dump($uri2->toRawString());

"#,
        expected: r#"NULL
int(443)
string(23) "https://example.com:443"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/port_success_unset_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com:80");
$uri2 = $uri1->withPort(null);

var_dump($uri1->getPort());
var_dump($uri2->getPort());
var_dump($uri2->toRawString());

"#,
        expected: r#"int(80)
NULL
string(19) "https://example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/port_success_unset_non_existent",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("ftp://example.com");
$uri2 = $uri1->withPort(null);

var_dump($uri2->getPort());
var_dump($uri2->getPort());
var_dump($uri2->toRawString());

"#,
        expected: r#"NULL
NULL
string(17) "ftp://example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/query_error_reserved",
        source: r##"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withQuery("#foo");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"##,
        expected: r#"Uri\InvalidUriException: The specified query is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/query_error_unicode",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withQuery("ő");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified query is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/query_success_context_sensitive_reserved",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withQuery("?foo=bar");

var_dump($uri1->getRawQuery());
var_dump($uri2->getRawQuery());
var_dump($uri2->toRawString());
var_dump($uri2->getQuery());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
string(8) "?foo=bar"
string(28) "https://example.com??foo=bar"
string(8) "?foo=bar"
string(28) "https://example.com??foo=bar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/query_success_empty",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withQuery("");

var_dump($uri1->getRawQuery());
var_dump($uri2->getRawQuery());
var_dump($uri2->toRawString());
var_dump($uri2->getQuery());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
string(0) ""
string(20) "https://example.com?"
string(0) ""
string(20) "https://example.com?"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/query_success_encoded",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withQuery("foo%3dbar"); // foo=bar

var_dump($uri1->getRawQuery());
var_dump($uri2->getRawQuery());
var_dump($uri2->toRawString());
var_dump($uri2->getQuery());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
string(9) "foo%3dbar"
string(29) "https://example.com?foo%3dbar"
string(9) "foo%3Dbar"
string(29) "https://example.com?foo%3Dbar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/query_success_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com?foo=bar");
$uri2 = $uri1->withQuery("foo=bar&baz=qux");

var_dump($uri1->getRawQuery());
var_dump($uri2->getRawQuery());
var_dump($uri2->toRawString());
var_dump($uri2->getQuery());
var_dump($uri2->toString());

"#,
        expected: r#"string(7) "foo=bar"
string(15) "foo=bar&baz=qux"
string(35) "https://example.com?foo=bar&baz=qux"
string(15) "foo=bar&baz=qux"
string(35) "https://example.com?foo=bar&baz=qux"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/query_success_unset_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com?foo=bar");
$uri2 = $uri1->withQuery(null);

var_dump($uri1->getRawQuery());
var_dump($uri2->getRawQuery());
var_dump($uri2->toRawString());
var_dump($uri2->getQuery());
var_dump($uri2->toString());

"#,
        expected: r#"string(7) "foo=bar"
NULL
string(19) "https://example.com"
NULL
string(19) "https://example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/query_success_unset_non_existent",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withQuery(null);

var_dump($uri1->getRawQuery());
var_dump($uri2->getRawQuery());
var_dump($uri2->toRawString());
var_dump($uri2->getQuery());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
NULL
string(19) "https://example.com"
NULL
string(19) "https://example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/roundtrip_special_case1",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("path1");
$uri2 = $uri1->withHost("host");
$uri2 = $uri2->withHost(null);

var_dump($uri1->getRawPath());
var_dump($uri2->getRawPath());
var_dump($uri2->toRawString());
var_dump($uri2->getPath());
var_dump($uri2->toString());

"#,
        expected: r#"string(5) "path1"
string(6) "/path1"
string(6) "/path1"
string(6) "/path1"
string(6) "/path1"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/roundtrip_special_case2",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("scheme:path1:");
$uri2 = $uri1->withScheme(null);
$uri2 = $uri2->withScheme("scheme");

var_dump($uri1->toRawString());
var_dump($uri2->toRawString());
var_dump($uri2->toString());

"#,
        expected: r#"string(13) "scheme:path1:"
string(15) "scheme:./path1:"
string(13) "scheme:path1:"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/roundtrip_special_case3",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("//host//path");
$uri2 = $uri1->withHost(null);
$uri2 = $uri2->withHost("host");

var_dump($uri1->toRawString());
var_dump($uri2->toRawString());
var_dump($uri2->toString());

"#,
        expected: r#"string(12) "//host//path"
string(14) "//host/.//path"
string(12) "//host//path"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/scheme_error_empty",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withScheme("");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified scheme is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/scheme_error_encoded",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withScheme("http%73");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified scheme is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/scheme_error_reserved",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withScheme("https:");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified scheme is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/scheme_success_basic",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withScheme("HTTP");

var_dump($uri1->getRawScheme());
var_dump($uri2->getRawScheme());
var_dump($uri2->toRawString());
var_dump($uri2->getScheme());
var_dump($uri2->toString());

"#,
        expected: r#"string(5) "https"
string(4) "HTTP"
string(18) "HTTP://example.com"
string(4) "http"
string(18) "http://example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/scheme_success_unset_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withScheme(null);

var_dump($uri1->getRawScheme());
var_dump($uri2->getRawScheme());
var_dump($uri2->toRawString());
var_dump($uri2->getScheme());
var_dump($uri2->toString());

"#,
        expected: r#"string(5) "https"
NULL
string(13) "//example.com"
NULL
string(13) "//example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/scheme_success_unset_non_existent",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("/foo/bar");
$uri2 = $uri1->withScheme(null);

var_dump($uri1->getRawScheme());
var_dump($uri2->getRawScheme());
var_dump($uri2->toRawString());
var_dump($uri2->getScheme());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
NULL
string(8) "/foo/bar"
NULL
string(8) "/foo/bar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/userinfo_error_reserved",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->withUserInfo("us/r:password"); // us/r:password
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified userinfo is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/userinfo_success_empty",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withUserInfo("");

var_dump($uri1->getRawUserInfo());
var_dump($uri2->getRawUserInfo());
var_dump($uri2->getUserInfo());

"#,
        expected: r#"NULL
string(0) ""
string(0) ""
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/userinfo_success_encoded",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withUserInfo("%75s%2Fr:password"); // us/r:password

var_dump($uri1->getRawUserInfo());
var_dump($uri2->getRawUserInfo());
var_dump($uri2->toRawString());
var_dump($uri2->getUserInfo());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
string(17) "%75s%2Fr:password"
string(37) "https://%75s%2Fr:password@example.com"
string(15) "us%2Fr:password"
string(35) "https://us%2Fr:password@example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/userinfo_success_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://:pass@example.com");
$uri2 = $uri1->withUserInfo("user:password");

var_dump($uri1->getRawUserInfo());
var_dump($uri2->getRawUserInfo());
var_dump($uri2->toRawString());
var_dump($uri2->getUserInfo());
var_dump($uri2->toString());

"#,
        expected: r#"string(5) ":pass"
string(13) "user:password"
string(33) "https://user:password@example.com"
string(13) "user:password"
string(33) "https://user:password@example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/userinfo_success_new",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://example.com");
$uri2 = $uri1->withUserInfo("user:password");

var_dump($uri1->getRawUserInfo());
var_dump($uri2->getRawUserInfo());
var_dump($uri2->toRawString());
var_dump($uri2->getUserInfo());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
string(13) "user:password"
string(33) "https://user:password@example.com"
string(13) "user:password"
string(33) "https://user:password@example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/userinfo_success_unset_existing",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("https://user:password@example.com");
$uri2 = $uri1->withUserInfo(null);

var_dump($uri1->getRawUserInfo());
var_dump($uri2->getRawUserInfo());
var_dump($uri2->toRawString());
var_dump($uri2->getUserInfo());
var_dump($uri2->toString());

"#,
        expected: r#"string(13) "user:password"
NULL
string(19) "https://example.com"
NULL
string(19) "https://example.com"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/modification/userinfo_success_unset_non_existent",
        source: r#"<?php

$uri1 = Uri\Rfc3986\Uri::parse("/foo/bar");
$uri2 = $uri1->withUserInfo(null);

var_dump($uri1->getRawUserInfo());
var_dump($uri2->getRawUserInfo());
var_dump($uri2->toRawString());
var_dump($uri2->getUserInfo());
var_dump($uri2->toString());

"#,
        expected: r#"NULL
NULL
string(8) "/foo/bar"
NULL
string(8) "/foo/bar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/basic_error_multibyte",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("🐘");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/basic_error_null_byte",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("https://exam\0ple.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/basic_success_all",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(4) "user"
  ["password"]=>
  string(4) "info"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  int(443)
  ["path"]=>
  string(8) "/foo/bar"
  ["query"]=>
  string(15) "abc=123&def=ghi"
  ["fragment"]=>
  string(8) "hashmark"
}
string(66) "https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark"
string(66) "https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/basic_sucess_email",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("mailto:user@example.com");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(6) "mailto"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(16) "user@example.com"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(23) "mailto:user@example.com"
string(23) "mailto:user@example.com"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/basic_sucess_empty",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(0) ""
string(0) ""
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/basic_sucess_file",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("file:///E:/Documents%20and%20Settings");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(4) "file"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(0) ""
  ["port"]=>
  NULL
  ["path"]=>
  string(30) "/E:/Documents%20and%20Settings"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(37) "file:///E:/Documents%20and%20Settings"
string(37) "file:///E:/Documents%20and%20Settings"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/basic_sucess_urn",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("urn:uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(3) "urn"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(41) "uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(45) "urn:uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66"
string(45) "urn:uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/host_error_multibyte",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("https://exḁmple.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/host_error_reserved",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("https://ex[a]mple.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/host_success_empty1",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(0) ""
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(8) "https://"
string(8) "https://"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/host_success_empty2",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://user:pass@");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(4) "user"
  ["password"]=>
  string(4) "pass"
  ["host"]=>
  string(0) ""
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(18) "https://user:pass@"
string(18) "https://user:pass@"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/host_success_empty3",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("//");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(0) ""
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(2) "//"
string(2) "//"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/host_success_empty4",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("///");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(0) ""
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(3) "///"
string(3) "///"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/host_success_ipv4",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://192.168.0.1");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "192.168.0.1"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(19) "https://192.168.0.1"
string(19) "https://192.168.0.1"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/host_success_ipv4_wrong",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://192.168");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(7) "192.168"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(15) "https://192.168"
string(15) "https://192.168"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/host_success_ipv6",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://[2001:0db8:3333:4444:5555:6666:7777:8888]");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(41) "[2001:0db8:3333:4444:5555:6666:7777:8888]"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(49) "https://[2001:0db8:3333:4444:5555:6666:7777:8888]"
string(49) "https://[2001:0db8:3333:4444:5555:6666:7777:8888]"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/host_success_ipvfuture",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://[v7.host]");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(9) "[v7.host]"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(17) "https://[v7.host]"
string(17) "https://[v7.host]"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/path_error_multibyte",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("https://example.com/fȎȎ");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/path_error_reserved",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("https://example.com/fo[o/ba]r/");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/path_success_relative_reference",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("foo");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(3) "foo"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(3) "foo"
string(3) "foo"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/path_success_relative_reference_absolute",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("/foo");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(4) "/foo"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(4) "/foo"
string(4) "/foo"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/path_success_relative_reference_only_query_fragment",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("?query#fragment");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  NULL
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  string(5) "query"
  ["fragment"]=>
  string(8) "fragment"
}
string(15) "?query#fragment"
string(15) "?query#fragment"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/path_success_slash_only",
        source: r#"<?php

$uri = new Uri\Rfc3986\Uri("https://example.com/");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(20) "https://example.com/"
string(20) "https://example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/port_error_multibyte",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("https://example.com:Ȏ");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/port_error_negative",
        source: r#"<?php

try {
    var_dump(new Uri\Rfc3986\Uri("http://example.com:-1"));
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/port_error_percent_encoded",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("https://example.com:%30");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/port_error_user_info_wrong_place",
        source: r#"<?php

try {
    var_dump(new Uri\Rfc3986\Uri("https://example.com:8080@username:password"));
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/port_success_empty",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("http://example.com:");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(19) "http://example.com:"
string(19) "http://example.com:"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/port_success_leading_zeros",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("http://example.com:000000000000000000008080");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  int(8080)
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(43) "http://example.com:000000000000000000008080"
string(23) "http://example.com:8080"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/port_success_leading_zeros_zero",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("http://example.com:000000000000000000000000");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  int(0)
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(43) "http://example.com:000000000000000000000000"
string(20) "http://example.com:0"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/port_success_zero",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("http://example.com:0");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  int(0)
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(20) "http://example.com:0"
string(20) "http://example.com:0"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/scheme_error_empty",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("://example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/scheme_error_multibyte",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("ƕŢŢƤƨ://example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/scheme_error_percent_encoded",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("http%2F://example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/scheme_error_reserved",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("http&://example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/scheme_success_empty_host",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https:example.com");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(11) "example.com"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(17) "https:example.com"
string(17) "https:example.com"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/scheme_success_iana",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("chrome-extension://example.com");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(16) "chrome-extension"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(30) "chrome-extension://example.com"
string(30) "chrome-extension://example.com"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/scheme_success_only",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("http:");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(5) "http:"
string(5) "http:"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/scheme_success_only2",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("http:/");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(6) "http:/"
string(6) "http:/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/userinfo_error_extended_ascii",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("http://úzör@example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/userinfo_error_multibyte",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("http://usĕr:pąss@example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/userinfo_error_reserved",
        source: r#"<?php

try {
    new Uri\Rfc3986\Uri("http://us[er]:pass@example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/userinfo_success_empty",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://@example.com");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(0) ""
  ["password"]=>
  string(0) ""
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(20) "https://@example.com"
string(20) "https://@example.com"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/userinfo_success_only_password",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://:pass@example.com");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(0) ""
  ["password"]=>
  string(4) "pass"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(25) "https://:pass@example.com"
string(25) "https://:pass@example.com"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/parsing/userinfo_success_only_username",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://user:@example.com");

var_dump($uri);
var_dump($uri->toRawString());
var_dump($uri->toString());

"#,
        expected: r#"object(Uri\Rfc3986\Uri)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(4) "user"
  ["password"]=>
  string(0) ""
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(25) "https://user:@example.com"
string(25) "https://user:@example.com"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "rfc3986/reference_resolution/resolve_error_null_byte",
        source: r#"<?php

$uri = Uri\Rfc3986\Uri::parse("https://example.com");

try {
    $uri->resolve("/f\0o");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\InvalidUriException: The specified URI is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_false_host",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:pass@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$url2 = Uri\WhatWg\Url::parse("https://user:pass@192.168.0.1:443/foo/bar?abc=123&def=ghi#hashmark");

var_dump($url1->equals($url2));
var_dump($url2->equals($url1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_false_password",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:pass@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$url2 = Uri\WhatWg\Url::parse("https://user:@example.com:443/foo/bar?abc=123&def=ghi#hashmark");

var_dump($url1->equals($url2));
var_dump($url2->equals($url1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_false_path",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:pass@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$url2 = Uri\WhatWg\Url::parse("https://user:pass@example.com:443/foo/bar/baz?abc=123&def=ghi#hashmark");

var_dump($url1->equals($url2));
var_dump($url2->equals($url1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_false_path_percent_encoded",
        source: r#"<?php

$url1 = new Uri\WhatWg\Url("https://example.com/foo/bar");
$url2 = new Uri\WhatWg\Url("https://example.com/foo/b%61r");

var_dump($url1->equals($url2));
var_dump($url2->equals($url1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_false_port",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:pass@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$url2 = Uri\WhatWg\Url::parse("https://user:pass@example.com:442/foo/bar?abc=123&def=ghi#hashmark");

var_dump($url1->equals($url2));
var_dump($url2->equals($url1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_false_query",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:pass@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$url2 = Uri\WhatWg\Url::parse("https://user:pass@example.com:443/foo/bar?abc=123#hashmark");

var_dump($url1->equals($url2));
var_dump($url2->equals($url1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_false_query2",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:pass@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$url2 = Uri\WhatWg\Url::parse("https://user:pass@example.com:443/foo/bar?ABC=123&def=ghi#hashmark");

var_dump($url1->equals($url2));
var_dump($url2->equals($url1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_false_user",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:pass@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$url2 = Uri\WhatWg\Url::parse("https://usr:pass@example.com:443/foo/bar?abc=123&def=ghi#hashmark");

var_dump($url1->equals($url2));
var_dump($url2->equals($url1));

"#,
        expected: r#"bool(false)
bool(false)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_fragment_different",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$url2 = Uri\WhatWg\Url::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hash");

var_dump($url1->equals($url2, Uri\UriComparisonMode::IncludeFragment));
var_dump($url2->equals($url1, Uri\UriComparisonMode::IncludeFragment));

var_dump($url1->equals($url2, Uri\UriComparisonMode::ExcludeFragment));
var_dump($url2->equals($url1, Uri\UriComparisonMode::ExcludeFragment));

"#,
        expected: r#"bool(false)
bool(false)
bool(true)
bool(true)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_true_identical",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");
$url2 = Uri\WhatWg\Url::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");

var_dump($url1->equals($url2, Uri\UriComparisonMode::IncludeFragment));
var_dump($url2->equals($url1, Uri\UriComparisonMode::IncludeFragment));

var_dump($url1->equals($url2, Uri\UriComparisonMode::ExcludeFragment));
var_dump($url2->equals($url1, Uri\UriComparisonMode::ExcludeFragment));

"#,
        expected: r#"bool(true)
bool(true)
bool(true)
bool(true)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_true_normalization1",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:info@example%2ecom:0443/foo/bar?abc=123&def=ghi#hashmark");
$url2 = Uri\WhatWg\Url::parse("HTTPS://user:info@EXAMPLE.COM:443/../foo/bar?abc=123&def=ghi#hashmark");

var_dump($url1->equals($url2));
var_dump($url2->equals($url1));

"#,
        expected: r#"bool(true)
bool(true)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_true_normalization2",
        source: r#"<?php

$url1 = new Uri\WhatWg\Url("https://[2001:0db8:0001:0000:0000:0ab9:C0A8:0102]");
$url2 = new Uri\WhatWg\Url("https://[2001:DB8:1::AB9:C0A8:102]");

var_dump($url1->equals($url2));
var_dump($url2->equals($url1));

"#,
        expected: r#"bool(true)
bool(true)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/equivalence/equals_true_normalization3",
        source: r#"<?php

$url1 = new Uri\WhatWg\Url("https://example.com");
$url2 = new Uri\WhatWg\Url("https://example.com/");

var_dump($url1->equals($url2));
var_dump($url2->equals($url1));

"#,
        expected: r#"bool(true)
bool(true)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/getters/gh20771",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse('test://');

var_dump($url->getUnicodeHost());

"#,
        expected: r#"string(0) ""
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/fragment_success_auto_encode",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withFragment("<>");

var_dump($url1->getFragment());
var_dump($url2->getFragment());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(6) "%3C%3E"
string(27) "https://example.com/#%3C%3E"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/fragment_success_empty",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withFragment("");

var_dump($url1->getFragment());
var_dump($url2->getFragment());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/fragment_success_encoded",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withFragment("foo%3db%61r"); // foo=bar

var_dump($url1->getFragment());
var_dump($url2->getFragment());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(11) "foo%3db%61r"
string(32) "https://example.com/#foo%3db%61r"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/fragment_success_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com#foo");
$url2 = $url1->withFragment("bar");

var_dump($url1->getFragment());
var_dump($url2->getFragment());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(3) "foo"
string(3) "bar"
string(24) "https://example.com/#bar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/fragment_success_hashmark",
        source: r##"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withFragment("#");

var_dump($url1->getFragment());
var_dump($url2->getFragment());
var_dump($url2->toAsciiString());

"##,
        expected: r#"NULL
string(0) ""
string(21) "https://example.com/#"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/fragment_success_null_byte",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withFragment("frag\0ment");

var_dump($url1->getFragment());
var_dump($url2->getFragment());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(11) "frag%00ment"
string(32) "https://example.com/#frag%00ment"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/fragment_success_unicode",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withFragment("ő");

var_dump($url1->getFragment());
var_dump($url2->getFragment());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(6) "%C5%91"
string(27) "https://example.com/#%C5%91"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/fragment_success_unset_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com#foo");
$url2 = $url1->withFragment(null);

var_dump($url1->getFragment());
var_dump($url2->getFragment());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(3) "foo"
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/fragment_success_unset_non_existent",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withFragment(null);

var_dump($url1->getFragment());
var_dump($url2->getFragment());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/fragment_success_with_leading_hashmark",
        source: r##"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withFragment("#fragment");

var_dump($url1->getFragment());
var_dump($url2->getFragment());
var_dump($url2->toAsciiString());

"##,
        expected: r#"NULL
string(8) "fragment"
string(29) "https://example.com/#fragment"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_error_empty",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url->withHost("");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified host is malformed (HostMissing)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_error_forbidden_host_codepoint_opaque1",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("foo://example.com");

try {
    $url = $url->withHost("ex@mple.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified host is malformed (HostInvalidCodePoint)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_error_forbidden_host_codepoint_opaque2",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("foo://example.com");
$url2 = $url1->withHost("ex#mple.com");

var_dump($url1->getAsciiHost());
var_dump($url2->getAsciiHost());

"#,
        expected: r#"string(11) "example.com"
string(2) "ex"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_error_forbidden_host_codepoint_opaque3",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withHost("ex#mple.com");

var_dump($url1->getAsciiHost());
var_dump($url2->getAsciiHost());

"#,
        expected: r#"string(11) "example.com"
string(2) "ex"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_error_forbidden_host_codepoint_special1",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url = $url->withHost("ex@mple.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified host is malformed (DomainInvalidCodePoint)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_error_forbidden_host_codepoint_special2",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url = $url->withHost("ex:mple.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified host is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_error_forbidden_host_codepoint_special3",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url = $url->withHost("ex:mple.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified host is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_error_null_byte",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url->withHost("h\0st");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified host is malformed (DomainInvalidCodePoint)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_error_unset_existing",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url = $url->withHost(null);
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified host is malformed (HostMissing)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_success_encoded",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withHost("%65xample.net"); // example.net

var_dump($url1->getAsciiHost());
var_dump($url2->getAsciiHost());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(11) "example.com"
string(11) "example.net"
string(20) "https://example.net/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_success_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withHost("example.net");

var_dump($url1->getAsciiHost());
var_dump($url2->getAsciiHost());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(11) "example.com"
string(11) "example.net"
string(20) "https://example.net/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_success_idna",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withHost("éxämple.com");

var_dump($url1->getAsciiHost());
var_dump($url2->getAsciiHost());
var_dump($url2->toAsciiString());
var_dump($url2->getUnicodeHost());
var_dump($url2->toUnicodeString());

"#,
        expected: r#"string(11) "example.com"
string(19) "xn--xmple-gra7a.com"
string(28) "https://xn--xmple-gra7a.com/"
string(13) "éxämple.com"
string(22) "https://éxämple.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_success_ipv4",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withHost("192.168.0.1");

var_dump($url1->getAsciiHost());
var_dump($url2->getAsciiHost());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(11) "example.com"
string(11) "192.168.0.1"
string(20) "https://192.168.0.1/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/host_success_ipv6",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withHost("[2001:0db8:3333:4444:5555:6666:7777:8888]");

var_dump($url1->getAsciiHost());
var_dump($url2->getAsciiHost());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(11) "example.com"
string(40) "[2001:db8:3333:4444:5555:6666:7777:8888]"
string(49) "https://[2001:db8:3333:4444:5555:6666:7777:8888]/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/multiple_error_with_warnings",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://example.com")
    ->withScheme("\tscheme")
    ->withHost("\tex.com")
    ->withQuery("\refoo=bar")
    ->withFragment("\nfoo");

try {
    $url->withScheme("0");
} catch (\Uri\WhatWg\InvalidUrlException $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
    var_dump($e->errors);
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified scheme is malformed
array(0) {
}
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/password_success_auto_encoded",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:password@example.com");
$url2 = $url1->withPassword("p:s/");

var_dump($url1->getPassword());
var_dump($url2->getPassword());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(8) "password"
string(8) "p%3As%2F"
string(34) "https://user:p%3As%2F@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/password_success_empty",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://@example.com");
$url2 = $url1->withPassword("");

var_dump($url1->getPassword());
var_dump($url2->getPassword());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/password_success_empty_host",
        source: r#"<?php

$url1 = new Uri\WhatWg\Url("scheme://");
$url2 = $url1->withPassword("password");

var_dump($url1->getPassword());
var_dump($url2->getPassword());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(9) "scheme://"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/password_success_encoded",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withPassword("p%61ssword");

var_dump($url1->getPassword());
var_dump($url2->getPassword());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(10) "p%61ssword"
string(32) "https://:p%61ssword@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/password_success_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://:pass@example.com");
$url2 = $url1->withPassword("password");

var_dump($url1->getPassword());
var_dump($url2->getPassword());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(4) "pass"
string(8) "password"
string(30) "https://:password@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/password_success_new",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withPassword("password");

var_dump($url1->getPassword());
var_dump($url2->getPassword());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(8) "password"
string(30) "https://:password@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/password_success_null_byte",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withPassword("pass\0word");

var_dump($url1->getPassword());
var_dump($url2->getPassword());
var_dump($url2->toAsciiString());


"#,
        expected: r#"NULL
string(11) "pass%00word"
string(33) "https://:pass%00word@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/password_success_unset_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://username:password@example.com");
$url2 = $url1->withPassword(null);

var_dump($url1->getPassword());
var_dump($url2->getPassword());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(8) "password"
string(0) ""
string(29) "https://username@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/password_success_unset_existing2",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://:password@example.com");
$url2 = $url1->withPassword(null);

var_dump($url1->getPassword());
var_dump($url2->getPassword());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(8) "password"
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/password_success_unset_non_existent1",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withPassword(null);

var_dump($url1->getPassword());
var_dump($url2->getPassword());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/password_success_unset_non_existent2",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://username:@example.com");
$url2 = $url1->withPassword(null);

var_dump($url1->getPassword());
var_dump($url2->getPassword());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(0) ""
string(0) ""
string(29) "https://username@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/path_success_auto_encoded",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com/");
$url2 = $url1->withPath("/p^th#");

var_dump($url1->getPath());
var_dump($url2->getPath());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(1) "/"
string(10) "/p%5Eth%23"
string(29) "https://example.com/p%5Eth%23"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/path_success_empty",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withPath("");

var_dump($url1->getPath());
var_dump($url2->getPath());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(1) "/"
string(1) "/"
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/path_success_encoded",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withPath("/foo%2Fb%61r"); // /foo/bar

var_dump($url1->getPath());
var_dump($url2->getPath());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(1) "/"
string(12) "/foo%2Fb%61r"
string(31) "https://example.com/foo%2Fb%61r"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/path_success_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com/foo/bar");
$url2 = $url1->withPath("/baz");

var_dump($url1->getPath());
var_dump($url2->getPath());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(8) "/foo/bar"
string(4) "/baz"
string(23) "https://example.com/baz"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/path_success_null_byte",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withPath("/p\0th\0");

var_dump($url1->getPath());
var_dump($url2->getPath());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(1) "/"
string(10) "/p%00th%00"
string(29) "https://example.com/p%00th%00"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/path_success_unicode",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withPath("/ő");

var_dump($url1->getPath());
var_dump($url2->getPath());
var_dump($url2->toAsciiString());
"#,
        expected: r#"string(1) "/"
string(7) "/%C5%91"
string(26) "https://example.com/%C5%91"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/path_success_without_leading_slash",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withPath("foo/bar");

var_dump($url1->getPath());
var_dump($url2->getPath());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(1) "/"
string(8) "/foo/bar"
string(27) "https://example.com/foo/bar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/port_error_negative",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url->withPort(-1);
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified port is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/port_error_too_large",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url->withPort(65536);
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified port is malformed (PortOutOfRange)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/port_success_empty_host",
        source: r#"<?php

$url1 = new Uri\WhatWg\Url("scheme://");
$url2 = $url1->withPort(433);

var_dump($url1->getPort());
var_dump($url2->getPort());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(9) "scheme://"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/port_success_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("scheme://example.com:80");
$url2 = $url1->withPort(443);

var_dump($url1->getPort());
var_dump($url2->getPort());
var_dump($url2->toAsciiString());

"#,
        expected: r#"int(80)
int(443)
string(24) "scheme://example.com:443"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/port_success_new",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("scheme://example.com");
$url2 = $url1->withPort(443);

var_dump($url1->getPort());
var_dump($url2->getPort());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
int(443)
string(24) "scheme://example.com:443"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/port_success_special1",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("http://example.com:88");
$url2 = $url1->withPort(80);

var_dump($url1->getPort());
var_dump($url2->getPort());
var_dump($url2->toAsciiString());

"#,
        expected: r#"int(88)
NULL
string(19) "http://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/port_success_special2",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withPort(443);

var_dump($url1->getPort());
var_dump($url2->getPort());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/port_success_unset_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com:80");
$url2 = $url1->withPort(null);

var_dump($url1->getPort());
var_dump($url2->getPort());
var_dump($url2->toAsciiString());

"#,
        expected: r#"int(80)
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/port_success_unset_non_existent",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("ftp://example.com");
$url2 = $url1->withPort(null);

var_dump($url2->getPort());
var_dump($url2->getPort());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(18) "ftp://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/query_success_auto_encoded",
        source: r##"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withQuery("#foo ");

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url2->toAsciiString());

"##,
        expected: r#"NULL
string(9) "%23foo%20"
string(30) "https://example.com/?%23foo%20"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/query_success_context_sensitive_reserved",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withQuery("?foo=bar");

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(7) "foo=bar"
string(28) "https://example.com/?foo=bar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/query_success_empty",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withQuery("");

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/query_success_encoded",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withQuery("foo%3db%61r"); // foo=bar

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(11) "foo%3db%61r"
string(32) "https://example.com/?foo%3db%61r"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/query_success_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com?foo=bar");
$url2 = $url1->withQuery("foo=bar&baz=qux");

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(7) "foo=bar"
string(15) "foo=bar&baz=qux"
string(36) "https://example.com/?foo=bar&baz=qux"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/query_success_null_byte",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withQuery("f\0o=bar&baz=q\0x");

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(19) "f%00o=bar&baz=q%00x"
string(40) "https://example.com/?f%00o=bar&baz=q%00x"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/query_success_question_mark",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withQuery("?");

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(0) ""
string(21) "https://example.com/?"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/query_success_unicode",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withQuery("ő");

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(6) "%C5%91"
string(27) "https://example.com/?%C5%91"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/query_success_unset_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com?foo=bar");
$url2 = $url1->withQuery(null);

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(7) "foo=bar"
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/query_success_unset_non_existent",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withQuery(null);

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/query_success_with_leading_question_mark",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com/foo/bar");
$url2 = $url1->withQuery("?foo=bar");

var_dump($url1->getQuery());
var_dump($url2->getQuery());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(7) "foo=bar"
string(35) "https://example.com/foo/bar?foo=bar"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/scheme_error_empty",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url->withScheme("");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified scheme is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/scheme_error_encoded",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url->withScheme("http%73");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified scheme is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/scheme_error_invalid",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url->withScheme("http?");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified scheme is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/scheme_error_null_byte",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url->withScheme("sch\0me");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified scheme is malformed
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/scheme_success_basic",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withScheme("HTTP");

var_dump($url1->getScheme());
var_dump($url2->getScheme());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(5) "https"
string(4) "http"
string(19) "http://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/scheme_success_colon",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withScheme("http:");

var_dump($url1->getScheme());
var_dump($url2->getScheme());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(5) "https"
string(4) "http"
string(19) "http://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/scheme_success_full",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withScheme("http://");

var_dump($url1->getScheme());
var_dump($url2->getScheme());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(5) "https"
string(4) "http"
string(19) "http://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/username_success_auto_encoded",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:password@example.com");
$url2 = $url1->withUsername("u:s/r");

var_dump($url1->getUsername());
var_dump($url2->getUsername());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(4) "user"
string(9) "u%3As%2Fr"
string(39) "https://u%3As%2Fr:password@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/username_success_empty_host",
        source: r#"<?php

$url1 = new Uri\WhatWg\Url("scheme://");
$url2 = $url1->withUsername("user");

var_dump($url1->getUsername());
var_dump($url2->getUsername());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(9) "scheme://"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/username_success_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://user:@example.com");
$url2 = $url1->withUsername("username");

var_dump($url1->getUsername());
var_dump($url2->getUsername());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(4) "user"
string(8) "username"
string(29) "https://username@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/username_success_new",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withUsername("username");

var_dump($url1->getUsername());
var_dump($url2->getUsername());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
string(8) "username"
string(29) "https://username@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/username_success_null_byte",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withUsername("usern\0me");

var_dump($url1->getUsername());
var_dump($url2->getUsername());
var_dump($url2->toAsciiString());


"#,
        expected: r#"NULL
string(10) "usern%00me"
string(31) "https://usern%00me@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/username_success_unset_existing",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://username:password@example.com");
$url2 = $url1->withUsername(null);

var_dump($url1->getUsername());
var_dump($url2->getUsername());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(8) "username"
string(0) ""
string(30) "https://:password@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/username_success_unset_existing2",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://username:@example.com");
$url2 = $url1->withUsername(null);

var_dump($url1->getUsername());
var_dump($url2->getUsername());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(8) "username"
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/username_success_unset_non_existent1",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->withUsername(null);

var_dump($url1->getUsername());
var_dump($url2->getUsername());
var_dump($url2->toAsciiString());

"#,
        expected: r#"NULL
NULL
string(20) "https://example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/username_success_unset_non_existent2",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://:password@example.com");
$url2 = $url1->withUsername(null);

var_dump($url1->getUsername());
var_dump($url2->getUsername());
var_dump($url2->toAsciiString());

"#,
        expected: r#"string(0) ""
string(0) ""
string(30) "https://:password@example.com/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/modification/withPath_success_file_invalid_drive_letter",
        source: r#"<?php
$errors = [];

$url = Uri\WhatWg\Url::parse("c|/x", new Uri\WhatWg\Url("file:///d:/a/b"), $errors);

var_dump($url->getPath());
var_dump(array_map(static fn (Uri\WhatWg\UrlValidationError $error): string => $error->type->name, (array) $errors));
var_dump($url->withPath("/zz")->getPath());

"#,
        expected: r#"string(5) "/c:/x"
array(2) {
  [0]=>
  string(14) "InvalidUrlUnit"
  [1]=>
  string(29) "FileInvalidWindowsDriveLetter"
}
string(5) "/c:/x"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/basic_error_empty",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/basic_success_all",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://user:info@example.com:443/foo/bar?abc=123&def=ghi#hashmark");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(4) "user"
  ["password"]=>
  string(4) "info"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(8) "/foo/bar"
  ["query"]=>
  string(15) "abc=123&def=ghi"
  ["fragment"]=>
  string(8) "hashmark"
}
string(62) "https://user:info@example.com/foo/bar?abc=123&def=ghi#hashmark"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/basic_sucess_email",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("mailto:user@example.com");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(6) "mailto"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(16) "user@example.com"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(23) "mailto:user@example.com"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/basic_sucess_file",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("file:///E:/Documents%20and%20Settings");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "file"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(0) ""
  ["port"]=>
  NULL
  ["path"]=>
  string(30) "/E:/Documents%20and%20Settings"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(37) "file:///E:/Documents%20and%20Settings"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/basic_sucess_urn",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("urn:uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(3) "urn"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  NULL
  ["port"]=>
  NULL
  ["path"]=>
  string(41) "uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(45) "urn:uuid:6e8bc430-9c3a-11d9-9669-0800200c9a66"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_error_Invalid",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("https://ex[a]mple.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (DomainInvalidCodePoint)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_error_empty1",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("https://");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (HostMissing)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_error_empty2",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("https://user:pass@");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (HostMissing)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_error_empty3",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("/");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_error_ipvfuture",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("https://[v7.host]");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (Ipv6InvalidCodePoint)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_error_null_byte",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("https://exam\0ple.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (DomainInvalidCodePoint)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_success_empty4",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("///");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_success_ipv4",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://192.168.0.1");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "192.168.0.1"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(20) "https://192.168.0.1/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_success_ipv4_wrong",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://192.168");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "192.0.0.168"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(20) "https://192.0.0.168/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_success_ipv6",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://[2001:0db8:3333:4444:5555:6666:7777:8888]");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(40) "[2001:db8:3333:4444:5555:6666:7777:8888]"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(49) "https://[2001:db8:3333:4444:5555:6666:7777:8888]/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_success_multibyte",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://exḁmple.com");

var_dump($url);
var_dump($url->toAsciiString());
var_dump($url->toUnicodeString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(19) "xn--exmple-xf7b.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(28) "https://xn--exmple-xf7b.com/"
string(22) "https://exḁmple.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/host_success_multibyte_long",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://" . str_repeat("é", 5000) . ".com/");
$host = $url->getAsciiHost();

var_dump(strlen($host));
var_dump(substr($host, 0, 12));
var_dump(substr($host, -6));
var_dump(substr_count($host, "a"));
var_dump($url->getUnicodeHost() === str_repeat("é", 5000) . ".com");

"#,
        expected: r#"int(5010)
string(12) "xn--9caaaaaa"
string(6) "aa.com"
int(5000)
bool(true)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/password_success_empty",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://user:@example.com");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(4) "user"
  ["password"]=>
  string(0) ""
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(25) "https://user@example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/password_success_extended_ascii",
        source: r#"<?php

$url = new Uri\WhatWg\Url("http://user:pásswörd@example.com");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  string(4) "user"
  ["password"]=>
  string(18) "p%C3%A1ssw%C3%B6rd"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(43) "http://user:p%C3%A1ssw%C3%B6rd@example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/password_success_multibyte",
        source: r#"<?php

$url = new Uri\WhatWg\Url("http://user:pąss@example.com");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  string(4) "user"
  ["password"]=>
  string(9) "p%C4%85ss"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(34) "http://user:p%C4%85ss@example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/password_success_null_byte",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://username:\0pass@example.com/");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#1 (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(8) "username"
  ["password"]=>
  string(7) "%00pass"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(37) "https://username:%00pass@example.com/"
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/password_success_percent_encode_set",
        source: r#"<?php

$url = new Uri\WhatWg\Url("http://user:pa|s@example.com");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  string(4) "user"
  ["password"]=>
  string(6) "pa%7Cs"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(31) "http://user:pa%7Cs@example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/path_error_relative_reference",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("foo");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/path_error_relative_reference_absolute",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("/foo");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/path_error_relative_reference_only_query_fragment",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("?query#fragment");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/path_success_dot_segment_delimiters",
        source: r#"<?php

foreach ([
    "https://example.com/..#frag",
    "https://example.com/caf\u{e9}/..#frag",
    "https://example.com/..?q=1",
    "https://example.com/%?q",
] as $input) {
    $url = new Uri\WhatWg\Url($input);
    var_dump($url->getPath(), $url->getQuery(), $url->getFragment());
}

"#,
        expected: r#"string(1) "/"
NULL
string(4) "frag"
string(1) "/"
NULL
string(4) "frag"
string(1) "/"
string(3) "q=1"
NULL
string(2) "/%"
string(1) "q"
NULL
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/path_success_multibyte",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://example.com/fȎȎ");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(14) "/f%C8%8E%C8%8E"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(33) "https://example.com/f%C8%8E%C8%8E"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/path_success_null_byte",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://example.com/pa\0th\0");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#1 (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(8) "/pa%00th"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(27) "https://example.com/pa%00th"
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/path_success_opaque_space_before_query",
        source: r#"<?php

foreach (["data:x ?q", "data:x #f", "foo:a b ?c", "data:x  ?q"] as $input) {
    $url = new Uri\WhatWg\Url($input);
    var_dump($url->getPath());
}

"#,
        expected: r#"string(2) "x "
string(2) "x "
string(4) "a b "
string(3) "x  "
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/path_success_percent_encode_set2",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://example.com/foo\"/<bar>/^{baz}");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(30) "/foo%22/%3Cbar%3E/%5E%7Bbaz%7D"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(49) "https://example.com/foo%22/%3Cbar%3E/%5E%7Bbaz%7D"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/path_success_slash_only",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://example.com/");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(20) "https://example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/path_success_special",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://example.com/fo[o/ba]r/baz=q@z,&");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(20) "/fo[o/ba]r/baz=q@z,&"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(39) "https://example.com/fo[o/ba]r/baz=q@z,&"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/port_error_multibyte",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("https://example.com:Ȏ");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (PortInvalid)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/port_error_negative",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("http://example.com:-1");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (PortInvalid)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/port_error_percent_encoded",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("https://example.com:%30");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (PortInvalid)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/port_error_user_info_wrong_place",
        source: r#"<?php

try {
    var_dump(new Uri\WhatWg\Url("https://example.com:8080@username:password"));
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (PortInvalid)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/port_success_empty",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("http://example.com:");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(19) "http://example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/port_success_zero",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("http://example.com:0");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  int(0)
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(21) "http://example.com:0/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/query_success_null_byte",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://example.com/?fo\0=bar\0");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#1 (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  string(9) "fo%00=bar"
  ["fragment"]=>
  NULL
}
string(30) "https://example.com/?fo%00=bar"
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/scheme_error_empty",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("://example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/scheme_error_invalid",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("http&://example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/scheme_error_multibyte",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("ƕŢŢƤƨ://example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/scheme_error_null_byte",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("ht\0tp://example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/scheme_error_only1",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("http:");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (HostMissing)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/scheme_error_only2",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("http:/");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (HostMissing)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/scheme_error_percent_encoded",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("http%2F://example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/scheme_error_special",
        source: r#"<?php

try {
    new Uri\WhatWg\Url("hÁttp://example.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (MissingSchemeNonRelativeUrl)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/scheme_success_iana",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("chrome-extension://example.com");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(16) "chrome-extension"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(0) ""
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(30) "chrome-extension://example.com"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/scheme_success_without_slash",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https:example.com");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(20) "https://example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/username_success_at_sign",
        source: r#"<?php

$url = new Uri\WhatWg\Url("http://user@name:pass@word@localhost/");

var_dump($url->getUsername());
var_dump($url->getPassword());
var_dump($url->getAsciiHost());
var_dump($url->toAsciiString());

"#,
        expected: r#"string(11) "user%40name"
string(11) "pass%40word"
string(9) "localhost"
string(41) "http://user%40name:pass%40word@localhost/"
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/username_success_empty",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://:pass@example.com");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(0) ""
  ["password"]=>
  string(4) "pass"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(26) "https://:pass@example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/username_success_extended_ascii",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://úzör@example.com");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(14) "%C3%BAz%C3%B6r"
  ["password"]=>
  string(0) ""
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(35) "https://%C3%BAz%C3%B6r@example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/username_success_multibyte",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://usĕr:pass@example.com");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(9) "us%C4%95r"
  ["password"]=>
  string(4) "pass"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(35) "https://us%C4%95r:pass@example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/username_success_null_byte",
        source: r#"<?php

$url = new Uri\WhatWg\Url("https://user\0name:pass@example.com/");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#1 (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  string(11) "user%00name"
  ["password"]=>
  string(4) "pass"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(37) "https://user%00name:pass@example.com/"
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/parsing/username_success_percent_encode_set",
        source: r#"<?php

$url = new Uri\WhatWg\Url("http://us[er]:pass@example.com");

var_dump($url);
var_dump($url->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#%d (%d) {
  ["scheme"]=>
  string(4) "http"
  ["username"]=>
  string(10) "us%5Ber%5D"
  ["password"]=>
  string(4) "pass"
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(35) "http://us%5Ber%5D:pass@example.com/"
"#,
        expectf: true,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/reference_resolution/resolve_error_host_null_byte",
        source: r#"<?php

$url = Uri\WhatWg\Url::parse("https://example.com");

try {
    $url->resolve("https://ex\0mple.com");
} catch (Throwable $e) {
    echo $e::class, ": ", $e->getMessage(), PHP_EOL;
}

"#,
        expected: r#"Uri\WhatWg\InvalidUrlException: The specified URI is malformed (DomainInvalidCodePoint)
"#,
        expectf: false,
        isolate: false,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
    UriCase {
        name: "whatwg/reference_resolution/resolve_success_path_null_byte",
        source: r#"<?php

$url1 = Uri\WhatWg\Url::parse("https://example.com");
$url2 = $url1->resolve("/f\0o");

var_dump($url1);
var_dump($url2);
var_dump($url2->toAsciiString());

"#,
        expected: r#"object(Uri\WhatWg\Url)#1 (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(1) "/"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
object(Uri\WhatWg\Url)#2 (8) {
  ["scheme"]=>
  string(5) "https"
  ["username"]=>
  NULL
  ["password"]=>
  NULL
  ["host"]=>
  string(11) "example.com"
  ["port"]=>
  NULL
  ["path"]=>
  string(6) "/f%00o"
  ["query"]=>
  NULL
  ["fragment"]=>
  NULL
}
string(25) "https://example.com/f%00o"
"#,
        expectf: false,
        isolate: true,
        compile_failure: false,
        heap_size: None,
        zend_test: false,
    },
];
