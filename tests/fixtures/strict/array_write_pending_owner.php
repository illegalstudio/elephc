<?php
function failPendingArrayWrite(int $count): int {
    if ($count > 0) { throw new RuntimeException('rhs'); }
    return 9;
}
$items = [1];
try { $items[0] = failPendingArrayWrite($argc); echo 'X'; }
catch (RuntimeException $error) { echo 'C'; }
$code = $argc > 0 ? 'echo "E";' : '';
eval($code);
echo '|', $items[0];
