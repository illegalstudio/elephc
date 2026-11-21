<?php declare(strict_types=0);
try { echo [1][9]; } catch (Throwable $error) { echo get_class($error), '|'; }
try { echo 'abc'[9]; } catch (Throwable $error) { echo 'S'; }
$a = ['x'=>1]; $reference = &$a['x']; unset($a['x']);
try { echo $a['x']; } catch (OutOfBoundsException $error) { echo 'R'; }
echo ':', $reference, '|';
$probe = []; $probe['new'] ??= 7; echo $probe['new'];
$unsetProbe = ['existing'=>0]; unset($unsetProbe['missing']); $nested = [];
$nested['outer']['inner'] = 8; echo ':', $nested['outer']['inner'], '|';
$created = ['existing'=>0]; $newReference = &$created['created']; $newReference = 9; echo $created['created'], '|';
try { $a['absent'] += 1; } catch (OutOfBoundsException $error) { echo 'C'; }
$words = explode(',', 'one,two');
try { echo $words[8]; } catch (OutOfBoundsException $error) { echo 'B'; }
