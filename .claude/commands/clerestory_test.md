# Clerestory test

Run Clerestory's self-contained test controller. Do not reproduce its build,
monitor discovery, test ordering, polling, or cleanup steps in the agent.

From the workspace root:

```sh
python3 crates/bevy_clerestory/tests/scripts/run_suite.py --automated \
  --hardware-profile crates/bevy_clerestory/tests/config/hardware.local.json \
  $ARGUMENTS
```

If the local hardware profile does not exist, omit `--hardware-profile`. The
controller will run application-state cases and report physical reconnect cases
as unavailable. Use `--dry-run` to list every case, its interaction requirement,
evidence source, and availability without building an app or changing a
display. Use `--assisted` for cases that require one human action; it is a
separate run and never occurs during `--automated`.

The controller itself must:

- state the automated restore count, physical case count, and physical probe
  count before those partitions begin;
- report the three test/lint preflight gates as `Preflight 1/3` through `3/3`;
- print progress for every prebuild, discovery gate, restore case, probe case,
  and physical case;
- continue collecting safe independent results after an ordinary assertion
  failure;
- preserve JSON, Markdown, child logs, snapshots, and ordered records in the
  artifact directory it prints;
- turn configured monitor hardware back on and stop only processes it started,
  including after interruption or failure.

Return the controller's exit status and report paths. Do not parse console text
to invent another result summary, move an editor or terminal, inject pointer or
keyboard input, or rerun individual Cargo commands in place of the controller.
