# Hana Prosody

Audio and transcription primitives for Hana voice input.

Run the feedback UI from the workspace root:

```sh
cargo run -p hana_prosody --example voice_sidecar
```

Press space to start recording from the default macOS input device. Press space
again to stop recording and send that captured window to Apple Speech. The demo
does not infer speech boundaries while the user is talking; the keyboard presses
define the audio window.

The library does not own Hana's transcript or renderer command files. Clients
receive transcription outcomes and decide whether to write JSONL, keep audio, or
discard everything. Apple Speech still requires a WAV file, so
`spawn_transcription` writes a temporary WAV under the caller-provided scratch
directory and removes it after transcription. Call `write_wav` directly only
when a client explicitly wants to keep an audio artifact.

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
