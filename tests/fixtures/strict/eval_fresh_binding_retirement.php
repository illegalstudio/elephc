<?php
function splitFreshOrigin(): void {
    eval('$created = 1;');
    try { eval('$created = "bad";'); echo 'W|'; }
    catch (TypeError $error) { echo 'S|'; }
}
function unsetFreshOrigin(): void {
    eval('$created = 1;');
    eval('unset($created);');
    eval('$created = "new"; echo $created;'); echo '|';
}
splitFreshOrigin(); unsetFreshOrigin(); unsetFreshOrigin();
