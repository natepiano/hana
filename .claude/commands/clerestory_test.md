# Clerestory test

Run Clerestory's self-contained test controller (`run_suite.py`). Do not reproduce its
build, monitor discovery, test ordering, polling, or cleanup steps in the agent — the
controller owns all of that. This command must work the same way on every platform: pick
the right hardware profile, satisfy any elevation the platform needs (or tell the user
exactly how to assist), then run the controller and report its result.

## Command

From the workspace root:

```sh
python3 crates/bevy_clerestory/tests/scripts/run_suite.py --automated \
  --hardware-profile <PROFILE> \
  $ARGUMENTS
```

`--dry-run` lists every case, its interaction requirement, evidence source, and
availability without building an app or changing a display. `--assisted` is a separate
run for cases needing one human action; it never occurs during `--automated`.

## Step 1 — select the hardware profile

Choose `<PROFILE>` as the first of these that exists on disk, then substitute its path:

- **Windows:** `crates/bevy_clerestory/tests/config/hardware.windows-vm.local.json`
- **otherwise (macOS/Linux):** `crates/bevy_clerestory/tests/config/hardware.example.json`

If none exists, omit `--hardware-profile` entirely — the controller runs the
application-state cases and reports physical and cross-DPI cases as unavailable. A
profile names the machine's monitor and its power commands; never invent one.

## Step 2 — Windows only: elevation for the display-backed partitions

On Windows the cross-DPI and physical-reconnect partitions enable/disable the virtual
display driver (VDD), which requires Administrator; the plain restore cases do not. The
controller auto-provisions the mixed-DPI scale itself (enables the VDD, forces it to
scale 1 via the DisplayConfig DPI API, disables it after) — there is nothing to set up by
hand, and the VDD persists across reboots.

First determine whether the current shell is already elevated:

```sh
powershell.exe -NoProfile -Command "([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)"
```

- Prints `True` → run the controller command directly.
- Prints `False` → do **not** run a degraded non-elevated pass (the VDD partitions would
  fail). Instead run it through the elevation bridge if present, else instruct the user:
  1. Check for the bridge task: `schtasks /query /tn ClerestoryVddTest` (run
     `powershell.exe` with `MSYS2_ARG_CONV_EXCL='*'` so the `/tn` flag survives).
  2. **If the task exists**, run the controller through it: write a `C:\tmp\vdd_task.cmd`
     that `cd`s to the workspace root, deletes `C:\tmp\vdd_task.done` and
     `C:\tmp\vdd_task.out`, runs the Step-1 controller command redirecting to
     `C:\tmp\vdd_task.out`, then `echo done> C:\tmp\vdd_task.done`; trigger it with
     `schtasks /run /tn ClerestoryVddTest`; poll for `C:\tmp\vdd_task.done`; then read
     `C:\tmp\vdd_task.out` for the controller's output, summary, and `SUITE EXIT` status.
  3. **If the task does not exist**, tell the user to run the Step-1 command in an
     elevated terminal (or to recreate the `ClerestoryVddTest` bridge). State the exact
     command. Do not proceed non-elevated.

macOS/Linux need no elevation for this controller.

## The controller itself must

- state the automated restore count, physical case count, and physical probe count
  before those partitions begin;
- report the three test/lint preflight gates as `Preflight 1/3` through `3/3`;
- print progress for every prebuild, discovery gate, restore case, probe case, cross-DPI
  case, and physical case;
- continue collecting safe independent results after an ordinary assertion failure;
- preserve JSON, Markdown, child logs, snapshots, and ordered records in the artifact
  directory it prints;
- turn configured monitor hardware back on and stop only processes it started, including
  after interruption or failure.

Return the controller's exit status and report paths. Do not parse console text to invent
another result summary, move an editor or terminal, inject pointer or keyboard input, or
rerun individual Cargo commands in place of the controller.
