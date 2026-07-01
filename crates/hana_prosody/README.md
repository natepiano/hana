# Hana Prosody

Prototype sidecar for the Hana voice-art loop.

Run the feedback UI from the workspace root:

```sh
cargo run -p hana_prosody --example voice_sidecar
```

Press space to start recording from the default macOS input device. Press space
again to stop recording and send that captured window to Apple Speech. The
sidecar does not infer speech boundaries while the user is talking; the keyboard
presses define the audio window.

Committed transcripts are appended to `../hana/run/art/inbox.jsonl` by default.
Temporary WAV files live under `../hana/run/art/audio` while Apple Speech is
working, and successful transcripts remove their WAV file after the inbox write
succeeds. Set `HANA_ART_RUN_DIR` before launch to choose these paths. A running
sidecar can also be redirected over BRP with `hana_voice/set_runtime`, which is
what Hana's `scripts/tell_me.py` uses when it reuses an already-open sidecar.

On macOS, Apple Speech is the STT backend. Set `HANA_STT_LOCALE` to choose a
recognizer locale, and set `HANA_STT_REQUIRE_ON_DEVICE=1` when the loop must
fail instead of using a network-backed system recognizer.

The generated-speech diagnostics live in ignored integration tests because they
depend on macOS `say`, `afconvert`, and Apple Speech authorization:

```sh
cargo nextest run -p hana_prosody --run-ignored ignored-only
```

Those tests generate short `say` fixtures and run Apple Speech over the named
fixtures with the expected phrase derived from the filename.
