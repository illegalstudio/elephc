//! Purpose:
//! Declares PHP 8.6's `Uri\Rfc3986\UriBuilder` and `Uri\WhatWg\UrlBuilder`, the last
//! `ext/uri` surface added after the readonly URI value classes.
//!
//! Called from:
//! - `crate::uri_prelude::uri_declarations`.
//!
//! Key details:
//! - The builders are ordinary PHP source parsed at injection time. Their components are
//!   validated with the same `elephc_uri_validate` bridge the value classes use, and `build()`
//!   composes the raw reference and hands it to the already-verified parser.
//! - WHATWG `build()` mirrors php-src's pre-checks for special schemes and credentials so the
//!   php-src error text and `errors` payload stay stable.

use crate::compiler_stack::with_compiler_stack;
use crate::parser::ast::Program;

/// The complete builder source for both URI dialects.
const BUILDERS_SRC: &str = r#"<?php

namespace Uri\Rfc3986 {
    final class UriBuilder
    {
        private ?string $scheme = null;
        private ?string $userinfo = null;
        private ?string $host = null;
        private ?int $port = null;
        private string $path = "";
        private ?string $query = null;
        private ?string $fragment = null;

        public function reset(): static
        {
            $this->scheme = null;
            $this->userinfo = null;
            $this->host = null;
            $this->port = null;
            $this->path = "";
            $this->query = null;
            $this->fragment = null;
            return $this;
        }

        public function setScheme(?string $scheme): static
        {
            if ($scheme !== null && \elephc_uri_validate(0, 0, \bin2hex($scheme)) !== 0) {
                throw new \Uri\InvalidUriException('The specified scheme is malformed');
            }
            $this->scheme = $scheme;
            return $this;
        }

        public function setUserInfo(#[\SensitiveParameter] ?string $userInfo): static
        {
            if ($userInfo !== null && \elephc_uri_validate(0, 3, \bin2hex($userInfo)) !== 0) {
                throw new \Uri\InvalidUriException('The specified userinfo is malformed');
            }
            $this->userinfo = $userInfo;
            return $this;
        }

        public function setHost(?string $host): static
        {
            if ($host !== null && \elephc_uri_validate(0, 4, \bin2hex($host)) !== 0) {
                throw new \Uri\InvalidUriException('The specified host is malformed');
            }
            $this->host = $host;
            return $this;
        }

        public function setPort(?int $port): static
        {
            if ($port !== null && $port < 0) {
                throw new \Uri\InvalidUriException('The specified port is malformed');
            }
            $this->port = $port;
            return $this;
        }

        public function setPath(string $path): static
        {
            if (\elephc_uri_validate(0, 6, \bin2hex($path)) !== 0) {
                throw new \Uri\InvalidUriException('The specified path is malformed');
            }
            $this->path = $path;
            return $this;
        }

        public function setQuery(?string $query): static
        {
            if ($query !== null && \elephc_uri_validate(0, 7, \bin2hex($query)) !== 0) {
                throw new \Uri\InvalidUriException('The specified query is malformed');
            }
            $this->query = $query;
            return $this;
        }

        public function setFragment(?string $fragment): static
        {
            if ($fragment !== null && \elephc_uri_validate(0, 8, \bin2hex($fragment)) !== 0) {
                throw new \Uri\InvalidUriException('The specified fragment is malformed');
            }
            $this->fragment = $fragment;
            return $this;
        }

        public function build(?\Uri\Rfc3986\Uri $baseUrl = null): \Uri\Rfc3986\Uri
        {
            if ($this->host === null) {
                if ($this->userinfo !== null) {
                    throw new \Uri\InvalidUriException('Cannot set a userinfo without having a host');
                }
                if ($this->port !== null) {
                    throw new \Uri\InvalidUriException('Cannot set a port without having a host');
                }
            }
            if ($this->host !== null && $this->path !== '' && !\str_starts_with($this->path, '/')) {
                throw new \Uri\InvalidUriException('The specified path is malformed');
            }
            if ($this->path !== '') {
                if ($this->scheme === null) {
                    $slash = \strpos($this->path, '/');
                    $first = $slash === false ? $this->path : \substr($this->path, 0, (int) $slash);
                    if (\str_contains($first, ':')) {
                        throw new \Uri\InvalidUriException('The path must not begin with ":" when the URI does not contain a scheme');
                    }
                }
                if ($this->host === null && \str_starts_with($this->path, '//')) {
                    throw new \Uri\InvalidUriException('The path must not begin with "//" when the URI does not contain a host');
                }
            }

            $text = '';
            if ($this->scheme !== null) {
                $text .= $this->scheme . ':';
            }
            if ($this->host !== null) {
                $text .= '//';
                if ($this->userinfo !== null) {
                    $text .= $this->userinfo . '@';
                }
                $text .= $this->host;
                if ($this->port !== null) {
                    $text .= ':' . $this->port;
                }
            }
            $text .= $this->path;
            if ($this->query !== null) {
                $text .= '?' . $this->query;
            }
            if ($this->fragment !== null) {
                $text .= '#' . $this->fragment;
            }

            $uri = new \Uri\Rfc3986\Uri($text);
            if ($baseUrl !== null) {
                return $baseUrl->resolve($uri->toRawString());
            }
            return $uri;
        }
    }
}

namespace Uri\WhatWg {
    enum UrlPercentEncodingMode
    {
        case Username;
        case Password;
        case OpaqueHost;
        case Path;
        case OpaquePath;
        case PathSegment;
        case Query;
        case SpecialQuery;
        case FormQuery;
        case Fragment;
    }

    function url_percent_encode(string $input, UrlPercentEncodingMode $mode): string
    {
        $ordinal = match ($mode) {
            \Uri\WhatWg\UrlPercentEncodingMode::Username => 0,
            \Uri\WhatWg\UrlPercentEncodingMode::Password => 1,
            \Uri\WhatWg\UrlPercentEncodingMode::OpaqueHost => 2,
            \Uri\WhatWg\UrlPercentEncodingMode::Path => 3,
            \Uri\WhatWg\UrlPercentEncodingMode::OpaquePath => 4,
            \Uri\WhatWg\UrlPercentEncodingMode::PathSegment => 5,
            \Uri\WhatWg\UrlPercentEncodingMode::Query => 6,
            \Uri\WhatWg\UrlPercentEncodingMode::SpecialQuery => 7,
            \Uri\WhatWg\UrlPercentEncodingMode::FormQuery => 8,
            \Uri\WhatWg\UrlPercentEncodingMode::Fragment => 9,
        };
        $hex = \elephc_uri_percent_encode($ordinal, \bin2hex($input));
        if ($hex === null) {
            throw new \Uri\UriError('Cannot percent-encode input');
        }
        return \hex2bin($hex);
    }

    final class UrlBuilder
    {
        private string $scheme = "";
        private ?string $username = null;
        private ?string $password = null;
        private ?string $host = null;
        private ?int $port = null;
        private string $path = "";
        private ?string $query = null;
        private ?string $fragment = null;
        private array $warnings = [];

        public function reset(): static
        {
            $this->scheme = "";
            $this->warnings = [];
            $this->username = null;
            $this->password = null;
            $this->host = null;
            $this->port = null;
            $this->path = "";
            $this->query = null;
            $this->fragment = null;
            return $this;
        }

        public function setScheme(string $scheme): static
        {
            $this->__elephc_record_invalid_unit($scheme);
            $scheme = self::__elephc_strip_newlines($scheme);
            if (\elephc_uri_validate(1, 0, \bin2hex($scheme)) !== 0) {
                throw new \Uri\WhatWg\InvalidUrlException('The specified scheme is malformed (MissingSchemeNonRelativeUrl)');
            }
            $this->scheme = $scheme;
            return $this;
        }

        public function setUsername(?string $username): static
        {
            $this->username = $username;
            return $this;
        }

        public function setPassword(#[\SensitiveParameter] ?string $password): static
        {
            $this->password = $password;
            return $this;
        }

        public function setHost(?string $host): static
        {
            if ($host !== null) {
                self::__elephc_validate_host($host);
            }
            $this->host = $host;
            return $this;
        }

        public function setPort(?int $port): static
        {
            if ($port !== null) {
                if ($port < 0) {
                    throw new \Uri\WhatWg\InvalidUrlException('The specified port is malformed (PortInvalid)');
                }
                if ($port > 65535) {
                    throw new \Uri\WhatWg\InvalidUrlException('The specified port is malformed (PortOutOfRange)');
                }
            }
            $this->port = $port;
            return $this;
        }

        public function setPath(string $path): static
        {
            $this->__elephc_record_invalid_unit($path);
            $this->path = self::__elephc_strip_newlines($path);
            return $this;
        }

        public function setQuery(?string $query): static
        {
            if ($query !== null) {
                $this->__elephc_record_invalid_unit($query);
                $query = self::__elephc_strip_newlines($query);
            }
            $this->query = $query;
            return $this;
        }

        public function setFragment(?string $fragment): static
        {
            if ($fragment !== null) {
                $this->__elephc_record_invalid_unit($fragment);
                $fragment = self::__elephc_strip_newlines($fragment);
            }
            $this->fragment = $fragment;
            return $this;
        }

        public function build(?\Uri\WhatWg\Url $baseUrl = null, &$softErrors = null): \Uri\WhatWg\Url
        {
            $builderWarnings = $this->warnings;
            $this->warnings = [];
            $normalizedHost = $this->host === null ? null : \trim($this->host, "\t\n\r");
            $special = \in_array($this->scheme, ['http', 'https', 'ws', 'wss', 'ftp', 'file'], true);
            $hostErrors = [];
            if ($this->host !== null && $this->host !== '' && $normalizedHost === '') {
                $hostErrors[] = new \Uri\WhatWg\UrlValidationError(
                    $this->host,
                    \Uri\WhatWg\UrlValidationErrorType::InvalidUrlUnit,
                    false,
                );
            }
            if ($special && $this->scheme !== 'file' && $normalizedHost === null) {
                $softErrors = [];
                throw new \Uri\WhatWg\InvalidUrlException(
                    'The specified host is malformed (HostMissing)',
                    \array_merge((array) $builderWarnings, [new \Uri\WhatWg\UrlValidationError('', \Uri\WhatWg\UrlValidationErrorType::HostMissing, true)]),
                );
            }
            if ($normalizedHost === null || $normalizedHost === '' || $this->scheme === 'file') {
                if ($this->username !== null && $this->username !== '') {
                    throw new \Uri\WhatWg\InvalidUrlException('The specified URL cannot have username', $hostErrors);
                }
                if ($this->password !== null && $this->password !== '') {
                    throw new \Uri\WhatWg\InvalidUrlException('The specified URL cannot have password', $hostErrors);
                }
                if ($this->port !== null) {
                    throw new \Uri\WhatWg\InvalidUrlException('The specified URL cannot have port', $hostErrors);
                }
            }

            $baseOpaque = $baseUrl !== null
                && !\in_array($baseUrl->getScheme(), ['http', 'https', 'ws', 'wss', 'ftp', 'file'], true)
                && $baseUrl->getHostType() === null;
            if ($baseOpaque) {
                $trimmedPath = self::__elephc_strip_newlines($this->path);
                $pathWarnings = \array_slice((array) $builderWarnings, 0, 0);
                if ($trimmedPath !== '') {
                    $pathWarnings = (array) $builderWarnings;
                }
                if ($this->host !== null) {
                    $softErrors = [];
                    self::__elephc_throw_missing_scheme('host', (array) $builderWarnings);
                }
                if ($trimmedPath !== '') {
                    $softErrors = [];
                    self::__elephc_throw_missing_scheme('path', (array) $builderWarnings);
                }
                if ($this->query !== null) {
                    $softErrors = [];
                    self::__elephc_throw_missing_scheme('query', $pathWarnings);
                }
                if ($this->fragment === null) {
                    $softErrors = [];
                    self::__elephc_throw_missing_scheme('path', $pathWarnings);
                }
            }

            $path = $this->path;
            $forceFileText = false;
            if ($this->scheme === '' && $this->host === null && $baseUrl !== null) {
                if ($baseUrl->getScheme() === 'file' && \str_starts_with($path, '//')) {
                    $forceFileText = true;
                } elseif ($baseUrl->getScheme() === 'file' && \strlen($path) >= 3
                    && \ctype_alpha(\substr($path, 0, 1))
                    && \substr($path, 1, 1) === ':'
                    && (\substr($path, 2, 1) === '/' || \substr($path, 2, 1) === '\\')) {
                    $forceFileText = true;
                    $path = '/' . \strtoupper(\substr($path, 0, 1)) . \substr($path, 1);
                } elseif (\str_starts_with($path, '//')) {
                    $path = '/.' . $path;
                } elseif ($path !== '' && !\str_starts_with($path, '/')) {
                    $slash = \strpos($path, '/');
                    $first = $slash === false ? $path : \substr($path, 0, (int) $slash);
                    if (\str_contains($first, ':')) {
                        $path = './' . $path;
                    }
                }
            }

            $text = '';
            if ($forceFileText) {
                $text = 'file://' . $path;
            } elseif ($this->scheme !== '') {
                $text .= self::__elephc_strip_newlines($this->scheme) . ':';
                if ($this->host === null && $this->scheme === 'file') {
                    $text .= '//';
                }
            }
            if ($this->host !== null) {
                $text .= '//';
                $username = $this->username;
                $password = $this->password;
                if (($username === null || $username === '') && ($password === null || $password === '')) {
                    $username = null;
                    $password = null;
                }
                if ($username !== null || $password !== null) {
                    if ($username !== null) {
                        $text .= \Uri\WhatWg\url_percent_encode($username, \Uri\WhatWg\UrlPercentEncodingMode::Username);
                    }
                    if ($password !== null) {
                        $text .= ':' . \Uri\WhatWg\url_percent_encode($password, \Uri\WhatWg\UrlPercentEncodingMode::Password);
                    }
                    $text .= '@';
                }
                $text .= $this->host;
                if ($this->port !== null) {
                    $text .= ':' . $this->port;
                }
            }
            if (!$forceFileText && ($this->scheme !== '' || $this->host !== null || $baseUrl !== null) && $path !== '') {
                if (($this->host !== null || ($this->scheme === 'file' && !$forceFileText)) && !\str_starts_with($path, '/')) {
                    if (\str_starts_with($path, '\\')) {
                        $path = '/' . \substr($path, 1);
                    } else {
                        $path = '/' . $path;
                    }
                }
                if ($special || $this->host !== null || $baseUrl !== null || \str_starts_with($path, '/')) {
                    $text .= \Uri\WhatWg\url_percent_encode($path, \Uri\WhatWg\UrlPercentEncodingMode::Path);
                } else {
                    if (\str_contains($path, '#') || \str_contains($path, '?')) {
                        throw new \Uri\WhatWg\InvalidUrlException('The specified path is malformed');
                    }
                    $text .= \Uri\WhatWg\url_percent_encode($path, \Uri\WhatWg\UrlPercentEncodingMode::OpaquePath);
                }
            } elseif (!$forceFileText && $path !== '') {
                $text .= \Uri\WhatWg\url_percent_encode($path, \Uri\WhatWg\UrlPercentEncodingMode::OpaquePath);
            }
            if ($this->query !== null && $this->query !== '') {
                $query = $this->query;
                if (\str_starts_with($query, '?')) {
                    $query = \substr($query, 1);
                }
                if ($special) {
                    $text .= '?' . \Uri\WhatWg\url_percent_encode($query, \Uri\WhatWg\UrlPercentEncodingMode::SpecialQuery);
                } else {
                    $text .= '?' . \Uri\WhatWg\url_percent_encode($query, \Uri\WhatWg\UrlPercentEncodingMode::Query);
                }
            }
            if ($this->fragment !== null && $this->fragment !== '') {
                $fragment = $this->fragment;
                if (\str_starts_with($fragment, '#')) {
                    $fragment = \substr($fragment, 1);
                }
                $text .= '#' . \Uri\WhatWg\url_percent_encode($fragment, \Uri\WhatWg\UrlPercentEncodingMode::Fragment);
            }

            try {
                $url = new \Uri\WhatWg\Url($text, $forceFileText ? null : $baseUrl, $softErrors);
                if ($this->query === '') {
                    $url = $url->withQuery('');
                }
                if ($this->fragment === '') {
                    $url = $url->withFragment('');
                }
                if (($this->username !== null || $this->password !== null) && \is_array($softErrors)) {
                    $filtered = [];
                    foreach ($softErrors as $entry) {
                        if ($entry->type !== \Uri\WhatWg\UrlValidationErrorType::InvalidCredentials) {
                            $filtered[] = $entry;
                        }
                    }
                    $softErrors = $filtered;
                }
                $extraErrors = (array) $builderWarnings;
                if ($this->path !== '' && \str_contains($this->path, '#')) {
                    $extraErrors[] = new \Uri\WhatWg\UrlValidationError(
                        \substr($this->path, (int) \strpos($this->path, '#')),
                        \Uri\WhatWg\UrlValidationErrorType::InvalidUrlUnit,
                        false,
                    );
                }
                if ($this->path !== '' && \str_contains($this->path, '\\')) {
                    $extraErrors[] = new \Uri\WhatWg\UrlValidationError(
                        \substr($this->path, (int) \strpos($this->path, '\\')),
                        \Uri\WhatWg\UrlValidationErrorType::InvalidReverseSoldius,
                        false,
                    );
                }
                if ($extraErrors !== [] && \is_array($softErrors)) {
                    $softErrors = \array_merge((array) $softErrors, $extraErrors);
                }
                return $url;
            } catch (\Uri\WhatWg\InvalidUrlException $error) {
                $softErrors = [];
                throw new \Uri\WhatWg\InvalidUrlException(
                    self::__elephc_component_message($error->getMessage(), $this->path, $this->query, $this->fragment, $this->host),
                    \array_merge($error->errors, (array) $builderWarnings),
                );
            }
        }

        private static function __elephc_component_message(string $message, string $path, ?string $query, ?string $fragment, ?string $host): string
        {
            if (\str_contains($message, 'HostMissing') || \str_contains($message, 'HostInvalidCodePoint')
                || \str_contains($message, 'DomainInvalidCodePoint') || \str_contains($message, 'Ipv4')
                || \str_contains($message, 'Ipv6') || \str_contains($message, 'Port')) {
                return \str_replace('The specified URI is malformed', 'The specified host is malformed', $message);
            }
            $component = 'URI';
            if ($path !== '') {
                $component = 'path';
            } elseif ($query !== null) {
                $component = 'query';
            } elseif ($fragment !== null) {
                $component = 'fragment';
            } elseif ($host !== null) {
                $component = 'host';
            }
            return \str_replace('The specified URI is malformed', 'The specified ' . $component . ' is malformed', $message);
        }

        private static function __elephc_throw_missing_scheme(string $component, array $warnings = []): void
        {
            $warnings[] = new \Uri\WhatWg\UrlValidationError(
                '',
                \Uri\WhatWg\UrlValidationErrorType::MissingSchemeNonRelativeUrl,
                true,
            );
            throw new \Uri\WhatWg\InvalidUrlException(
                'The specified ' . $component . ' is malformed (MissingSchemeNonRelativeUrl)',
                $warnings,
            );
        }

        private function __elephc_record_invalid_unit(string $value): void
        {
            $first = -1;
            $tab = \strpos($value, "\t");
            $newline = \strpos($value, "\n");
            $carriage = \strpos($value, "\r");
            if ($tab !== false) {
                $first = (int) $tab;
            }
            if ($newline !== false && ($first < 0 || (int) $newline < $first)) {
                $first = (int) $newline;
            }
            if ($carriage !== false && ($first < 0 || (int) $carriage < $first)) {
                $first = (int) $carriage;
            }
            if ($first >= 0) {
                $this->warnings[] = new \Uri\WhatWg\UrlValidationError(
                    \substr($value, $first),
                    \Uri\WhatWg\UrlValidationErrorType::InvalidUrlUnit,
                    false,
                );
            }
        }

        private static function __elephc_strip_newlines(string $value): string
        {
            $value = \str_replace("\t", '', $value);
            $value = \str_replace("\n", '', $value);
            $value = \str_replace("\r", '', $value);
            return $value;
        }

        private static function __elephc_validate_host(string $host): void
        {
            if ($host === '') {
                return;
            }
            $trimmed = \ltrim($host, "\t\n\r");
            if (\str_contains($trimmed, '/') || \str_contains($trimmed, '?')
                || \str_contains($trimmed, '#') || \str_contains($trimmed, '\\')) {
                self::__elephc_throw_host_error();
            }
            if ($trimmed !== '' && $trimmed[0] === '[') {
                if (!\str_ends_with($trimmed, ']')) {
                    throw new \Uri\WhatWg\InvalidUrlException(
                        'The specified host is malformed (Ipv6Unclosed)',
                        [
                            new \Uri\WhatWg\UrlValidationError(
                                '',
                                \Uri\WhatWg\UrlValidationErrorType::Ipv6Unclosed,
                                true,
                            ),
                        ],
                    );
                }
                if (\inet_pton(\substr($trimmed, 1, -1)) === false) {
                    throw new \Uri\WhatWg\InvalidUrlException(
                        'The specified host is malformed (Ipv6InvalidCodePoint)',
                        [
                            new \Uri\WhatWg\UrlValidationError(
                                '',
                                \Uri\WhatWg\UrlValidationErrorType::Ipv6InvalidCodePoint,
                                true,
                            ),
                        ],
                    );
                }
            }
        }

        private static function __elephc_throw_host_error(): void
        {
            throw new \Uri\WhatWg\InvalidUrlException(
                'The specified host is malformed (HostInvalidCodePoint)',
                [
                    new \Uri\WhatWg\UrlValidationError(
                        '',
                        \Uri\WhatWg\UrlValidationErrorType::HostInvalidCodePoint,
                        true,
                    ),
                ],
            );
        }
    }
}
"#;

/// Returns the parsed builder declarations.
pub(super) fn declarations() -> Program {
    with_compiler_stack(|| {
        let tokens = crate::lexer::tokenize(BUILDERS_SRC).expect("uri builder prelude must tokenize");
        crate::parser::parse_internal(&tokens).expect("uri builder prelude must parse")
    })
}
