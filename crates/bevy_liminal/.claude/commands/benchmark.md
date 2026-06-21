**Arguments**: $ARGUMENTS

If no arguments are provided, output the following usage statement and stop:

```
Usage: /benchmark <name> [baseline]

  <name>       Name for this benchmark run (e.g. 0_baseline, 1_my_optimization)
  [baseline]   Optional baseline directory name to compare against

Examples:
  /benchmark 0_baseline                              — collect results only
  /benchmark 1_my_test 0_baseline                    — collect and compare
```

Parse arguments: first argument is `<name>`, second (optional) is `<baseline>`.

Run the benchmark 3 times in auto mode with `--release`, then collect the results into a named directory.

## Steps

1. Run the following command 3 times sequentially (each must complete before starting the next). **You MUST use `dangerouslyDisableSandbox: true`** on each Bash call because Bevy needs GPU/Metal access:
   ```
   BENCHMARK_AUTO=1 cargo run --release --example benchmark
   ```
   Use a 600000ms timeout for each run.

2. After each individual run completes, report the **Entities10000 on** and **Entities50000 on** FPS and median frame time (ms) from that run's output.

3. After all 3 runs complete, create the directory `results/<name>/` if it doesn't exist.

4. Move all CSV files from `results/` (not from subdirectories) into `results/<name>/`.

5. Report how many CSVs were moved and confirm the directory name.

6. **Only if `<baseline>` was provided**: Run the comparison against the baseline:
   ```
   python3 scripts/compare_benchmarks.py --extreme results/<name> results/<baseline> > results/<name>/comparison.md
   ```

   Then show the comparison table in this format:

   ```
   | Entities | State | <name> (ms) | <baseline> (ms) | Delta |
   ```

   With rows for each of 7 entity counts (1, 5, 10, 100, 1000, 10000, 50000) × 2 states (off, on), using median ms values and delta ms with percentage.

7. If no baseline was provided, just report "Results collected in results/<name>/. Run `/benchmark <new_name> <name>` to compare against this baseline."
