<?php

require_once 'src/bootstrap.php';

use Showcases\Doom\App\Application;
use Showcases\Doom\App\Config;

$app = new Application(new Config());
$app->run();
