<?php
function freshStringScope(): void { eval('$value="before"; $value="after"; echo $value, "|";'); }
function openStringScope(): void { mixed $value = null; eval('$value="before"; $value="after"; echo $value, "|";'); }
freshStringScope(); openStringScope(); freshStringScope(); openStringScope();
