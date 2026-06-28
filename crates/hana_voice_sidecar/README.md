# Hana Voice Sidecar

Prototype sidecar for the Hana voice-art loop.

Run the feedback UI from the workspace root:

```sh
cargo run -p hana_voice_sidecar --example voice_sidecar
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

Generate macOS `say` fixtures for offline VAD checks:

```sh
cargo run -p hana_voice_sidecar --example voice_say_fixtures -- \
  --out-dir /tmp/hana_voice_sidecar_speech_fixtures \
  test testing reset okay rest
```

Replay those WAVs through the same session state machine used by the sidecar:

```sh
cargo run -p hana_voice_sidecar --example voice_vad_replay -- \
  /tmp/hana_voice_sidecar_speech_fixtures/test.wav \
  /tmp/hana_voice_sidecar_speech_fixtures/testing.wav
```

With no paths, `voice_vad_replay` scans `HANA_ART_RUN_DIR/audio` or the default
`../hana/run/art/audio` directory. It appends synthetic tail silence by default
so short generated clips can still exercise the settle-and-commit path.

Run Apple Speech directly over named fixture files:

```sh
cargo run -p hana_voice_sidecar --example voice_stt_files -- \
  /tmp/hana_voice_sidecar_speech_fixtures/test.wav \
  /tmp/hana_voice_sidecar_speech_fixtures/testing.wav
```

The expected phrase is derived from the filename, with underscores treated as
spaces.
