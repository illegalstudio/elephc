<!doctype html>
<html>
<body>
<h1><?= "Products" ?></h1>
<ul>
<?php
$products = ["Ada" => 12.5, "Grace" => 8.0, "Linus" => 20.0];
foreach ($products as $name => $price) {
?>
  <li><?= $name ?> — $<?= $price ?></li>
<?php
}
?>
</ul>
</body>
</html>
