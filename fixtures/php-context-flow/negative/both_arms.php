<?php function render($b) { $x = $_GET["x"]; if ($b) { $x = htmlspecialchars($x); } else { return; } echo $x; }
// SPDX-License-Identifier: MPL-2.0
