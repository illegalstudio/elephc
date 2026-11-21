<?php
function catchScopeProbe(int $selector): void {
 $source=$selector > 0 ? '
 try { echo "T"; } catch (TypeError $error) { echo "T"; unset($error); }
 try { throw new ValueError("second"); } catch (ValueError $error) { echo "V"; unset($error); }
 ' : '';
 eval($source);
 try { throw new TypeError("third"); } catch (TypeError $error) { echo 'N'; unset($error); }
}
catchScopeProbe($argc);
gc_collect_cycles();
