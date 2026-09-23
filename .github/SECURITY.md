<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Security policy

Pons is a development-stage offline scanner. Report vulnerabilities privately
through GitHub's private vulnerability reporting for hyperpolymath/pons-asinorum,
or email j.d.a.jewell@open.ac.uk. Include the affected revision, operating system,
a minimal reproducer and expected impact. Do not put credentials or private
source code in public issues.

The current development branch receives fixes; no stable supported release has
been published. Core scans never execute the inspected project. Optional document
and spelling adapters execute explicitly selected local tools with bounded input,
output and duration. Use the documented offline container for isolation.
See [the deployment guide](../build/container/README.adoc).
