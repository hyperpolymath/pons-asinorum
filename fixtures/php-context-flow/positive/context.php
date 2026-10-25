<?php function query($db) { $x = htmlspecialchars($_GET["q"]); mysqli_query($db, $x); }
// SPDX-License-Identifier: MPL-2.0
