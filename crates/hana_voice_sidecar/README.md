# Hana Voice Sidecar

Prototype sidecar for the Hana voice-art loop.

Run the feedback UI from the workspace root:

```sh
cargo run -p hana_voice_sidecar --example voice_sidecar
```

Press space to toggle continuous transcription. While the loop is on, the app
records from the default macOS input device, detects speech with a simple RMS
gate, proposes candidate windows after silence, writes each candidate as a WAV
file, and lets Apple Speech decide whether it contains a usable transcript. A
valid transcript is appended to the inbox; an unusable candidate is discarded
and the loop keeps listening.

Committed transcripts are appended to `../hana/run/art/inbox.jsonl` by default.
Temporary WAV files live under `../hana/run/art/audio` while Apple Speech is
working, and successful transcripts remove their WAV file after the inbox write
succeeds. Set `HANA_ART_RUN_DIR` to redirect these paths.

On macOS, Apple Speech is the STT backend. Set `HANA_STT_LOCALE` to choose a
recognizer locale, and set `HANA_STT_REQUIRE_ON_DEVICE=1` when the loop must
fail instead of using a network-backed system recognizer.
