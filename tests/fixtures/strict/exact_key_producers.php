<?php
function showProducedKeyKinds(string $label, mixed $items): void {
    if (!is_array($items)) { throw new TypeError('A key producer must return an array'); }
    echo $label, ':';
    foreach ($items as $key => $value) {
        echo gettype($key), '=', $key, ':', $value, '|';
    }
    echo "\n";
}
function exactKeyProducerObservations(mixed $scratch = 42): void {
    $decoded = json_decode('{"0":1,"01":2,"-1":3}', true);
    showProducedKeyKinds('json', $decoded);
    $decoded = unserialize('a:3:{s:1:"0";i:11;i:0;i:22;s:2:"01";i:33;}');
    showProducedKeyKinds('serialized', $decoded);
    set_error_handler(static function (int $level, string $message): bool { return true; });
    echo unserialize('invalid-wire') === false ? 'invalid|' : 'bad|';
    echo unserialize($scratch) === false ? 'scratch|' : 'bad|';
    restore_error_handler();
    set_error_handler(static function (int $level, string $message): bool {
        throw new ValueError('stop parsing');
    });
    try { unserialize('throwing-wire'); echo 'bad|'; }
    catch (ValueError $error) { echo 'thrown|'; }
    restore_error_handler();
}
