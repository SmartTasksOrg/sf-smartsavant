<?php
// SmartSavant for PHP: the engine, gates, schema checks, adapters (stub, scripted, replay), exporters,
// importer and audit of the Python reference (src/sf_smartsavant), in namespace SmartTasks\SmartSavant.
//   require 'ports/php/src/SmartSavant.php';
declare(strict_types=1);

namespace SmartTasks\SmartSavant;

const VERSION = '0.1.0';

require_once __DIR__ . '/Py.php';
require_once __DIR__ . '/Gates.php';
require_once __DIR__ . '/Schema.php';
require_once __DIR__ . '/Engine.php';
require_once __DIR__ . '/Exporters.php';
require_once __DIR__ . '/Audit.php';
