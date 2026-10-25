<?php

namespace Showcases\Doom\Tests\Fixture;

use Elephc\Testing\TestCase;
use Showcases\Doom\App\Config;
use Showcases\Doom\App\Session;

// Boots a Session on a freshly written TestWad and removes the file afterwards.
abstract class SessionCase extends TestCase {
    protected string $wadPath = "";

    protected function boot(): Session {
        TestWad $wad = new TestWad();
        $this->wadPath = $wad->write();
        Config $config = new Config();
        $config->wadPath = $this->wadPath;
        $config->verbose = false;
        Session $session = new Session($config);
        $this->assertTrue($session->boot(), "the test WAD boots");
        return $session;
    }

    protected function tearDown(): void {
        if ($this->wadPath !== "") {
            unlink($this->wadPath);
        }
    }
}
