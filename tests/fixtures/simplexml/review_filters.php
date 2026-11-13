<?php
class XmlReviewOtherFilter extends php_user_filter {
    public function onClose(): void { echo 'other-close|'; }
}
class XmlReviewThrowFilter extends php_user_filter {
    public function onCreate(): bool { echo 'create|'; return true; }
    public function filter($in, $out, &$consumed, bool $closing): int { throw new RuntimeException('filter'); }
    public function onClose(): void { echo 'unexpected-close|'; }
    public function __destruct() {
        $other = fopen('php://temp', 'w+');
        stream_filter_append($other, 'xml.review.other', STREAM_FILTER_READ);
        fclose($other);
        echo 'drop|';
    }
}
stream_filter_register('xml.review.other', 'XmlReviewOtherFilter');
stream_filter_register('xml.review.throw', 'XmlReviewThrowFilter');
// REVIEW_BODY
set_error_handler(function(int $level, string $message): bool { return true; });
$constructor = $argc === 2;

$uri = 'php://filter/read=xml.review.throw/resource=data://text/plain,%3Cr%2F%3E';
try {
    if ($constructor) { new SimpleXMLElement($uri, 0, true); }
    else { simplexml_load_file($uri); }
} catch (Throwable $error) {
    echo get_class($error), ':', $error->getMessage(), '|';
    if ($constructor) { echo get_class($error->getPrevious()), '|'; }
}
unset($error, $uri);

restore_error_handler();
