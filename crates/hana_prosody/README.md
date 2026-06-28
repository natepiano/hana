# Hana Prosody

Prototype sidecar for the Hana voice-art loop.

Run the feedback UI from the workspace root:

```sh
cargo run -p hana_prosody --example voice_sidecar
```

Press space to toggle continuous transcription. While the loop is on, the app
records from the default macOS input device, detects speech with Earshot VAD,
proposes final windows after silence, and probes active windows when background
noise keeps the VAD from settling. Probe transcripts are tentative: a later
probe with more words replaces the current tentative transcript, and stable
tentative text is appended to the inbox when no better probe arrives. Unusable
candidates are discarded and the loop keeps listening.

Committed transcripts are appended to `../hana/run/art/inbox.jsonl` by default.
Temporary WAV files live under `../hana/run/art/audio` while Apple Speech is
working, and successful transcripts remove their WAV file after the inbox write
succeeds. Set `HANA_ART_RUN_DIR` to redirect these paths.

On macOS, Apple Speech is the STT backend. Set `HANA_STT_LOCALE` to choose a
recognizer locale, and set `HANA_STT_REQUIRE_ON_DEVICE=1` when the loop must
fail instead of using a network-backed system recognizer.

The generated-speech diagnostics live in ignored integration tests because they
depend on macOS `say`, `afconvert`, and Apple Speech authorization:

```sh
cargo nextest run -p hana_prosody --run-ignored ignored-only
```

Those tests generate short `say` fixtures, replay the WAVs through the same
session state machine used by the sidecar, and run Apple Speech over the named
fixtures with the expected phrase derived from the filename.
