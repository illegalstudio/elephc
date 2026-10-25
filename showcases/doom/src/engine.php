<?php

// Everything except the window: WAD reading, maps, play and the 8-bit renderer.
// The tests load only this file, so they need neither SDL nor a display.

require_once 'Data/Vertex.php';
require_once 'Data/Linedef.php';
require_once 'Data/Sidedef.php';
require_once 'Data/Seg.php';
require_once 'Data/SubSector.php';
require_once 'Data/Sector.php';
require_once 'Data/Node.php';
require_once 'Data/Thing.php';
require_once 'Data/Mobj.php';
require_once 'Data/Mover.php';
require_once 'Data/LightEffect.php';
require_once 'Data/Button.php';

require_once 'Support/Trig.php';
require_once 'Wad/Wad.php';

require_once 'Graphics/Animations.php';
require_once 'Graphics/Palette.php';
require_once 'Graphics/Textures.php';
require_once 'Graphics/Flats.php';
require_once 'Graphics/Sprites.php';

require_once 'Map/MapData.php';
require_once 'Map/MapLoader.php';

require_once 'Bsp/BspWalker.php';

// keyboard state is read from SDL's array through a plain pointer, no SDL call
require_once 'SDL/Input.php';

require_once 'Play/ThingTypes.php';
require_once 'Play/Things.php';
require_once 'Play/Collision.php';
require_once 'Play/Specials.php';
require_once 'Play/Player.php';

require_once 'Render/Screen.php';
require_once 'Render/LightTables.php';
require_once 'Render/BspRenderer.php';
require_once 'Render/SpriteRenderer.php';
require_once 'Render/Hud.php';
require_once 'Render/Automap.php';
require_once 'Render/Renderer.php';

require_once 'App/Config.php';
require_once 'App/Session.php';
