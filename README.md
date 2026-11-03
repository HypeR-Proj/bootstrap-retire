<!--
SPDX-FileCopyrightText: 2026 roolrz
SPDX-License-Identifier: Apache-2.0
-->

# bootstrap-retire

One-shot predecessor retirement service.

Receives only the predecessor process, task-group, root-directory and console-drain
capabilities named in the handoff. Stops those processes, drains their console and
detaches the old mount after backing references end.

See the [implementation entry](src/main.rs) and [component declaration](component.json)
for the public interfaces, build dependencies and delivered artifacts.

## Boundaries

This is launched by the startup ownership protocol, not a general process killer or user
command. A timeout does not authorize releasing resources still held by a predecessor.

## Build and test

Use this checkout inside the [repo-managed workspace](https://github.com/HypeR-Proj/HypeR).
From this repository directory:

```sh
make
make test
```

An absolute `OUT=/path/to/out` selects a shared output directory. The module
entry delegates to build/system, which prepares the installed SDK and declared
dependencies. Native binaries require HypeR; host module checks cover the tests
selected by component.json. A standalone clone does not contain the workspace’s
other source repositories or Native SDK.
