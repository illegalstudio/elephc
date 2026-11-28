<?php
class ThrowingOriginDestructor {
    public function __destruct() { echo 'D|'; throw new RuntimeException('drop'); }
}
function objectCandidate(mixed $value): mixed { return $value; }
function destroyedOrigin(bool $throwing): void {
    if ($throwing) { $value = new ThrowingOriginDestructor(); }
    else { $value = new stdClass(); }
    try { unset($value); }
    catch (RuntimeException $error) { echo 'C', isset($value) ? 'B' : 'M', '|'; unset($error); }
    $value = 's';
    try { $value = objectCandidate(2); echo 'W'; }
    catch (TypeError $error) { echo 'T'; unset($error); }
    echo gettype($value), '|';
}
destroyedOrigin(true); destroyedOrigin(false);
