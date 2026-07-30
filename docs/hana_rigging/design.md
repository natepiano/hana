---
area: "[[transport]]"
backlog_alignment: ⭐⭐⭐⭐⭐
backlog_effort: ⭐⭐⭐⭐⭐
backlog_goal: 1 - Ship Hana
backlog_impact: ⭐⭐⭐⭐
backlog_score: 29
backlog_rank: 6
backlog_urgency: ⭐⭐
category:
  - "[[issue structure#feature|feature]]"
date_created: "[[2026-07-22]]"
date_modified: "[[2026-07-28]]"
project: "[[hana]]"
return:
  - "[[issues - hana.base]]"
  - "[[issues.base]]"
see_also:
  - "[[implement transport controls]]"
  - "[[transport and synchronization]]"
  - "[[hardware streamdeck integration]]"
  - "[[physical audio rendering architecture]]"
  - "[[chromatik competitive research]]"
  - "[[typed camera open failures]]"
  - "[[restore primary window after reconnect]]"
  - "[[versioned save format]]"
stage: backlog
status: open
tags: [issue]
---
You bought that device — hook it up and make it a transport controller. Solve some hardware problems, man.

This issue is now **v1 of `hana_rigging`**, a flexible hardware interface module. The Stream Deck is the first device through it, not the point of it.

## Executive summary

### The problem

Hana already talks to four kinds of hardware — monitors, screens, cameras, and now a Stream Deck — and has solved the same four problems separately each time: *which unit is this*, *is it here right now*, *what do we do when it leaves*, *what do we do when it comes back*. The duplication is not a guess. The staleness check that guards a stale async open is **byte-for-byte identical** in `screen/session.rs:295-299` and `secondary.rs:342-346`, written independently. Two more copies exist in less similar form. Every new device kind pays this cost again, and the copies drift.

`hana_rigging` is where that logic lives once.

### How it works

**The kernel touches no hardware.** It cannot open a USB device, enumerate a display, or read a camera. All it does is hold a table of what is bound to what, and decide what should happen when that changes. This is why it can serve monitors and DMX lasers from the same code: scanning for monitors has to run on the main thread inside winit, and no crate that also serves Art-Net could carry that.

**Providers do the touching.** One per hardware family — a monitors provider that asks winit, a camera provider that asks nokhwa, a Stream Deck provider that polls USB. Each is a plain Rust object registered with `add_device_provider`.

**The loop, once per tick, in `RiggingSet::Reconcile`:**

1. The kernel asks each provider for a **scan** — the complete set of what that provider can currently see. Not "here's what changed": providers are bad at tracking change, and the whole set is what they can honestly report. The kernel computes the difference itself.
2. Differencing the scan against the table produces arrivals and departures. Every device is a Bevy **entity** with reflected components, so it is visible to queries, to observers, and to BRP.
3. Matching a saved binding to a live unit is **exact match or nothing**. There is no "closest", no "within 200 points", no "first one that looks right". Every current bug in this area comes from a fallback that returned something plausible instead of nothing.
4. When a device leaves, its **policy** says what to keep. When it returns, the policy says whether to act and *toward what state*.
5. Acting is **not blocking**. `apply` starts the work; `poll` is polled with a deadline. Every poll re-checks that the unit is still the one authorized and still safe to drive — because between starting and finishing, someone can walk into a laser beam.
6. Consumers ask **what a device can do**, not what it is. There is no device class hierarchy.

### The answer to "how do we share capabilities through code"

**There are no hardware classes.** No `trait Device`, no base class, no `HardwareKind` enum. A capability is a **component** — `Illuminants` means "has things that emit light", whether that is a Stream Deck key's RGB backlight, an LED ring, or a DMX dimmer. Code that dims lights queries `Illuminants` and gets all three without knowing any of them exist. Sharing happens through queries, and a device gains a capability by having a component inserted, which can happen at runtime for a device that describes itself.

Rejected alternative, for the record: `Box<dyn Device>` with capability traits. It cannot be `Reflect`, so BRP goes blind to every device — the exact failure already logged for `EnvironmentMapLight`. It cannot gain a trait at runtime. And it cannot host main-thread winit work.

### Key types

**Three identifiers, and confusing them is the bug this crate exists to prevent:**

| Type | What it names | Lifetime |
|---|---|---|
| `DeviceId(u64)` | a live unit, this run | process-local, **never saved** |
| `DeviceKey { kind: DeviceKind, id: DeviceIdSource }` | a unit durably — either `Reported { scheme: SchemeName, value: ReportedId }` (the unit told us, e.g. a USB serial) or `Synthesized { digest: Digest }` (we derived it from descriptors) | **saved to disk**; the variant *is* the trust level (R6, D6) |
| `RigKey` | the **role**, not the hardware — "the left projector" | outlives every unit that ever fills it |

**State — four axes, deliberately not collapsed into one enum:**

| Type | Question it answers | Values |
|---|---|---|
| `DeviceIdentity` | do we know *which unit* this is? | `Proven` · `RestoreOnly` · `Authored` · `Displaced` · `WrongUnit` · `Unverified(reason)` |
| `Presence` | is it here? | `Present` · `Absent` · `Unreachable { since }` |
| `Claim` | do we have it, or does something else? | `Held` · `Free` · `Contended { holder }` · `Blocked { gate }` · `NotApplicable` |
| `Arming` | is it *safe* to drive right now? | `Ready` · `Inhibited { reason }` · `NotApplicable` |

They stay separate because they fail differently and are reported by different parties. `Devices::armable()` is the single function that folds all four and answers *may we drive this*. Nothing else is allowed to decide that.

The type keeps its name but its variants are **conclusions reached after reconciliation**, not raw evidence, so they can never contradict the key (D3). `Proven` may arm; `RestoreOnly` may restore a saved configuration but never drive output. Why named variants rather than a boolean: `Authored` is a hand-patched DMX address, which is *more* trustworthy than a discovered display because it cannot lie. `Displaced` is "the saved key matched nothing, but something of the right kind is in the same slot" — needs a human, never auto-adopted. `WrongUnit` is "the saved place matched and the unit sitting in it is a different one" — the most common failure in live lighting, and the reason RDM exists, but it needs no lighting hardware to occur: a camera unplugged from a port and replaced by a different camera produces it exactly.

**Addressing — one device, many things:**

- `Slot(String)` — a Stream Deck key, an encoder, a DMX channel offset, an LED ring.
- `EndpointRef { device: DeviceKey, slot: Slot }` — what a binding actually points at.

**Policy — two axes:**

- `Retain` — what to keep: `Nothing` · `LastIntentional` (the state we last set) · `Declared(blob)` (a state you name up front: blackout, home position, shutter closed, laser safe state).
- `Act` — when: `Never` · `OnRequest` · `OnReturn`.

Default is `{Nothing, Never}`. **The kernel never silently arms anything.**

**Recovery — replaces two hand-rolled generation counters:**

- `RecoveryPhase` — `Nominal` · `AwaitingDevice` · `Recovering(id)` · `PastDeadline(id)` · `Retired`.
- `Apply` — carries `expected: DeviceId` and `revision`, both re-checked on every poll, so an attempt authorized against one unit can never land on a different one.
- `ApplyProgress` — `Pending` · `Done` · `Failed` · `Aborted` · `Substituted`. `Aborted` is terminal and **never auto-retries**; it is what a safety gate closing mid-attempt produces, and collapsing it into `PastDeadline` would re-permit the retry.

**The provider contract, entire:**

```rust
pub trait DeviceProvider: Send + Sync + 'static {
    /// The configuration this provider accepts. Its own type, its own vocabulary.
    type Parameters: Reflect;

    fn scan(&mut self, world: &mut World) -> DeviceScan;   // the whole set, or Unchanged
    fn capture(&mut self, ..., endpoint: &EndpointRef) -> Option<Self::Parameters>;
    fn apply(&mut self, ..., params: &Self::Parameters, apply: ApplyId); // starts, doesn't block
    fn poll(&mut self, ..., apply: ApplyId) -> ApplyProgress;            // polled

    /// Did the request take effect? Defaulted: byte-equal means `AsRequested`,
    /// otherwise `StillConverging`. A camera overrides it so 59.94 satisfies 60.
    fn fulfillment(&self, requested: &Self::Parameters, observed: &Self::Parameters)
        -> Fulfillment { .. }
}
```

Device configuration crosses the boundary as a value the kernel **stores without interpreting** — held as `Captured::{Writable, Frozen}` over a reflected value. That is what keeps the kernel free of every device vocabulary — resolution, refresh rate, pan/tilt, key image — while still being able to hold it and put it back. Providers live in other crates, so the kernel keeps them as `Box<dyn ...>`; because an associated type cannot be reached through `dyn`, `add_device_provider` wraps the typed provider in an adapter written once inside `hana_rigging` (D4).

### What v1 proves

`bevy_clerestory` is refactored onto it **first** as consumer #1 (R3), then screens and cameras migrate, before the Stream Deck and before any publish. One provider proves nothing; the entire argument is that two independently written machines collapse into one. The first integration test — a display and a webcam unplugged in the same run, against one kernel — is where the claim is either true or false.

## Why a crate and not a one-off

Hana already solves the same device problems four separate times, sharing nothing:

| | Identity key | Presence lifecycle | Failure typing |
|---|---|---|---|
| Cameras (`hana_video`) | device **name**; index re-resolved and re-verified after open | `SessionState::{Opening, Live, Disconnected}` + generation counter | `CameraOpenError { name: Option<String>, reason: String }` |
| Screens (`hana_video`) | platform display id (`CGDirectDisplayID`) | `StreamState::{Opening, Live, Disconnected}` + generation counter | bare `String` |
| Screen panels (`hana/src/screens`) | **logical position ±200 pt** | `ScreenConnection` mirrored for edge detection | reason surfaced; cause **guessed** |
| Monitors (`bevy_clerestory`) | `MonitorId` | — | — (designed, **not built**) |

Cameras and screens spell the session state machine and the stale-open generation guard **identically and independently**. Two solutions converging without shared code is the argument for a shared core.

"Identity is not a positional index" was rediscovered **four times** in camera code alone (`camera.rs:48-58`, `stream/mod.rs:90-95`, `render.rs:30-34`, `secondary.rs:6-11`).

**The sharpest single piece of evidence:** `hana_video::screen::ScreenSource` carries `pub id: u32` — the real `CGDirectDisplayID` — and its own doc comment says it is published "so a consumer can match the texture back to a monitor **by geometry**". The only use of `source.id` in the whole hana app is printing it as a debug label row (`hana/src/screens/layout.rs:195`). Matching goes through `SCREEN_FEED_MATCH_TOLERANCE_POINTS = 200`, a nearest-neighbour geometry fudge. **The stable identity is already present, already published, and unused.** Two monitors with origins closer than 200 pt would alias.

## Architecture: a policy kernel, not a device manager

`hana_rigging` **discovers nothing and performs no I/O.**

The obvious design — "rigging owns presence" — fails on one hard fact: the monitor scan is main-thread winit (`NonSendMarker`, `WinitMonitors`, the `WINIT_WINDOWS` thread-local). Rigging cannot scan monitors without pulling `bevy_winit` into a crate that also serves DMX fixtures and lasers.

So invert the direction:

- **Providers scan and push the whole set in.** `bevy_clerestory` becomes the monitor provider; `hana_video` the camera and screen provider; a new kind crate the Stream Deck provider.
- **The kernel owns** device identity, the id-keyed presence diff, the availability policy, and the recovery state machine — and hands decisions back.

This dissolves most of the coupling that made extraction look expensive: the kernel owns no events, no persistence format, no scan, and never sees a window key. Cross-crate ordering reduces to a single public `RiggingSet::Reconcile`.

### Two things that must *not* be in the core

1. **`FallbackAndReturn` is not a core policy.** Core is `Ignore | Notify | Reacquire`. "The OS relocated it to a substitute and we'll bring it back" is a *provider capability* — a DMX fixture has no substitute universe. A core enum with a variant only windows can use is a leaked abstraction.
2. **Window-only recovery phases stay private to clerestory**, and cannot be an associated type either: a generic phase enum breaks `reflect_auto_register` for every BRP-observable type.

### The rule that decides membership

> The rigging layer never models what a device is showing or doing — only whether it is there, and what it was configured to be.

## What the core must survive

Seven reference devices, chosen to be maximally unlike each other. A concept belongs in the kernel only if it is **protocol-free**, needed by **at least four of seven** with *natural* absence for the rest, and its omission would **duplicate policy code**.

Stream Deck over HID · Art-Net DMX fixture · laser with safety interlock · camera · projector/monitor · self-describing serial/BLE sensor · **multichannel audio interface driving a speaker array**.

The audio device is mandatory, not decorative. [[physical audio rendering architecture]] is the most rigorous device model in the vault — it asks this crate's central question verbatim ("How should physical output devices be discovered, identified, persisted, and reassigned?") and forces three things monitors never do: **N entities : 1 device** (speaker entities each map to one hardware output channel), **cross-device grouping constraints** (a speaker group a source pans across must be driven by one interface on one machine), and **measured config held separately from applied correction** (or calibration re-runs and hand edits fight over one value). Designing against monitors and cameras alone lands strictly weaker than a doc already in the backlog.

### Essential — belongs in the kernel

- Identity is a stable backend key, never a positional index.
- Re-verify identity after an async open (TOCTOU on the device table).
- `Opening / Live / Disconnected` session state machine.
- Generation counter rejecting stale async opens.
- Async open with an outcome channel; never block the frame.
- **Enumeration failure ≠ device absence** (stated as a rule twice in existing code).
- Last-good output and stable handles survive disconnect.
- Bounded waits everywhere, so teardown always completes.
- Bounded attempt with provider-supplied arrival evidence — the kernel bounds the attempt and counts consecutive failures; the provider says what counts as settled. ("Has a frame arrived?" needs kind knowledge and cannot be kernel work.)
- Repeated-failure escalation, **self-clearing on recovery**.
- Failure carries identity *and a machine-classifiable reason*.
- Edge detection from mirrored last-known state, seeded disconnected so the first frame is not a transition.
- Decline-the-capability as a first-class alternative to a placeholder.
- Timer backstop plus OS-event fast path for reconcile.

### Incidental — must not become core abstractions

Keying identity on a device *name* (a nokhwa limitation); pixel formats and texture packing (payload, not device); the `/tmp` advisory lock (the generalizable form is "some buses require serialized open"; the file-lock mechanism is not); "only relaunching clears it" (a ScreenCaptureKit bug); non-send sessions forcing exclusive systems (a Bevy artifact — consider an owning thread plus a `Send` channel); permission-as-error-string (permission denial is *essential* to model, the string spelling is the artifact); the 200-pt tolerance, in any form; primary/secondary camera roles leaking into the device layer.

### How much of the asset-loading vocabulary transfers

> **A device's availability is a level, not an edge.**

[[asset loading]]'s `hana_lading` has a one-shot per-type terminal event; devices need a repeatable per-identity signal. **Transfers:** severity chosen at registration → required / optional / decline-the-capability; the type-erased failure event feeding one durable recorder → a device-fault log; the `Option`-returning capability accessor → "give me this device's live handle or nothing". **Does not transfer:** the closed batch, the completion count, the exactly-one-terminal-event contract, the never-cleared record, the whole notion of a startup join point. Screens' stalled-list self-clears on recovery and that is correct for devices; lading's permanence is right only because assets cannot recover.

## Device facts that stress the model

From the direct-HID survey — these are why the abstraction can't be monitor-shaped:

1. **Sleep/resume is a distinct failure mode from unplug.** On macOS host suspend the HID handle breaks and must be closed and reopened with a full re-render, and it is not detectable the same way as a disconnect. Hana runs on laptops; this fires constantly. Availability needs a resume path.
2. **The identity token can change under us.** Stream Deck serials have been observed mutating across replug (bitfocus/companion#1173), and serial length changed 12→14 across a firmware revision. That is neither "can't tell" nor "user authored" — it is a third case: *previously verified, now different*.
3. **Devices host devices, and transport is not fixed per kind.** The Network Dock hosts child devices over TCP with no real USB product id; the Studio reports `SUPPORTS_CHILD_DEVICES`. A flat device list and a USB-shaped address both break. A Corsair keyboard runs the Stream Deck gen2 protocol — **protocol ≠ vendor**.
4. **Recovering from contention is an action, not a diagnosis.** On open, a zeroed reset report must be written to abort a half-finished image transfer left by a *previous owner*. "Claim succeeded" does not imply "device is in a known state."
5. **Claiming is a contest with no good outcome.** hidapi seizes by default on macOS, and Apple's `IOHIDDevice::handleOpen` shows a seizing open *succeeds* over the already-running Elgato app while silently disabling its input queues. Shared mode is worse: both processes drive the device with no arbitration. Contention is the **normal operating state, not an error**, and "we opened it" does not mean "we should have".
6. **Every transport failure arrives as one opaque string.** Contention, unplug, and permission denial all surface as `HidError::HidApiError { message: String }` wrapping a hex `IOReturn`. This is the **second** device kind to land in the same hole that produced `VIDEO_UNAVAILABLE_LIKELY_CAUSE` — a cause *guessed* in the UI because the device layer returns prose. Two independent kinds forced into the same failure means typed classification is centralization evidence, not taste.
7. **Availability is polled and inferred, never notified.** There is no hot-plug anywhere in the hidapi stack; the production reference re-enumerates every 10 s and treats a read-loop error as the only disconnect signal. The core must be **poll-shaped with notification as a fast path**, not the reverse. Monitors, which do get OS events, are the exception in the reference set — designing around them would have inverted this.
8. **A blocked device is indistinguishable from an absent one.** On Apple-silicon laptops the "Allow accessory to connect" prompt makes a denied device simply not enumerate. `Blocked` is therefore not observable in the device table at all — it is only inferable from *history*: we hold a saved key, the platform has a permission gate, and nothing matching enumerated. That inference needs durable identity, which is another thing `DeviceKey` pays for.

Resolved by the same survey, and worth recording because it was an open question: **Input Monitoring TCC does not apply.** Apple's kernel gates only Keyboard, Mouse, and TouchPad usages; the Stream Deck is a Consumer-page device. Unsigned `cargo run` reaches it with no entitlement and no prompt.

Also: outbound feedback is several distinct channels (key images, per-key RGB, 24-segment LED rings), an input channel need not be a control at all (the Studio's NFC reader shares the button report stream), and the real seam is ~6 **protocol families**, not 17 product ids — kind crates should be shaped around families, not marketing names.

## Boundary with existing input handling

`bevy_enhanced_input` owns the human-input device layer (keyboard, mouse, gamepad) and `bevy_kana::input` owns the action and keybinding macros. **`hana_rigging` must not reimplement either.**

The split to hold: rigging owns presence, identity, claim and contention, availability policy, and outbound rendering *to* the device. Inbound button events feed the existing action system via `bind_action_system!` — which `hana/src/transport.rs:18` already uses to bind a transport action. This keeps the v1 hana-side diff small and keeps the crate about hardware rather than input routing.

## Sequencing — read this before committing

**This section was written before the recovery work landed. Its premise is now false; the current facts and the single authoritative order are below. The struck text is kept because the reasoning it records is still why the kernel is not being born inside clerestory.**

~~**Nothing in clerestory's recovery design is implemented.** `WindowRecovery`, `MonitorIdentity`, `Unverified`, `CapturedWindowState`, `RestoreApply`, `TopologyRevision`, `by_id`, `entity_by_id` — all return **zero** occurrences in `bevy_clerestory/src` (verified). Hana pins `bevy_clerestory = "0.1.1"` from crates.io, which has no `MonitorId` at all; monitor events exist only in unreleased `0.2.0-dev`.~~

~~This is **good news**: there is nothing to extract, no published recovery API to break, no on-disk format to migrate, no double refactor.~~ The half that survives: the registry is still *born* in `hana_rigging` rather than extracted from clerestory later, because extracting a published, reflected, BRP-observable surface is strictly worse.

~~The accepted cost: **`hana_rigging` must publish to crates.io before `bevy_clerestory 0.2.0` can ship** … The chain is: write the kernel → publish → clerestory `0.2.0` → hana Phase 1.~~

**What is true now (R1, R3, R4a, R4b, D5):**

- **The recovery work is merged.** `feature/reconnect` is in `bevy_hana/main` at `1021c737`; `bevy_clerestory 0.2.0` is released and the workspace is on `0.3.0-dev`. That is gate **G1**, met. So clerestory's recovery machinery *does* exist, which is precisely why it is **consumer #1** (R3): refactoring a working implementation onto the kernel is the test that either decomposes `Retain` × `Act` or kills it.
- **There is now an on-disk format to migrate**, contradicting the struck paragraph: persisted window state goes v3 → v4 by conservative conversion with self-heal (D5, gate **G8**).
- **Publishing gates delivery, not development** (R4a). hana consumes `bevy_clerestory` by monorepo git rev instead of crates.io `0.1.1` (R4b, gate **G3**), so no migration waits on a release.
- **`0.3.0`, not `0.2.0`**, is the clerestory version that ships this work.

**One authoritative build order, replacing every other order in this document:**

1. `hana_rigging` kernel, no providers, unit-tested against hand-built scans.
2. `bevy_clerestory` refactored onto it as consumer #1, by path dep, no publish. Includes the v3 → v4 state migration.
3. Catalyst screens provider.
4. Catalyst cameras provider **and** the saved-key migration (gate **G9**).
5. The two-provider integration test — a display and a webcam unplugged in one run.
6. Stream Deck provider — the only N:1 consumer and the only genuinely new code.
7. Publish `hana_rigging`, then `bevy_clerestory 0.3.0`, then hana's dependency bump, then [[restore primary window after reconnect]] Phase 1.

The rule underneath the order is unchanged: **do not publish until two real consumers have used it.** The publish freezes the surface; the migrations that can still falsify it land first.

Two prerequisites inside clerestory, already accepted work, needed regardless of which crate the result lands in: split the presence-event payload into identity vs descriptor, and demote the winit enumeration index to an adapter format (a registry that sorts or filters silently restores windows to the *wrong display*).

## v1 scope

1. `hana_rigging` kernel in the library monorepo: identity, presence diff, availability policy, recovery state machine, provider contract. No I/O.
2. Stream Deck over **direct USB HID** as hana's transport controller. Not via BitFocus Companion — Companion would own the device and prove nothing about hardware abstraction.
3. Migrate the three existing device kinds onto it: screens, cameras, and clerestory monitor recovery. **If any of the three gets worse, the API is wrong** — that is the acceptance test, not a nice-to-have.
4. **The Stream Deck is the fourth acceptance target, not just a feature.** Corrected by D3: all three migration targets above are **one-slot devices**, so none of them exercises N:1 endpoints, slot migration, slot collision, cohesion, or reassignment — the machinery [[physical audio rendering architecture]] forced into the design and the source of findings T4, T6, R4, and R5. The test as originally stated passes with every one of those unfixed. The Stream Deck qualifies as an N:1 target — keys, encoders, LED rings, and an NFC reader on one device — and is already in the slice, so the only cost is naming it as an acceptance target.

### Constraints the HID survey fixes on the slice

- **One model only.** `Original 0x0060` and the Mini family are separate protocols, one of them undocumented by Elgato. Scope v1 to MK.2 or XL and let the abstraction prove itself on the *second* model rather than the first.
- **No tokio.** `elgato-streamdeck`'s `async` feature pulls tokio and hana has none in the workspace (verified across every `Cargo.toml`). Use the blocking API on `IoTaskPool`. New dependencies: `elgato-streamdeck` — which brings `hidapi` and `image` — plus `crossbeam-channel`.
- **The owned device thread is not optional.** `HidDevice` is `!Sync`, no OS event stream exists because the usage page is vendor-defined, and Elgato's recommended input poll is 50 ms. One thread owns the handle, channels carry commands in and reports out, a `PreUpdate` system drains, `AppExit` releases.
- **Dirty-tracking and a rate limit from day one.** A full XL refresh is ~96–288 blocking USB round-trips, each with a JPEG encode, and the encode rather than the bus is usually the bottleneck. Writes also need spacing: node's shipped stack serializes through a concurrency-1 queue with a 1 ms gap, commented *"some streamdecks get upset with too many reports in quick succession."* Issued from a Bevy system this blocks the frame; issued from the IO pool it competes with asset streaming.

**Estimate: 3–5 days** for a working transport-controller slice, **plus 2–4 days** to make it the general `hana_rigging` shape rather than a one-off. The device thread is about a day (`bevy_streamdeck` is a 293-line template), the image path a day, transport wiring a day — and **enumeration, claim, reconnect, and error classification is 1–2 days with nothing to copy.** That last line is the argument for the crate in one sentence: it is the only genuinely new work, and it is the part every device kind needs.

## Open decisions

1. ~~**Authored identity.**~~ **Resolved: a distinct `DeviceIdentity` variant, not a flavour of `Unverified`.** A hand-patched DMX address is the *opposite* of "can't tell" — it is more trustworthy than a discovered display, because it cannot lie. Filing it under `Unverified` would disarm every fixture and every hand-placed speaker in the system. It is genuinely an orthogonal axis (*who vouches for this* versus *can continuity be trusted*), but a product of two enums is worse than a sum here, because `Authored × can't-tell` and `Authored × mutated` are meaningless — an authored id cannot drift out from under you. So `Proven` (named `Discovered` when this was written) and `Authored` are separate variants, both armable, with different failure semantics: a `Proven` unit that vanishes may return with the same id and reacquisition is correct, while an `Authored` unit that vanishes means either the transport is gone or **the patch is wrong**, and the kernel must surface that rather than silently re-bind. The structural consequence monitors never forced: for authored devices presence is transitive through the transport — `presence(interface) ∧ patched(address)` — which is what `DeviceRecord.transport` exists for, and which then also carries child devices for free.
2. ~~**Durable identity token.**~~ **Resolved: `DeviceKey { kind: DeviceKind, id: DeviceIdSource }` — a kind plus a nested `Reported`/`Synthesized` source (R6, D6). The original resolution below said "three plain strings"; that shape is superseded, the reasoning about schemes and migration still holds.** Process-local identity was a winit limitation, not a principle, and the kernel persists `DeviceKey` while `DeviceId` stays runtime-only. `scheme` is what makes it migratable: when a provider changes how it derives identity, a plain `V1 -> V2` function rewrites `scheme` and `value` and touches nothing else — [[versioned save format]]'s own mechanism, which it describes as the `windows.ron` `version: 2` pattern grown up. `value` is opaque to the kernel and meaningful only to the issuing provider, so no Rust type path reaches the file and adding a fourth field later never breaks an old one. This fills the one device hook [[versioned save format]] leaves open: it lists "hardware designations" among document contents and never expands it.
3. ~~**Which representation do Stream Deck buttons feed?**~~ **Resolved: both, and the device layer never decides.** The vault said a keymap surface ([[hardware streamdeck integration]]) *and* a tool-graph jack ([[chromatik competitive research]], "same shape as cameras today"), and read as a fork that is unanswerable — direct HID settles who opens the device and nothing more. Under the capability model it stops being a fork: buttons are `Controls` slots, and a keymap and a jack are two *consumers* querying the same component. Neither is privileged, both may exist at once, and adding the second changes no device code. v1 wires only the action-system path through `bind_action_system!` because that already exists and keeps the hana-side diff small; the jack becomes a second observer whenever the tool graph lands. **A design where this question still needed an answer would be the wrong design** — it would mean the device layer had opinions about routing.

A fourth decision, opened by facts 5–8 and resolved in *Claim — the axis presence cannot carry*: **is "another process holds it" a presence value?** No. `Claim` is a separate provider-asserted axis, and the load-bearing rule is that `Act::OnReturn` must not fire while `Claim::Contended`.

## Live bugs found along the way

Independent of any release, fixable now:

- `hana/src/conduit/jack.rs:227` — `resolve_monitor` matches by logical position and, per its own doc comment, "falls back to the first monitor when no position matches". Exactly the prohibition.
- `hana/src/conduit/jack.rs:73` and `conduit/window.rs:79` — retain winit `Monitor` entities across disconnect.
- `conduit` has **no observer for a monitor vanishing while a window is fullscreen on it**.
- The primary camera is opened once at `OnEnter(Ready)` and **never retried** on failure — while the same physical device *is* retryable as a secondary, and screens retry on a 3 s timer.
- Off-macOS, `nokhwa`'s `frame_raw()` blocks untimed so the stop flag is unobservable — a latent zombie capture thread. The macOS backend bounds it and has two tests for exactly this.
- `VIDEO_UNAVAILABLE_LIKELY_CAUSE = "another app is using the webcam"` is a **guessed cause shipped as UI**, because the device layer returns only an opaque reason string.
- Panel-to-window matching uses **exact** position equality while feed matching uses a 200-pt tolerance.

## Core types

Every type below is non-generic, so `reflect_auto_register` covers it and BRP `world.observe+watch` works without manual registration. That constraint is load-bearing, not incidental — it is why the provider contract is a trait object rather than a type parameter.

### Identity — two tokens

```rust
/// Runtime handle. Opaque, process-local, `Copy`, cheap to compare. Never persisted.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
pub struct DeviceId(u64);

/// Durable designation. Stable, human-readable, migratable, no Rust type paths.
///
/// The nested `id` is the whole point (R6): the variant, not a string, says
/// whether the identifying value is proof or a hint, so a caller cannot lose
/// that distinction by copying a field.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Reflect)]
pub struct DeviceKey {
    /// Sort of unit: display, audio interface, DMX universe, HID panel.
    pub kind: DeviceKind,
    /// Where the identifying value came from, and therefore what it licenses.
    pub id:   DeviceIdSource,
}

/// How this unit came to be identified. **This is the trust level.**
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Reflect)]
pub enum DeviceIdSource {
    /// The unit reported this value itself — a USB serial the hardware exposes,
    /// a monitor serial in its EDID. Proof of which physical unit this is, and
    /// the only source that can ever arm output (T1, D3).
    Reported { scheme: SchemeName, value: ReportedId },
    /// Derived by hana from descriptors, because the unit exposes nothing unique
    /// to itself — the port-derived camera id, the serial-less display UUID (D1).
    /// A hint: it may restore a saved configuration, it never drives output.
    Synthesized { digest: Digest },
}
```

Three validated newtypes rather than bare strings (D6), each with a private field and a checked constructor:

- `SchemeName` — names an identity space. Registered at app-build time, so a typo fails at startup instead of silently minting a second device. Two providers naming one scheme assert they mean the same space; that is the migration hinge.
- `ReportedId` — the value the unit reported. Opaque to the kernel, meaningful only inside its scheme.
- `Digest(u64)` — the synthesized value. A `u64`, not a `String`: it is an FNV-1a hash, which is 64 bits, and a fixed-width integer makes a malformed digest unrepresentable while dropping two heap allocations.

Every identifier the platform can supply — reported serial, OS unique id, attachment path, vendor/product/model, synthesized digest — is **retained on the device record** even though only the strongest becomes the key (D1). Absence is always a named variant, never `Option`: *this unit exposes no serial* and *this platform cannot be asked* lead to different policy, so they are different variants (`ReportedSerial::{Provided, NotExposedByUnit, PlatformCannotReport}`).

Two rules the screens migration forced, neither of which monitors alone would surface:

- **Devices may be co-reported.** Records carrying an equal `DeviceKey` from different providers merge into one device. One physical display enumerated by both winit and `xcap` is one device, not two — which is what actually retires the 200 pt geometry tolerance instead of relocating it.
- **Duplicate keys within one scan disarm themselves.** Every member of the duplicate set becomes `Unverified(NotUniqueInScan)`. This enforces rule I9 rather than trusting it, and it is what stops a weak scheme — two identical webcams under one device name — from presenting as `Proven`.

```ron
DeviceKey(kind: Display,        id: Reported(scheme: "cgdisplay",     value: "69733382"))
DeviceKey(kind: AudioInterface, id: Reported(scheme: "coreaudio-uid", value: "Scarlett18i20:D4E5"))
DeviceKey(kind: DmxUniverse,    id: Reported(scheme: "patch",         value: "artnet/10.0.0.7/u1"))
DeviceKey(kind: HidPanel,       id: Reported(scheme: "usb-serial",    value: "CL15K1A00080"))
DeviceKey(kind: HidPanel,       id: Reported(scheme: "net-dock-node", value: "dock:AB12/child/2"))
DeviceKey(kind: Camera,         id: Synthesized(digest: 14695981039346656037))
```

The last line is the ordinary case for a USB webcam: its "unique id" is a port location, so it is not proof, and the key says so in its shape.

```rust
/// The **conclusion** reconciliation reached about which unit this is (D3).
///
/// This is computed after matching, from the retained evidence — it records the
/// verdict, not the raw evidence, so it can never contradict the key. It carries
/// no `PartialEq`, which is how R8's never-self-match rule is enforced: there is
/// no way to write `Unverified == Unverified`.
pub enum DeviceIdentity {
    /// A `Reported` key matched a live unit uniquely. The only verdict that can
    /// mint an arm authorization.
    Proven(DeviceId),
    /// A `Synthesized` key matched uniquely. Enough to put a saved configuration
    /// back — a window returning to its monitor — and never enough to drive
    /// output. This is the everyday verdict for webcams and serial-less panels.
    RestoreOnly(DeviceId),
    /// A human assigned this address. Authoritative by construction; the unit
    /// reports nothing and the patch *is* the identity.
    Authored(DeviceId),
    /// A saved key matched nothing, but a unit of the right kind sits in the same
    /// slot on the same transport. Neither absent nor confirmed. Needs a human.
    Displaced { saved: DeviceKey, candidate: DeviceId },
    /// A human authored this binding and the unit itself reports something else.
    /// The saved key matched; the unit sitting in it is the wrong one. Never
    /// armable — driving a misidentified fixture is how pan data reaches a dimmer.
    /// (See *Adversarial refutation* T3.)
    WrongUnit { authored: DeviceKey, reported: DeviceId },
    /// Not identified: absent, not unique among live units, or the platform
    /// could not be asked. Carries which of those it was, because they lead to
    /// different policy (D1's no-`Option` rule). Prohibits automatic action.
    Unverified(UnverifiedReason),
}

impl DeviceIdentity {
    /// Identity only — *do we know which unit this is*. **Not** the policy predicate:
    /// arming also requires presence, claim, and `Arming`, folded in `Devices::armable`
    /// (see *Adversarial refutation* T1 and *Reconciliation* §1).
    pub fn identified(&self) -> Option<DeviceId> {
        match self {
            Self::Proven(id) | Self::RestoreOnly(id) | Self::Authored(id) => Some(*id),
            Self::Displaced { .. } | Self::WrongUnit { .. } | Self::Unverified(_) => None,
        }
    }
}
```

Identity lookup is **exact-or-`None`**. Geometry lookup — `closest_to`, `first()`, nearest-within-tolerance — always returns something and is correct at the provider layer. Both currently hang off clerestory's one `Monitors` type, which is *how* the never-fall-back rule became a live violation in four places. In the kernel they cannot merge: the kernel holds no geometry to fall back to.

### Presence and scans

```rust
pub enum Presence {
    /// Provider asserts the unit is there and usable.
    Present,
    /// Provider asserts the unit is gone.
    Absent,
    /// Provider cannot tell — remote node silent, transport down.
    /// Distinct from `Absent`: collapsing them either resurrects retired
    /// outputs or destroys live ones.
    Unreachable { since: Duration },
}

pub struct DeviceRecord {
    pub key:       DeviceKey,
    pub identity:  DeviceIdentity,
    /// Parent link. Authored devices hang off their interface; child devices off
    /// their host. `None` = root. Says "this hangs off that" and never names a bus.
    pub transport: Option<DeviceKey>,
    pub presence:  Presence,
}

/// What a provider has to say about the devices it can see (D7).
pub enum DeviceScan {
    /// Nothing was scanned since the last report, so there is nothing to say.
    /// The kernel keeps the set it already holds.
    ///
    /// Normal for every provider on most frames — cameras enumerate every two
    /// seconds, HID every ten, and clerestory only when the display
    /// configuration changes. Returning this builds nothing.
    Unchanged,
    /// A scan completed.
    Complete(DeviceSet),
}

pub struct DeviceSet {
    pub provider: ProviderId,
    /// Whole current set, not deltas. Absent devices are omitted.
    /// Parent links make this a forest; the kernel ingests roots first.
    pub devices:  Vec<DeviceRecord>,
    /// Monotonic per provider; the kernel folds these into a global revision.
    pub revision: ProviderRevision,
}
// One name for one thing: earlier sections called this `TopologyRevision`, which is
// monitor vocabulary for a per-provider counter. D7 put the revision on the
// provider's own scan, so `ProviderRevision` is the name everywhere.

```

Scans are whole-set because that is what makes a tree tractable — the kernel diffs two forests rather than replaying edges, so a provider that loses track self-corrects on the next scan. It is also what makes clerestory's current ordering bug structurally impossible to repeat: the kernel emits nothing until the scan is committed.

Two rules about the forest, and no more: presence is **conjunctive down the chain** (a child whose parent is `Unreachable` is `Unreachable`, not `Absent`), and a parent's departure retires descendants through the same keyed path, so nothing depends on a live entity existing.

### Endpoints — N bindings : 1 device

[[physical audio rendering architecture]] forces this: speaker entities each map to one hardware output channel, so one registration per device is wrong.

```rust
/// What a registration actually binds to.
pub struct EndpointRef { pub device: DeviceKey, pub slot: Slot }

/// Provider-defined, opaque to the kernel.
/// "0" | "ch/7" | "u1/c37" | "key/3" | "encoder/1" | "nfc"
pub struct Slot(String);
```

| Kind | Device | Slots |
|---|---|---|
| Monitor | one display | one — the degenerate case |
| Audio | one interface | N output channels |
| DMX | one interface/universe | N fixtures at channel offsets |
| Stream Deck | one panel | N keys, N encoders, N LED rings, one NFC reader |
| Touch | one digitiser | N contacts |

This is also what dissolves "input device that is also a render target": one device, many slots, bound twice by two kind layers. Feedback channels and the NFC-on-the-button-stream case are slot vocabulary, which is provider-owned.

```rust
/// Checked by the kernel, never understood by it.
pub enum Cohesion { None, SameDevice, SameHost }
```

`SameDevice`/`SameHost` covers the one-interface-one-machine constraint. The kernel refuses to arm a group that violates it and learns nothing about sample clocks. The same predicate covers a laser array on one DAC.

### Availability — two axes

```rust
pub struct Availability { pub retain: Retain, pub act: Act }

pub enum Retain {
    /// No registry entry; the binding is forgotten on departure.
    Nothing,
    /// Keep the last intentional configuration across absence.
    LastIntentional,
    /// Apply a state declared up front, not one we observed.
    /// Blackout, home position, shutter closed, laser safe state.
    Declared(Box<dyn Reflect>),
}

pub enum Act {
    /// Report only. Never touch the device.
    Never,
    /// Re-apply only when explicitly asked.
    OnRequest,
    /// Re-apply automatically when the *verified* unit returns.
    OnReturn,
}
```

This refines `Ignore | Notify | Reacquire` rather than replacing the intent behind it. The tri-state conflates *retain* with *act*, and it cannot name the application-controlled cell — remember the target, re-apply only on request — so it smuggles it in. Two axes name all four legal cells; `Nothing` plus any acting `Act` is rejected at construction, because you cannot re-apply what you did not keep.

| retain | act | | old name |
|---|---|---|---|
| `Nothing` | `Never` | not registered | `Ignore` / `Disabled` |
| `LastIntentional` | `Never` | remember, report, never touch | `Notify` / `ApplicationControlled` |
| `LastIntentional` | `OnRequest` | remember; re-apply on demand | unnameable in the tri-state |
| `LastIntentional` | `OnReturn` | re-apply when the verified unit returns | `Reacquire` |

**`Declared` is the third answer, and the model was incomplete without it.** The two axes answer *do we keep anything* and *when do we act*, but until now they left a third question unasked: **what state do we act toward?** Every cell above silently assumed one answer — the last state we ourselves set. That is right for a monitor and wrong for most of the reference set. A light rig wants blackout on return, a moving head wants its home position, a projector wants its shutter closed, a laser wants its safe state. None of those is "what we last set", and expressing them by first driving the device to the desired state so it becomes the remembered one is exactly the automatic action T1 forbids.

`Declared` costs no new axis: `Act` still decides *when*, `Retain` now decides *toward what*, and `{Declared, OnReturn}` reads as "when this comes back, put it in this state" without any new machinery. `{Declared, Never}` is legal and useful — it names a safe state for something the kernel will never drive on its own.

**The same gap exists on the departure edge**, which refutation R6 found independently: there is no departure action at all, so nothing can say "leave it like this on the way out". `Declared` supplies the missing *target*; R6 supplies the missing *trigger*. They are one feature seen from two ends.

**One thing that must not be inverted.** Driving a device to a safe state on departure is itself an automatic action, and T1's whole point is that the kernel must not issue those. So the departure application is **provider-executed** — the kernel supplies the declared blob and the fact of departure, and the provider decides whether energizing anything to reach that state is safe. A kernel that "fails safe by applying the safe blob" has reintroduced the laser bug wearing a reassuring name.

`FallbackAndReturn` decomposes cleanly and stays out of the core as already decided: `{LastIntentional, OnReturn}` plus a clerestory-private `FallbackBehavior`. A DMX fixture picks the same cell and simply has no `FallbackBehavior` — on reconnect its frozen levels are re-applied, with no relocation step because there was never anywhere to relocate to.

Default is `{Nothing, Never}`. The generic layer never silently arms anything. Policy is fixed at registration — a change is cancel plus re-register — and is copied where entity loss cannot erase it.

### Recovery

```rust
pub enum RecoveryPhase {
    /// Registered, unit present, nothing owed.
    Nominal,
    /// The unit is gone and its configuration is retained. Nothing can be
    /// attempted until it returns. `Act::Never` rests here indefinitely.
    AwaitingDevice,
    /// The unit is back and verified, and an apply is in flight to take it back.
    Recovering(ApplyId),
    /// That apply passed its deadline. Not an error and not a failure — the
    /// device may still be converging — so it is retried rather than reported.
    PastDeadline(ApplyId),
    /// Retired by key. Later presence must not revive it.
    Retired,
}

/// The **role**, not the hardware: an app-assigned handle for "the thing this
/// binding is for" that outlives every unit that ever fills it. `WindowKey` and
/// `SecondaryCameraId(u32)` are both this, spelled twice (see *Monitor round-trip*
/// and *Migration proof*). Distinct from `DeviceKey`, which names a unit — a rig
/// survives the unit being unplugged and replaced, which is the whole point.
pub struct RigKey(String);

pub struct Apply {
    pub id:       ApplyId,
    pub rig:      RigKey,
    pub endpoint: EndpointRef,
    /// The identity this attempt was authorised against.
    /// Re-checked on every poll; a mismatch invalidates it.
    pub expected: DeviceId,
    /// Scan revision at authorisation. A newer revision invalidates.
    pub revision: ProviderRevision,
    /// Bounds the attempt end to end, not per step.
    pub deadline: Instant,
}

/// The configuration a provider last successfully applied to one device.
///
/// The kernel stores this without interpreting it — knowing what a monitor or
/// camera configuration *is* would violate the membership test. Each variant
/// holds the provider's own `Parameters` erased to a reflected value, which the
/// provider recovers by downcast and BRP can display for debugging.
pub enum Captured {
    /// Safe to overwrite from live readback.
    Writable(Box<dyn Reflect>),
    /// Absent or mid-attempt: live readback would poison the intent.
    Frozen(Box<dyn Reflect>),
}
```

**`ProviderBlob(Vec<u8>)` is retired (D4).** Bytes are only needed for a value that leaves the process, and this one does not: `Captured` is the kernel's live memory of the last successful apply, within one run. What persists is durable keys plus policy (D2) and the application's own authored intent. `Box<dyn Reflect>` rather than `Box<dyn Any>` costs nothing extra and buys BRP inspectability — reflection blindness already cost a phantom regression during the asset-loading work.

Suppression is **per key**. A global pause — one unit restoring freezing persistence for all of them — is a live bug in clerestory today and must not be carried across.

## Provider contract

One associated type — the provider's own configuration vocabulary — and five methods, one of which is defaulted (D4, N2, N3).

```rust
pub trait DeviceProvider: Send + Sync + 'static {
    /// The configuration this provider accepts and reads back — its own type, in
    /// its own vocabulary (N2). `Reflect` is required so the kernel can hold it
    /// erased in `Captured` and BRP can show it; an ordinary derive satisfies it.
    type Parameters: Reflect;

    /// Stable across runs. Namespaces `DeviceKey.kind`; appears in diagnostics.
    fn id(&self) -> ProviderId;

    /// Called once per `RiggingSet::Reconcile`. Whole-set, not deltas.
    /// The provider may take `NonSendMarker`, touch thread-locals, or do I/O here.
    /// The kernel does none of those.
    fn scan(&mut self, world: &mut World) -> DeviceScan;

    /// Read the live endpoint's configuration for capture.
    /// Called only while `Captured::Writable`.
    fn capture(&mut self, world: &mut World, endpoint: &EndpointRef)
        -> Option<Self::Parameters>;

    /// **Start** driving the endpoint to the state `params` describes, and return
    /// immediately — every real open in this codebase is backgrounded with a
    /// channel outcome, and a blocking apply would mandate the main-thread stall
    /// `open_camera` is criticised for.
    /// **Absolute, not differential** — the provider must not assume a known
    /// starting state.
    fn apply(&mut self, world: &mut World, endpoint: &EndpointRef,
             params: &Self::Parameters, apply: ApplyId, auth: ArmAuthorization);

    /// Polled each reconcile until it resolves. The kernel's end-to-end deadline
    /// bounds the whole sequence, so a provider that must reset then render
    /// reports `Pending` between the two and cannot claim it is done early.
    /// The provider supplies what counts as arrived — a delivered frame, a stable
    /// position, a completed image write. The kernel only times it.
    fn poll(&mut self, world: &mut World, apply: ApplyId) -> ApplyProgress;

    /// Did the request actually take effect? **Defaulted** (N3): observed
    /// byte-equal to requested is `AsRequested`, anything else is
    /// `StillConverging`. A camera overrides it so a 60 fps request satisfied at
    /// 59.94 counts as arrived; a device that lands somewhere else entirely
    /// returns `DeviceSubstituted`.
    fn fulfillment(&self, requested: &Self::Parameters,
                   observed: &Self::Parameters) -> Fulfillment { .. }
}

pub enum ApplyProgress { Pending, Done, Failed(DeviceFault), Aborted, Substituted }
```

~~`fn migrate(&self, from: u32, blob: ProviderBlob) -> Result<ProviderBlob, MigrateError>`~~ **is struck.** It existed to move a *persisted* provider blob forward across versions. Nothing persists a provider blob any more (D4 retired the bytes; D2 puts durable keys and policy in the kernel and authored intent in the application's own components), so the method had nothing left to migrate. The two real migrations are versioned and owned where the data actually lives: clerestory's persisted window state v3 → v4 (D5, gate G8) and catalyst's saved `HardwareKey` values (gate G9).

Registration is by trait object, at plugin build:

```rust
app.add_plugins(RiggingPlugin)
   .add_device_provider(MonitorProvider::default());
```

`add_device_provider` is where the associated type is **erased** (D4). An associated type cannot appear in a signature reached through `dyn`, so the typed provider above cannot itself be a `Box<dyn DeviceProvider>`; registration wraps it in an adapter — written once inside `hana_rigging` — that implements a separate object-safe trait. The provider author never writes the erased form, and the kernel never names a type it cannot know. `dyn` is required not for dispatch cost, which is irrelevant at a few calls per reconcile, but because providers live in other crates: a static enum registry would make `hana_rigging` name `bevy_clerestory` and invert the dependency.

The kernel holds `Vec<Box<dyn DeviceProvider>>` and calls `scan` on each inside `RiggingSet::Reconcile`. It never names a provider type, never depends on a kind crate, and cannot be made generic over one — which is exactly what keeps `DeviceIdentity`, `Availability`, `RecoveryPhase`, and every event non-generic and auto-registered. A kind crate depends on `hana_rigging`; `hana_rigging` depends on nothing but `bevy`.

## Monitor round-trip

If displays cannot be expressed on the core without loss, the core is wrong. They can.

| Clerestory concept | Kernel expression | Loss |
|---|---|---|
| `MonitorId(u64)` | `DeviceId(u64)`, issued by the monitor provider | none |
| — (no durable form exists) | `DeviceKey("display","cgdisplay",…)` | **gain** — survives restart |
| `MonitorIdentity::Verified` | `DeviceIdentity::Proven` | none |
| `MonitorIdentity::Unverified` | `DeviceIdentity::Unverified` | none |
| `MonitorInfo` geometry | provider descriptor plus blob | none — never was kernel data |
| `MonitorInfo.index` | not in the kernel; adapter format only | intended |
| `MonitorConnected` / `Disconnected` | provider events from scan deltas | none |
| `Monitors` resource | provider-owned; the kernel holds no geometry | intended |
| `WindowKey` | `RigKey` | none |
| `WindowRecovery::Disabled` | `{Nothing, Never}` | none |
| `WindowRecovery::ApplicationControlled` | `{LastIntentional, Never}` | none |
| `WindowRecovery::FallbackAndReturn` | `{LastIntentional, OnReturn}` + `FallbackBehavior` | none |
| `CapturedWindowState` | `Captured(Box<dyn Reflect>)` | none |
| `CapturedPersistence::{Writable,Frozen}` | `Captured::{Writable,Frozen}` | none |
| `RestoreApply` | `Apply` | none |
| `MonitorTopologyRevision` | `ProviderRevision` | none |
| `WindowRecoveryPending` / `Available` | `RigAwaiting` / `RigAvailable` | none |
| `RestoreWindow` (EntityEvent) | `ReapplyConfiguration` (EntityEvent) | none |
| `CancelWindowRecovery { window }` | `RetireRig { rig }` | none |
| `WindowRestored` / `WindowRestoreMismatch` | provider-emitted; flat field lists preserved | none |
| fallback-settling, on-fallback, missing-live phases | clerestory-private sub-machine on `RigKey` | none |

One display is one device with one slot. Clerestory needs none of the N:1 machinery and pays for none of it.

**Deliberately not expressible:** `monitors.first()`, `closest_to`, `MonitorSelection::Index`, `FallbackToPrimary`, and unknown-entity-becomes-`Primary`. Those are the never-fall-back violations. They stay in clerestory as geometry helpers, where always-return-something is correct, and they never touch identity.

## Ownership boundary

**The kernel owns** — the durable designation and the runtime identity of each unit, and the refusal to accept anything less than an exact match; presence over provider-supplied scans, including unreachability and conjunctive parent presence, and the topology revision it advances; the endpoint binding between a registration and a slot; cohesion constraints, checked and never explained; the last intentional configuration held opaquely, and whether it is currently writable or frozen; the availability policy attached at registration and copied where entity loss cannot erase it; the recovery lifecycle and its closed transitions; attempt identity, expected-identity and revision validation, and the deadline bounding an attempt end to end; keyed retirement that works with no live entity, ordered so explicit retirement beats simultaneous device loss.

**A provider owns** — discovering its units, or accepting authored ones; vouching for identity or declining to, minting `DeviceKey` values and naming its own `scheme`; its slot vocabulary; its concrete presence events and their payload type; the descriptor — geometry, channel count, capability, addressing, protocol family; its persistence format and schema version, inside the opaque blob; claiming the device, reconciling it into a known state, applying a configuration, and reporting the settle result; any degraded-but-live behaviour its substrate supports.

**The application owns** — selecting a policy per registration; deciding whether a semantic instance should exist at all; creating replacement entities and everything they carry; keeping route, patch, cue, session, or tool state alive while a unit is absent; its own UI, including adjudicating `Displaced`; repairing entity-specific mappings after replacement; explicitly requesting re-application.

**Nobody in the kernel owns** what the device is showing or doing.

Tested against the two hard cases. **Stream Deck:** the images are content and live in the kind crate; the kernel holds "`key/3` is bound to endpoint X, X is present, its last intentional configuration is this blob" and never learns the blob contains a JPEG. **Speaker:** measured calibration is a property of the unit *and the room*, so under a strict reading it is content and belongs to the audio crate — but it must survive absence, re-binding, and replacement, and if every kind crate re-implements durable-blob-that-survives-rebinding the exclusion list is too tight to use. The kernel stores the blob opaquely and never interprets it. If a future case needs the kernel to read a blob field, that is the signal the boundary is wrong, not a reason to add one field.

## The membership test

The membership sentence above is the principle; this is the version a contributor can fail.

> **If answering the question requires knowing what kind of device it is, it does not belong in `hana_rigging`.**

| Question | Needs kind knowledge | Home |
|---|---|---|
| Is it present? | no | kernel |
| Is this the same unit as before? | no | kernel |
| May I overwrite the saved value while it is absent? | no | kernel |
| Must these endpoints share one device or one host? | no — the *reason* does, the *check* does not | kernel |
| Did the attempt resolve inside its deadline? | no | kernel |
| Does this device need a reset report before rendering? | yes | kind crate |
| Is a broken HID handle a disconnect? | yes | kind crate |
| Which channel is speaker 4 on? | yes | audio |
| What image is on `key/3`? | yes | Stream Deck |
| Where on the display does the window go? | yes | clerestory |
| Is this DMX address already patched? | yes | lighting |

It has teeth because it rejects things that *feel* general. "Does the device need reconciliation after claim?" sounds like a lifecycle question and is not — it cannot be answered without naming the device.

## Measured config versus the write gate

[[physical audio rendering architecture]] keeps measured properties separate from applied corrections, or calibration re-runs and hand edits fight over one value. That is **not** the same split as `Captured::{Writable, Frozen}`, and merging them produces a specific bug.

- `Writable`/`Frozen` is a **temporal gate**: may the last intentional value be overwritten *right now*? It exists because a fallback would otherwise poison the saved intent while the unit is absent. Set by the kernel, from presence and attempt state.
- Measured versus applied is a **provenance split**: who authored this number? Both values are live at once and both are writable.

Freezing during an absence should keep the calibration and discard the renderer's correction. One flag cannot say that; two independent axes can.

**The kernel owns the temporal gate; the kind crate owns provenance.** `Captured` wraps the provider's whole `Parameters` value; the measured/applied split lives inside that value and the kernel never reads it.

Two things worth recording rather than smoothing over. The audio document contradicts itself: its `PhysicalSpeaker` sketch carries flat `latency` and `gain` fields, three lines from the rule warning against exactly that shape — the rule is the later half and wins. And that document has **no device-identity model at all** — its only identity primitive is CoreAudio's `device id` reached through `CpalOutputConfig`, and `AudioInterface` is a named concept with no fields. It asks the discover/identify/persist/reassign question and answers none of it. What is written here is a proposed answer to that open question, not a ratification of an existing audio design, and nobody who owns the audio work has reviewed it.

## Resolving the four device facts

Three of the four are provider concerns. Saying so is the point — a kernel that grows a case per device family is the failure being avoided.

1. **Sleep/resume — provider concern, no new kernel state.** "Present but handle invalid" is not a kernel fact. `Presence` is defined as *what the provider asserts*, not what OS enumeration says; a provider that knows its handle is dead reports `Absent`, then `Present` on reopen, and the normal reacquire path runs. The full re-render is free, because `Act::OnReturn` re-applies the last intentional configuration — which for a panel *is* the key images. A `Resumed` variant would be a state only a provider can set and only a provider can interpret, which is a provider field wearing a kernel type. **What the kernel does owe:** an `Absent → Present` cycle shorter than the settle window must not be swallowed. Reacquisition keys on the topology revision, not elapsed time, so a same-frame pair still advances the revision and still fires. Without that a fast suspend/resume looks like no change and the panel stays blank. That is a kernel test case, not a kernel state.
2. **Mutating identity token — kernel concern; it is the fourth `DeviceIdentity` variant.** Previously-verified-now-different is neither "can't tell" nor "user authored", and a persisted `DeviceKey` is what makes it visible: the saved token matches nothing live. `Displaced { saved, candidate }` is not armable and never auto-adopts — the kernel emits the event and waits for a human. Adopting the candidate rewrites the saved key, which is what bitfocus/companion#1173 needed and did not have. The variant is deliberately narrow: same kind, same transport, same slot, and the saved key matched nothing. Widen it and the never-fall-back rule collapses into the geometry-fudge failure this crate exists to end.
3. **Child devices over network transport — already carried, no new state.** `DeviceRecord.transport` is a single parent link, so the device set is a forest and `DeviceScan` survives a tree unchanged. It is the same field authored DMX identity already required, because an authored device's presence is transitive through its interface. `transport` says "this hangs off that" and never names USB, TCP, or Art-Net — a pseudo product id is just a `value` the provider minted under a `scheme` it named. The abstraction is not transport-shaped and never was.
4. **Post-claim reset — provider concern, and the kernel contract already compels it.** `apply` is specified as absolute, not differential: drive the endpoint to the state this blob describes. A provider that assumes a known starting state has already broken that, because presence does not imply pristine — the previous owner may have been another process or a crashed run of hana. The reset report is how one provider discharges an obligation every provider already has. A kernel `Reconciling` phase would require knowing which device kinds need it, which fails the membership test. **What the kernel does owe:** the attempt deadline is end to end, so a provider that must reset then render has both steps inside one attempt with one settle result, and cannot report it is done between them. That guarantee is what makes "provider concern" safe rather than a shrug.

## Claim — the axis presence cannot carry

Facts 5–8 expose a real gap: `Presence` answers *is the unit there*, and nothing answers *may we drive it*. A Stream Deck held by the Elgato app is `Present`. A camera held by Zoom is `Present`. Neither is usable, and the two remedies differ from each other and from a replug. Folding any of this into `Presence` repeats precisely the mistake the `Absent`/`Unreachable` split already rejects.

```rust
/// Provider-asserted, exactly like `Presence`. Orthogonal to it.
pub enum Claim {
    /// This substrate has no notion of exclusive ownership.
    /// A display and a self-describing broadcast sensor are `NotApplicable`.
    /// (Art-Net is *not* — see D1: nodes merge a bounded number of sources.)
    NotApplicable,
    /// We hold it.
    Held,
    /// Claimable; we have not claimed it.
    Free,
    /// Another process holds it, or holds it such that ours would be useless.
    Contended { holder: Option<String> },
    /// The platform refuses. The remedy is a permission grant, not a replug.
    Blocked { gate: String },
}
```

Passes the membership test: *may we drive it* needs no kind knowledge, and **five of seven** reference devices need it — HID panel, camera, audio interface, laser over serial, and Art-Net DMX. Art-Net was originally listed on the other side of this line and that was wrong (D1): nodes merge a bounded number of sources — the spec's merge is two, HTP or LTP — and report merge status in `ArtPollReply`, so a third controller is a real, observable `Contended { holder }` carrying the other console's IP and short name. The two that genuinely do not need it, a display and a self-describing broadcast sensor, are *naturally* `NotApplicable` rather than awkwardly `Free`. Omitting it duplicates policy: every kind crate would otherwise re-derive "present but unusable" and each would spell it differently, which is the four-times-independently pattern this crate exists to end.

The policy consequence is concrete and is the whole reason this cannot be a provider detail:

> **`Act::OnReturn` must not fire while `Claim::Contended`.**

Without that rule, every reconcile tick re-seizes the panel from the Elgato app, silently breaking it, forever. With it, the kernel reports and waits. Seizing is a deliberate act the application requests, never a consequence of a policy default — and `Blocked` routes to a different surface entirely, because no amount of waiting fixes a permission gate.

`Contended` also gives the failure classifier its first real job. The kernel cannot parse `HidError`; the kind crate must. But the kernel *defines the target vocabulary* the classifier maps onto, which is what stops the second kind from inventing its own prose the way cameras did.

**Poll-shaped is already correct.** `DeviceScan` is whole-set per `RiggingSet::Reconcile`, driven by a timer backstop with an OS-event fast path. Fact 7 validates that choice rather than changing it: a provider with no notification simply re-enumerates on the tick, and one with OS events advances the revision early. Neither is privileged.

## The capability model

The original question — *how do we create classes of hardware that share capabilities through code* — has an answer this architecture forces, and it is not a class hierarchy.

**There are no device classes. There are capability components and queries over them.**

The kernel's membership test bans it from knowing what a device *is*. So capabilities cannot be kernel logic. But they can be kernel **vocabulary**: reflected components that providers attach and consumers query, which the kernel stores and never reads — the same trick as `Captured`, except named and queryable instead of erased.

```rust
#[derive(Component, Reflect)] pub struct Controls    { pub slots: Vec<ControlSlot> }   // buttons, encoders, faders, NFC
#[derive(Component, Reflect)] pub struct Surfaces    { pub slots: Vec<SurfaceSlot> }   // key LCDs, displays, LED walls
#[derive(Component, Reflect)] pub struct Illuminants { pub slots: Vec<LevelSlot> }     // key RGB, LED rings, DMX dimmers
#[derive(Component, Reflect)] pub struct SampleSource{ pub slots: Vec<SampleSlot> }    // camera frames, sensor readings
#[derive(Component, Reflect)] pub struct AudioSink   { pub channels: u16, pub rate: u32 }
#[derive(Component, Reflect)] pub struct VectorSink  { pub slots: Vec<VectorSlot> }    // laser XY
#[derive(Component, Reflect)] pub struct Actuator    { pub slots: Vec<AxisSlot> }      // PTZ, moving head
```

Every slot field is an already-defined `Slot`, so capabilities are a *typed reading of the slot vocabulary*, not a parallel addressing scheme.

**How capabilities reach the entity — corrected by T2.** As first written this section named no channel at all, and the consumer example below was therefore dead for every device. Capabilities ride in the scan as a `capabilities: Capabilities` field on `DeviceRecord` — a closed set, every field optional, all reflected — and the kernel inserts them atomically with entity creation. Arrival is then always complete *as of its revision*, and a sensor that arrives bare and gains `SampleSource` one revision later is an ordinary scan diff, structurally identical to a `Claim` change. See T2 for why the two obvious alternatives — withheld arrival and a `CapabilitiesChanged` event — both fail.

Sharing happens through the query, not through inheritance:

```rust
// Everything with buttons — Stream Deck, MIDI surface, DMX console, foot switch.
// None of these crates know about each other.
fn bind_surfaces(rigs: Query<(&DeviceKey, &Controls), With<Live>>) { … }
```

Composition falls out, and this is what a hierarchy could never express cleanly:

| Device | Capabilities held simultaneously |
|---|---|
| Stream Deck + | `Controls` (keys, encoders) · `Surfaces` (key LCDs, touch strip) · `Illuminants` (LED rings) |
| ~~PTZ camera~~ | struck — under T5 this is **two devices**, not one composition. Frames and motor control arrive over independent transports and the claim boundary runs between them. |
| Moving-head fixture | `Illuminants` · `Actuator` |
| Laser | `VectorSink` · `Illuminants` · plus a kind-private interlock |
| Audio interface | `AudioSink` (N) · `SampleSource` (N inputs) |
| Display | `Surfaces` (one slot — the degenerate case) |
| Self-describing sensor | `SampleSource`, **shape known only at runtime** |

That last row is decisive and settles more than it looks. A self-describing BLE sensor announces its capability set after connecting. Components can be inserted at runtime; a trait object cannot gain traits at runtime, and a class cannot change its parent. **Any design where a device's capabilities are fixed at compile time cannot represent a device the reference set already contains.**

### The reference device already self-describes

The self-describing sensor was supposed to be the exotic case that justified runtime capabilities. It is not exotic — the Stream Deck does it too, and the entire ecosystem ignores it.

Elgato's HID spec defines **`Get Unit Information`, feature report `0x08`**: keypad rows and columns, key width and height, LCD width and height, image bits-per-pixel and colour scheme, gallery capacities. Serial and three firmware versions are separately readable. **No Rust or Python library reads any of it.** Every crate hardcodes a `match Kind::…` table of key counts, encoder counts, image sizes, rotations, and per-model report offsets.

**That table is already wrong.** `elgato-streamdeck` gives the Plus XL 120×120 keys and a `(100, 1200)` strip; Elgato documents 112×112 and 1200×100 — open issue #58 since 2026-06-11, independently confirmed against node's tables. A compile-time capability table drifted from the hardware and shipped wrong. That is the failure mode the component model structurally avoids: a provider that reads `0x08` populates `Surfaces` from the device instead of from a constant, and prefers the crate's table only as fallback.

Two more rows from the same product family make the point with no exotic hardware at all:

- **Pedal (`0x0086`)** — three pedals, no display. `is_visual()` is false and image writes fail with `NoScreen`. Same vendor, same protocol family: `Controls` **without** `Surfaces`.
- **Neo (`0x009A`)** — two capacitive touch sensors appended to the *end of the button array* at indices 8 and 9, separated from real keys only by `key_count()`, and they take solid RGB but not images. `Controls` and `Illuminants` overlapping inside one address space — which is why `Slot` is provider-owned and opaque to the kernel.

A `Kind` enum needs a variant per combination and is wrong the moment a firmware revision ships. Components need none: the provider attaches what the device reports having.

### What the centralization test rejects

| Looks universal | Killed by | Home |
|---|---|---|
| Calibration / measured-vs-applied | Stream Deck has none | inside the provider's `Parameters` |
| Safety interlock ~~kind crate~~ | — | **wrong; see T1.** The interlock *hardware* is kind-private, but the **veto is not**, because the kernel fires `Act::OnReturn`. There was no composition point: `armable()` is an inherent method on a kernel enum and a kind crate cannot extend it. Resolved by the `Arming` axis. |
| Throughput / bandwidth budget | monitor and DMX have no meaningful one | kind crate |
| Firmware version gating | two of seven | provider descriptor |
| Frame rate / refresh | absent for DMX, HID, audio | kind crate |
| Pixel format, channel encoding | payload, not device | already rejected as incidental |

The rule that generated this column: **a capability is core vocabulary only if a consumer would write the same code against two unrelated kinds.** Two crates want "all devices with buttons". No crate wants "all devices with a dimmer curve *and* a lens".

## The Bevy shape

Device-as-entity, provider-as-trait-object, state mirrored in a keyed resource. The runner-up is a `dyn Device` registry and it loses on three independent counts.

```rust
// One entity per device. Every component reflected, so BRP sees all of it.
#[derive(Component, Reflect)] pub struct Device;              // marker
#[derive(Component, Reflect)] pub struct Live;             // present ∧ claim usable
// plus DeviceKey, DeviceIdentity, Presence, Claim, Arming, Availability,
// RecoveryPhase, and the capability components — which the kernel inserts from
// DeviceRecord.capabilities atomically with the entity. (As first written this
// said "whichever the provider attached"; that channel did not exist — see T2.)

/// Durable state cannot live only on the entity — keyed retirement must work
/// with no entity alive. The entity mirrors the registry for querying and BRP.
#[derive(Resource)] pub struct Devices { by_key: HashMap<DeviceKey, DeviceState>, entity: HashMap<DeviceKey, Entity> }
```

Consumers react with observers on the device entity:

```rust
app.add_observer(|arrived: On<DeviceArrived>, panels: Query<&Controls>| { … });
```

```rust
/// One apply stopped being in progress. Fires once, carrying how it ended.
///
/// Named `Finished` rather than `Resolved` because it fires on all four
/// endings, including `Aborted`, where nothing was resolved.
#[derive(EntityEvent)]
pub struct ApplyFinished {
    pub apply:   ApplyId,
    /// `Done` · `Failed` · `Aborted` · `Substituted` — never `Pending`.
    pub outcome: ApplyProgress,
}
```

`DeviceArrived`, `DeviceDeparted`, `ClaimChanged`, `ApplyFinished`, and `ReapplyConfiguration` are all `EntityEvent` targeted at the device entity — which is what lets a consumer say "*my* Stream Deck came back" without a global match arm over device kinds.

**Why not `dyn Device`:**

1. **Trait objects are not `Reflect`.** Device state would be invisible to BRP `world.query`, and this project has already paid for that exact mistake — `EnvironmentMapLight` was unregistered, BRP could not see it, and a phantom regression was chased during the asset-loading migration. Every field above is a reflected component specifically so debugging is possible.
2. **Capabilities are runtime-discovered** (the self-describing sensor). Components can be added later; traits cannot.
3. **Main-thread I/O has nowhere to live.** The monitor scan is winit `NonSendMarker` and thread-locals. A `Box<dyn Device: Send + Sync>` in a resource cannot host it, and `NonSend<Box<dyn Device>>` would drag *every* provider — including the HID one that wants its own thread — onto the main thread. `scan(&mut World)` inside an exclusive system solves it for both: the winit provider touches thread-locals directly, the HID provider drains a channel from a thread it owns.

Note the shapes are not in tension. Providers are trait objects because there are few of them, they are registered at build, and they do I/O. Devices are entities because there are many, they are queried, and they must be inspectable. `DeviceProvider` stays a trait object for the reason already stated — it is what keeps every kernel type non-generic and auto-registered.

**Testability**, which is the payoff for a kernel with no I/O:

```rust
#[test]
fn contended_device_does_not_reacquire() {
    let mut app = App::new();
    app.add_plugins(RiggingPlugin)
       .add_device_provider(ScriptedProvider::new([
           snap![("hid-panel","usb-serial","CL15") => Present, Free],
           snap![("hid-panel","usb-serial","CL15") => Present, Contended { holder: None }],
       ]));
    app.update();                                  // arrives, arms
    app.update();                                  // contended
    assert_eq!(app.world().resource::<Devices>().phase(&key), RecoveryPhase::AwaitingDevice);
    assert_eq!(app.world().resource::<Applys>().len(), 0);   // never seized
}
```

No hardware, no mocks beyond a scripted scan list, and the assertion is on the exact bug fact 5 predicts. A design that cannot be tested this way has put I/O in the kernel.

## Migration proof

The acceptance test is that screens, cameras, and monitor recovery each get *better*. Monitors are proved above. Screens and cameras are proved here, adversarially — and the attempt broke the design twice. Both breaks are repaired in place, in *Core types* and *Provider contract*, and both repairs are recorded in **What the proof changed** at the end of this section.

### Screens

| Existing concept | Kernel expression | Loss |
|---|---|---|
| `ScreenSource.id: u32` (`CGDirectDisplayID`, `screen/source.rs:21`) | `DeviceKey("display","cgdisplay",…)` | **gain** — becomes the identity, not a debug row |
| `ScreenSource` geometry, `name`, `kind`, `frequency`, `rotation` | provider descriptor | none — never was kernel data |
| `ScreenSource.is_primary` (`source.rs:25`, enumerated and never read) | provider descriptor | none |
| `StreamState::{Opening, Live(LiveStream), Disconnected}` (`screen/session.rs:91-98`) | `RecoveryPhase` + provider-held `Option<ScreenCaptureStream>` | **gain** — three variants collapse to one `Option` |
| `ScreenSession.generation: u32` (`session.rs:113`) | `Apply.id` + `Apply.revision` | none |
| `outcome.generation != session.generation \|\| !Opening` (`session.rs:295-299`) | kernel-side attempt validation | **gain** — written once, not twice |
| `ScreenConnection::{Connected, Disconnected}` (`session.rs:26-33`) | `Presence` + `Claim`, projected | **gain** — gains `Unreachable` and `Contended` |
| `ScreenFeed.connection`, written by three systems | derived projection, single writer | **gain** |
| `ScreenSessions` never removes a session (`session.rs:154-270`) | `Retain::LastIntentional` + `RecoveryPhase::AwaitingDevice` | none — same behaviour, named |
| `REOPEN_RETRY_INTERVAL = 3s` (`screen/constants.rs:51-53`) | kernel retry pacing over `AwaitingDevice` rigs | **gain** — see below |
| `FIRST_FRAME_TIMEOUT = 5s` + `FrameDelivery` (`session.rs:65-88`) | `Apply.deadline` + provider-supplied arrival evidence | none, with a caveat below |
| `starved_opens` + `STALLED_OPEN_LIMIT = 3` (`session.rs:113-118`) | repeated-failure escalation, self-clearing | none |
| `ScreenFeeds.stalled: Vec<ScreenSource>` (`session.rs:52-63`) | device-fault log | none |
| `ScreenFill.logical_position: IVec2` (`screens/connection.rs:52`) | `EndpointRef { device, slot }` | **gain** — the 200 pt match dies, but not for free; see *Two providers, one display* |
| `feed_for` / `source_for` within `SCREEN_FEED_MATCH_TOLERANCE_POINTS` (`connection.rs:216-235`, `panel.rs:454-471`) | exact `DeviceKey` lookup | **gain** |
| `sync_screen_connections` mirror + edge detect (`connection.rs:98-122`) | kernel phase transitions; observers on kernel events | **gain** — the mirror stops existing |
| `MonitorConnected/Disconnected { entity }` (`connection.rs:66-78`) | panel-space events, kept | none — correctly panel-space, not device-space |
| `ScreenSessions` non-send (`session.rs:124-135`) | provider-private, unchanged | none |
| per-frame `Vec<u8>` → `CameraFrame` → `Image` | untouched | none — never enters the kernel |

### Cameras

| Existing concept | Kernel expression | Loss |
|---|---|---|
| device name as identity (`camera.rs:48-59`, restated at `secondary.rs:7-11`, `stream/mod.rs:90-102`, `render.rs:30-34`) | `DeviceKey("camera","nokhwa-name",…)` | **real, and named below** |
| `CameraDescriptor { name, index, description }` (`camera.rs:8-15`) | provider descriptor | none |
| `CameraMetadata { name, resolution, frame_rate }` (`stream/mod.rs:80-88`) | descriptor; `name` also feeds the key | none |
| `SecondaryCameraId(u32)` (`secondary.rs:31-41`) | `RigKey` | none — same role, app-assigned stable handle |
| `SessionState::{Opening, Live(CameraStream), Disconnected}` (`secondary.rs:93-100`) | `RecoveryPhase` + provider `Option<CameraStream>` | **gain** — deleted |
| `SecondarySession.generation: u32` (`secondary.rs:109-121`) | `Apply.id` | **gain** — deleted |
| the vanish path deliberately not bumping generation (`secondary.rs:115-117`) | `Apply.revision` invalidation | none — the subtlety stops needing a comment |
| opened-name ≠ session-name reject (`secondary.rs:347-358`) | `Apply.expected: DeviceId` re-checked on every poll | none — the kernel already specified this |
| `CameraOpenError { name: Option<String>, reason: String }` (`stream/mod.rs:90-102`) | typed fault + provider detail string | **gain** — see below |
| `CameraConnection::{Connected, Unavailable{name,reason}}` (`render.rs:63-75`) | `Presence` + `Claim` + fault | **gain** — `"device is busy"` becomes `Claim::Contended` |
| `SecondaryConnection::{Connected, Disconnected}` (`secondary.rs:43-51`) | same projection as screens | **gain** — the two enums become one |
| `SECONDARY_POLL_INTERVAL = 2s` (`constants.rs:22`) | kernel retry pacing | **gain** |
| enumeration-failure-is-not-absence (`secondary.rs:204-209`) | `Presence::Unreachable`, or omit-the-scan | **gain** — a rule becomes a type |
| primary excluded by name, index-0 fallback (`secondary.rs:240-255`) | app-level role selection over `DeviceKey` | **gain** — the fallback comment is already wrong |
| `PREFERRED_CAMERA_NAME` const (`render.rs:34`) | app policy over a `DeviceKey` | none |
| `CameraStreamStats` / diagnostics (`stats.rs`) | untouched | none — payload, not device |
| `SecondaryCameraFeed` fusing id + texture + connection (`secondary.rs:59-77`) | split: kernel state, provider texture | **gain** |

### What the migration deletes

Both kinds spell the same machine independently, and the staleness predicate is **byte-for-byte identical modulo type name** — `screen/session.rs:295-299` against `secondary.rs:342-346`. That is the crate's whole argument, in two files.

> **T5 — the camera half of this proof is cited against the wrong tree, and must be re-verified.**
> Every citation above is accurate against `~/rust/hana` (main), where `secondary.rs:342-346` still
> holds the byte-for-byte staleness check. But the camera session machine has **already been rewritten**
> in `~/rust/hana_tool_graph`, the catalyst worktree this work builds on (R5), where it now lives in
> `render/sessions.rs:280-302` and `hardware/camera.rs:294-312`, `:386-409`. Two changes matter:
> `session.generation: u32` has already become `session.request`, a request identity; and the TOCTOU
> guard already compares **keys** rather than names — `stream.metadata().hardware_key != session.key`.
>
> So two of the *gains* claimed above have partly shipped there already, and the proof cannot be read
> as evidence until it is re-run against the worktree. **Phase C is complete for screens and monitors,
> not for cameras.** The screens citation `screen/session.rs:295-299` is exact in both trees and needs
> no re-verification.

Stops existing:

- `StreamState` (`screen/session.rs:91-98`) and `SessionState` (`secondary.rs:93-100`) — two enums, same three variants, same doc wording. Providers keep an `Option<Handle>` instead.
- `ScreenSession.generation` and `SecondarySession.generation`, both increments (`session.rs:211`, `secondary.rs:265`), both wire fields (`capture_stream.rs:38`, `secondary.rs:127`), and both staleness checks.
- `retry_disconnected_screens` (`session.rs:321-349`) and the timer half of `poll_secondary_cameras` (`secondary.rs:213-221`) — two hand-rolled `Local<Option<Timer>>` retry pacers.
- `sync_screen_connections` (`connection.rs:98-122`) and `ScreenFill.connection`, the mirrored-state edge detector.
- `feed_for` (`connection.rs:216-235`), `source_for` (`panel.rs:454-471`), and `SCREEN_FEED_MATCH_TOLERANCE_POINTS` (`constants.rs:30-38`) — 200 pt of geometry fudge, in both directions.
- `ScreenConnection` and `SecondaryConnection` collapse into one projection of kernel state.
- `VIDEO_UNAVAILABLE_LIKELY_CAUSE` (`video_plane/constants.rs:99-101`).

Roughly 250 lines of state-machine and matching logic across two crates, replaced by registrations against one kernel.

### What it fixes for free

1. **The ±200 pt match dies.** `CGDirectDisplayID` is already read (`source.rs:46`), already carried to the OS capture API (`capture_stream.rs:160`), and outside `hana_video` its only use in the entire app is printing a debug row (`layout.rs:195`). The identity is present, published, and unused; the consumer instead matches `logical_position` within 200 pt. Two displays with origins closer than 200 pt alias today. Conditional on *Two providers, one display* below.
2. **The never-retried primary camera.** `open_camera` runs once at `OnEnter(HanaState::Ready)` (`video_plane/mod.rs:58`), blocks the main thread, and is never rescheduled — while the same physical device retries every 2 s as a secondary. Registering it with `Act::OnReturn` makes the asymmetry unstateable.
3. **The primary's missing TOCTOU check.** The secondary path verifies the opened stream's name against the requested one (`secondary.rs:347-358`, with a test at `:554`); the primary resolves name → index before the open (`render.rs:217-223`) and never re-checks. `Apply.expected` applies the check to both because it is kernel-side.
4. **`VIDEO_UNAVAILABLE_LIKELY_CAUSE` stops being a guess.** It is emitted unconditionally for *every* `Unavailable` reason (`label.rs:225-231`) because the device layer returns prose. Under `Claim`, "another app is using the webcam" is either `Claim::Contended` — reported, not guessed — or it is not shown.
5. **The non-send run-condition trap disappears.** `retry_disconnected_screens` is exclusive solely because "Bevy evaluates run conditions on worker threads, which aborts on non-send access" (`session.rs:326-329`) — a comment the camera side cites verbatim as precedent (`secondary.rs:171-173`). Kernel state is plain `Send` data, so "any rig awaiting?" is an ordinary run condition. The providers stay non-send; the *pacing* stops being.
6. **Enumeration-failure-is-not-absence stops being a comment.** Stated once on the camera path (`secondary.rs:204-209`) and nowhere on the screens path, which relies on an early `return` with no explanation (`session.rs:171-177`). `Presence::Unreachable` makes it a type. It also fixes a live inconsistency: `screen_sources()` returns an empty `Vec` on failure (`source.rs:79-82`), so a transient enumeration hiccup silently blanks every panel's metadata rows.
7. **`ScreenRole::from` decides primary-vs-secondary by `physical_position == IVec2::ZERO`** (`layout.rs:45-53`) while `ScreenSource.is_primary` is enumerated and never read. Descriptor data reaches the consumer intact.

### What gets worse, or is awkward

**1. Camera identity is a weak key, and `DeviceKey` must not launder it.** `DeviceKey("camera","nokhwa-name","Elgato Facecam MK.2")` sits in the same type as `DeviceKey("display","cgdisplay","69733382")` and *looks* as strong. It is not: two identical webcams produce one name. Today that ambiguity is invisible because nothing keys on identity strongly enough to care.

The `scheme` field makes the weakness legible rather than hidden — `nokhwa-name` reads as weaker than `usb-serial` to anyone looking. But legible is not checked, and a design that relies on a reader noticing is not a design. **Repair:** rule I9 already says no API may declare two unverified instances the same unit. The migration turns that into something the kernel can enforce rather than trust — **duplicate `DeviceKey`s within one scan are a kernel-detected condition, and every member of the duplicate set becomes `Unverified`.** Two identical webcams therefore disarm themselves, automatically, with no provider cooperation and no new type. That is strictly better than today, where the second camera silently shadows the first.

Remaining honestly worse: a *single* name-keyed camera has no proof of which unit it is, and its key is not stable against the user renaming the device in the OS. `cgdisplay` and `usb-serial` have no such failure. **Corrected by D1:** the original text said such a camera is `Discovered` — armable. It is not. A name- or port-derived value is `Synthesized`, so the verdict is `RestoreOnly`: it may put a saved configuration back and may never drive output. The kernel cannot fix a backend that exposes no serial — it can only stop the weakness from spreading, which the duplicate rule and the `Reported`-only arming gate do.

**2. Two providers, one display — and this one changed the design.** The 200 pt tolerance exists for a specific reason: `bevy_clerestory` (winit) and `hana_video` (`xcap`) enumerate the same physical display independently and report top-left coordinates that differ by a rounding step (`constants.rs:30-38`). Replacing geometry with `DeviceKey` does **not** automatically join them — a clerestory-minted key and an xcap-minted key are different strings for one monitor, and the join problem moves rather than dies.

It is solvable, and only because clerestory's `native_monitor_id` on macOS *is* `CGDirectDisplayID` (`monitors.rs:252-264`) — the same number `xcap` reports. Both providers can mint the identical `DeviceKey`. But that requires them to agree on `scheme`, which contradicts what this document said: that `value` is "meaningful only to the provider that minted it".

**Repair, made in *Core types*:** `scheme` is a **shared registry**, not provider-private. `value` stays opaque. Two providers that name the same scheme are asserting they mean the same identity space, and the kernel **merges records carrying an equal `DeviceKey` into one device with multiple contributing providers**. One display, two providers, one `DeviceKey` — geometry never enters. Off macOS, where the two enumerations may not share a scheme, they stay separate devices and the panel keeps a geometry association *in the panel layer*, where always-return-something is correct and no identity claim is made.

This is a real addition — co-reported devices — and monitors alone would never have surfaced it.

**3. The non-send session survives `scan(&mut World)`, but the deadline nearly did not.** `scan` is exclusive by signature, so `world.get_non_send::<ScreenSessions>()` works unchanged (`session.rs:179-186`), as does `SecondarySessions` (`secondary.rs:178-180`). No problem there.

The problem is `apply`. As written it implied "drive the device to this state and report", which reads as blocking. Every real open here is backgrounded onto a thread with an `mpsc` outcome (`capture_stream.rs`, `secondary.rs:303-314`) precisely because opens take seconds and `AVFoundation` discovery stalls. A blocking `apply` would reintroduce exactly the main-thread block that `open_camera` is criticised for (`render.rs:193-212`).

**Repair, made in *Provider contract*:** `apply` **starts** an attempt and returns immediately; a new `poll` is polled each reconcile and returns `Pending`, `Settled`, or `Failed`. The kernel's end-to-end deadline then bounds a genuinely asynchronous open, which is what both existing kinds already needed and hand-rolled.

**4. The started-but-silent watchdog fails my own membership test, and the test wins.** The Essential list says the kernel owns the "acknowledged but produces no traffic" watchdog. But "has a frame arrived?" cannot be answered without knowing the device is a capture device — it fails *If answering the question requires knowing what kind of device it is*.

**Resolution, no type change:** the split is one level finer than the Essential list states. The kernel owns *did this attempt resolve inside its deadline* and *how many consecutive attempts have failed*; the **provider** supplies what counts as settled. For a screen that is the first delivered frame (`FrameDelivery`, `FIRST_FRAME_TIMEOUT`); for a window it is a stable position; for a Stream Deck it is a completed image write. `starved_opens` counts attempts, not frames, so it is kernel. `FrameDelivery` observes traffic, so it is provider. The Essential bullet should read *bounded attempt with provider-supplied arrival evidence*, not *watchdog*.

**5. Per-frame texture flow fits, with one seam to cut.** The bytes path never touches identity — `Vec<u8>` → `CameraFrame` → `TimedCameraFrame` → `Image::data`, and `camera_texture` takes a frame by value (`render.rs:266-291`). Nothing there goes near the kernel.

The seam is that today the upload system's *gate* is the state machine — `let SessionState::Live(stream) = &session.state else { continue; }` (`secondary.rs:390`), `let StreamState::Live(live) = &mut session.state` (`session.rs:368`) — and the upload system also publishes identity on first frame (`secondary.rs:406-413`) and refreshes metadata on resolution change (`secondary.rs:428-434`). So the frame pump currently writes device state. After migration the provider gates on its own `Option<Handle>` and the kernel is not consulted per frame. The awkwardness is real but one-directional: `SecondaryCameraFeed` (`secondary.rs:59-77`) fuses id, texture, and connection into one struct written by three systems, and it has to be split. That is a hana_video refactor, not a kernel concession.

**6. Screens have no per-endpoint slot, and that is fine.** One display is one device with one slot, exactly like monitors. The N:1 machinery costs screens and cameras nothing. Worth stating because it is the negative result: the audio-driven `EndpointRef` design does not tax the two kinds that do not need it.

**7. One thing genuinely gets worse, briefly.** `ScreenSession` entries are never removed (`session.rs:154-270`), so a vanished display's texture handle survives forever and a reconnect resumes into the same handle with no material rebinding. Under the kernel that behaviour is `Retain::LastIntentional`, which is correct — but the *texture handle* is provider state, not kernel state, so the provider must keep its own never-removed map keyed by `DeviceKey` and the kernel's `Retired` phase must not be read as "drop the handle". Two lifetimes that are currently one. It is a correctness hazard the migration introduces and it needs a test: **a retired rig must not invalidate a provider handle that a material still binds.**

### What the proof changed

Two edits, both already applied above, neither cosmetic:

1. **`scheme` is a shared registry and devices may be co-reported** (*Core types*). Forced by two providers enumerating one display. Without it the 200 pt tolerance does not actually die — it relocates.
2. **`apply` starts an attempt; `poll` is polled** (*Provider contract*). Forced by every existing open being asynchronous with a channel outcome. Without it the kernel would mandate a main-thread block that both current kinds already avoid.

One refinement with no type change: the started-but-silent watchdog splits into kernel-owned bounded attempt plus provider-supplied arrival evidence.

The acceptance test holds. Screens lose the geometry tolerance and gain enforced identity; cameras lose two hand-rolled state machines and gain retry, TOCTOU verification, and typed contention. Neither gets worse, on the condition that co-reported devices and the split provider-handle lifetime are both built — and both are now written down rather than assumed.

## Known gaps in this design

1. **Reassignment is undesigned.** Nothing here says what happens when an interface is swapped for a different unit and twelve speaker entities must re-bind. `EndpointRef` and `Displaced` make it expressible; they do not make it decided. Largest open hole.
2. **`Presence::Unreachable` has no designed timing.** Staleness window, heartbeat cadence, and whether it decays to `Absent` are all open. [[physical audio rendering architecture]] lists multi-machine failure and reconnection as unresolved.
3. **`Slot(String)` is stringly typed.** It buys N:1, DMX offsets, per-touch addressing, and panel feedback channels in one move, at the cost of no compile-time validation. First thing to replace if slot vocabularies need arithmetic.
4. **`Cohesion` may need a scope tier between device and host.** One audio context owns one output stream, so a single machine driving two interfaces needs an aggregate device or a second context — which `SameDevice`/`SameHost` cannot express. Better resolved once the audio crate exists than guessed now.
5. **`Displaced` adjudication has no UI or persistence path.** Who answers, when, and whether an unanswered `Displaced` survives a restart are open.
6. **A phase reference is expressible but not served.** Timecode, MMC, and Ableton Link are neither source nor sink; they land on `{Nothing, Never}`, which is degenerate but not wrong. If a phase reference needs ordering guarantees relative to device arming, nothing here provides them.

7. **Claim arbitration has no requested-seize path.** The rule says a policy default must never seize, and that seizing is something the application asks for. Nothing here specifies what that request looks like, whether it is per-attempt or sticky, or what happens when the other holder takes it back a second later. A seize/yield war between hana and the Elgato app is currently expressible.
8. **Runtime capability discovery has no timing contract.** A self-describing sensor gains its capability components some time after `Present`. A consumer that queries on `DeviceArrived` may see a device with no capabilities yet and conclude wrongly. Either arrival must be withheld until the descriptor settles, or a `CapabilitiesChanged` event is needed. Undecided, and it interacts with the attempt deadline.
9. **~~A monitor's `DeviceKey` value may not survive a reboot.~~ Corrected by T6 — this gap described clerestory before the reconnect work landed.** The concern as written was that the co-reported identity is `CGDirectDisplayID`, a window-server-assigned integer Apple does not promise to reuse across restarts. That is no longer what clerestory identifies a monitor by. Verified from source: `monitors/identity/mod.rs:25` — **EDID bytes on Windows and X11, the ColorSync display UUID on macOS**, hashed with FNV-1a. `CGDirectDisplayID` is still read, but for *capture*, not identity.
   The residual risk is narrower and different in kind. The ColorSync UUID is derived from the panel's own vendor, model **and serial number**, *"falling back to the physical port for panels that report no serial"* (`monitors/identity/native.rs:184-185`). So a panel that reports a serial has a genuinely durable identity across reboots and ports; a panel that reports none has a **port-derived** one, stable across reboots but different once it is moved. That is exactly D1's `Reported` versus `Synthesized` split arriving from an independent direction, and it is why D5's v4 conversion cannot classify a saved fingerprint from the hash alone.
   What remains genuinely unestablished: only **Wayland** is anonymous, because it withholds EDID, and no platform other than macOS has been exercised end to end.
10. **~~Only macOS has been checked at all.~~ Corrected by T6 — the non-macOS paths exist; what is missing is cross-provider verification, not the code.** As written this said clerestory hashes a monitor *name* string on Windows and uses X11/Wayland global ids on Linux. It hashes **real EDID bytes** on both Windows and X11 (`monitors/identity/native.rs:142,144` — `WindowsEdid(Vec<u8>)`, `X11Edid(Vec<u8>)`). Only **Wayland** is genuinely anonymous, because it withholds EDID, and that case lands on `Unverified` by design rather than by omission.
   What is actually unverified is the same thing macOS needed *The v1 slice* risk 1 to establish: that **both providers mint the identical key** on each platform. The remaining gap is therefore a matrix, not a hole — for Windows and X11 each: cross-provider key equivalence, duplicate-panel handling, behavior across a display-configuration change, and the Wayland `Unverified` path. No claim in this document covers any row of it yet.

## Adversarial refutation

> **Historical review record (Phase D).** Kept for the reasoning and the counter-examples. Where a type
> definition here disagrees with *Core types* or with a lettered decision (R, D, N), the later one wins;
> nothing in this section is normative.

Three devices deliberately unlike the Stream Deck — Art-Net DMX with RDM, a laser with a safety
interlock, a self-describing BLE sensor — expressed on the kernel as designed, plus direct attacks on
`Slot`, `Cohesion`, and reassignment.

**Eighteen findings: seven force a type change, six force a rule, five are documentation.** Two are
negative results that *prevent* a change. Six parts of the design were attacked and held; they are
recorded at the end, because the survivors are what make the survivors credible.

Where a finding contradicts existing text, the contradicted section and the type are named in the
finding. Nothing above this section has been rewritten.

### Forces a type change

**T1 — the kernel re-energizes a laser at the moment a human has walked into the beam path.**

Six steps, every one of them a cell this design endorses:

1. A laser registers `{LastIntentional, OnReturn}` — the natural cell, and the successor to
   `Reacquire` in the *Monitor round-trip* table.
2. The DAC drops. `Presence::Absent`, phase `AwaitingDevice`, blob retained. **The beam goes dark.**
3. A human walks into the beam path, *because* the beam went dark.
4. The DAC returns. `Presence::Present`, `Claim::Held`, `DeviceIdentity::Discovered` → `armable()`
   returns `Some(id)`.
5. The kernel fires `ReapplyConfiguration`.
6. `apply` is specified as **absolute, not differential** — *drive the endpoint to the state this
   blob describes*. The blob describes beam on.

Nothing in the kernel can stop step 5, and the provider is never consulted: `DeviceProvider` has no
method the kernel calls to ask permission, only `apply`, which is an instruction.

**Contradicts *What the centralization test rejects*,** which disposes of this in one line: *"Safety
interlock | only lasers and machines | kind crate; composes as an extra armability predicate."*
**There is no composition point.** `armable()` is an inherent method on a kernel enum; a kind crate
cannot extend it, and the kernel — not the provider — is what fires `Act::OnReturn`.

**Registration cannot fix it.** `{LastIntentional, OnRequest}` moves the trigger to a human, but a
human requesting from a console in another room does not know the interlock is open either. And the
safe default `{Nothing, Never}` makes the laser the one device whose *natural* policy cell is the
dangerous one — inverting *"the generic layer never silently arms anything"* (§Availability).

**Why `DeviceIdentity::armable()` is the wrong layer, precisely:** it answers *do we know which unit
this is*. Identity is exactly the property that **cannot change when someone opens an interlock**, so
a device with perfect identity is armable by construction under the current signature. The interlock
*hardware* is genuinely kind-private — key switch, shutter, E-stop loop, ILDA scan-fail. The **veto is
not**, because the kernel owns the trigger.

**The fix is this design's own move, applied consistently.** `Claim` was added for a structurally
identical reason: presence could not answer *may we drive it*, the kernel fires the action, so the
kernel needed the axis. Identity cannot answer *is it safe to energize it*.

```rust
/// Provider-asserted, exactly like `Presence` and `Claim`. Orthogonal to both.
pub enum Arming {
    /// This substrate has no safety gate. Displays, DMX dimmers, HID panels.
    NotApplicable,
    /// The provider asserts it is safe to drive.
    Ready,
    /// A physical or policy gate is open. Never a kernel judgement.
    Inhibited { reason: String },
}

impl Devices {
    /// The single gate for driving a device. Replaces `DeviceIdentity::armable`
    /// and absorbs the prose claim rule.
    ///
    /// Requires `Reported` identity for **every** device, not only ones judged
    /// dangerous (T1). There is no danger classification on `DeviceKind` and
    /// adding one would put device-specific policy in the kernel, which the
    /// membership test forbids. A `Synthesized` key is a hint, and a hint never
    /// drives output.
    pub fn armable(&self, key: &DeviceKey) -> Option<DeviceId> {
        let r = self.by_key.get(key)?;
        let id = r.identity.identified()?;                                   // renamed
        matches!(r.key.source, DeviceIdSource::Reported { .. }).then_some(())?;
        matches!(r.presence, Presence::Present).then_some(())?;
        matches!(r.claim,    Claim::NotApplicable | Claim::Held).then_some(())?;
        matches!(r.arming,   Arming::NotApplicable | Arming::Ready).then_some(())?;
        Some(id)
    }

    /// The separate, weaker gate for **restoring** a saved configuration.
    ///
    /// Restore accepts an unambiguous `Synthesized` key, because refusing it
    /// would abandon every saved window layout on hardware that exposes no
    /// serial — the whole population D5's v4 conversion exists to keep working.
    /// Unambiguous is enforced, not assumed: a digest appearing twice in one
    /// scan demotes both to `Unverified`, which fails `identified()` here.
    ///
    /// Restoring writes configuration the device already had. Arming energizes
    /// an output. That is why they are two predicates and not one parameter.
    pub fn restorable(&self, key: &DeviceKey) -> Option<DeviceId> {
        let r = self.by_key.get(key)?;
        let id = r.identity.identified()?;
        matches!(r.presence, Presence::Present).then_some(())?;
        matches!(r.claim,    Claim::NotApplicable | Claim::Held).then_some(())?;
        Some(id)
    }
}
```

Rename the identity method to `identified()`, which is what it actually computes. This also repairs a
defect the laser merely *exposed*: today `armable()` is documented as *"the policy predicate. Only
these two may drive automatic action"* while the equally load-bearing rule — *"`Act::OnReturn` must
not fire while `Claim::Contended`"* — lives only in prose several sections away. Two gates, one in
code and one in English, is how the second one gets forgotten.

**T2 — `DeviceArrived` carries no capabilities, for every device, always. The flagship consumer
example in *The capability model* matches nothing.**

Trace it. `scan(&mut World) -> DeviceScan`; `DeviceScan` is `provider`, `devices:
Vec<DeviceRecord>`, `revision`; `DeviceRecord` is `key`, `identity`, `transport`, `presence`.
**There is no capability field, and no other `DeviceProvider` method carries one.** *The Bevy shape* says
the kernel inserts *"whichever capability components the provider attached"* — but the entity does not
exist until the kernel commits the scan, and `scan()` ran before that. A provider can only
attach capabilities on a **later** tick, after observing that the kernel created the entity.

So capabilities are at least one reconcile tick late for **every device in the system**, and

```rust
app.add_observer(|arrived: On<DeviceArrived>, panels: Query<&Controls>| { … });
```

matches nothing, always, for a Stream Deck exactly as much as for a BLE sensor. A consumer observing
`DeviceArrived` sees `Rig`, `DeviceKey`, `DeviceIdentity`, `Presence`, `Claim`, `Availability`,
`RecoveryPhase` — and no capabilities, ever.

**This is a defect in the capability model as written, not a timing question.** The self-describing
BLE sensor was supposed to be the hard case that motivated a timing contract; it is instead the case
that made a universal missing channel visible.

**Known gap 8 is the same bug seen from the other end.** It asks whether arrival should be withheld
until a descriptor settles. Both offered answers fail:

- **Withheld arrival** requires the kernel to know whether a descriptor is complete, which requires
  kind knowledge — it fails *The membership test* outright. It also cannot terminate: a sensor whose
  GATT discovery never finishes never arrives, and *present but never arrived* has no representation.
  And it collides with the attempt deadline, exactly as gap 8 suspects.
- **`CapabilitiesChanged` alone** leaves every consumer with two code paths where the arrival path is
  permanently dead but looks live — the worst available outcome, because it is the path everyone
  writes first.

**Concrete shape: capabilities ride in the scan and are diffed like presence.**

```rust
pub struct DeviceRecord {
    pub key:          DeviceKey,
    pub identity:     DeviceIdentity,
    pub transport:    Option<DeviceKey>,
    pub presence:     Presence,
    pub claim:        Claim,
    pub arming:       Arming,                 // T1
    pub capabilities: Capabilities,           // closed set, every field Option, all reflected
}
```

The kernel inserts them atomically with entity creation, so arrival is always complete **as of its
revision**. A BLE sensor that arrives bare at revision N and gains `SampleSource` at N+1 is an
ordinary scan diff — structurally identical to a device whose `Claim` changes. **No new event, no
timing contract, no withheld arrival: gap 8 closes rather than being answered.** Closing the
capability set is consistent with this design's own position that capabilities are a curated kernel
vocabulary gated by the centralization test; the alternative, `Vec<Box<dyn Reflect>>`, buys openness
the design has already argued against wanting.

**T3 — an authored identity can drift out from under you, and RDM proves it on the second reference
device.**

*Open decision 1* rejects the identity product on one sentence: *"`Authored × can't-tell` and
`Authored × mutated` are meaningless — an authored id cannot drift out from under you."*

RDM (ANSI E1.20) refutes that directly. It rides the same physical line, discovers real 48-bit UIDs by
binary search, and `GET DEVICE_INFO` returns model id, DMX footprint, personality, and the fixture's
own start address. So the unit *does* report, and it can disagree:

| Reality at universe 1 channel 37 | Design's expression |
|---|---|
| Patch says MAC Aura; RDM confirms model and address | no variant — authored-and-corroborated is unrepresentable |
| RDM finds nothing, node up | `Authored` (see D2) |
| **RDM reports a Chauvet Rogue R2** | **nothing fits** |

Someone swapped the fixture, changed its start address at the fixture's own menu, or a
`SET DMX_START_ADDRESS` moved it. **This is the most common failure in live lighting, and detecting it
is the reason RDM exists.**

- `Unverified` is *"absent, or not unique among live units."* Wrong on both counts — the answer is
  present and unique. It is *a wrong unit*.
- `Displaced { saved, candidate }` is *"a saved key matched nothing."* The saved key **matched**. The
  slot is right; the unit in it is wrong.

**Is it `Displaced`? No — and widening `Displaced` to cover it turns a UI action into a hazard.** The
adjudication offered for `Displaced` is *adopt the candidate, rewriting the saved key*. Here the saved
key is **already correct as an address**; the rig is what changed. Adopting would move the patch to
follow the wrong fixture, silently converting an operator-visible rig error into a permanent
mis-patch. The design's instinct that `Displaced` must stay narrow is right, and this is the case that
proves it.

A fifth variant, preserving the sum for the reflection reasons already argued:

```rust
/// A human authored this binding and the unit itself reports something else.
/// Never armable: driving a misidentified fixture is how pan data reaches a dimmer.
WrongUnit { authored: DeviceKey, reported: DeviceId },
```

`identified()` returns `None` for it, exactly as for `Displaced`.

**T4 — reassignment produces endpoints pointing at slots that do not exist, and nothing can detect
it.** *(Known gap 1.)*

Swap a Scarlett 18i20 for a MOTU 16A. Twelve `EndpointRef`s hold the old key. Old device `Absent` →
twelve registrations `AwaitingDevice`, holding `LastIntentional`. New device arrives `Discovered` with no
registrations bound to it. Nothing happens — **safe, correct, and useless.**

*Who decides* is already answered: the application adjudicates `Displaced` and repairs entity-specific
mappings. What is unanswered is what adoption *means* when the two halves of an endpoint have
different granularity. **Adoption is a device-level act with endpoint-level consequences.** Swap to an
eight-channel unit and `ch/8`…`ch/11` do not exist; the kernel rewrites the device key and produces
four endpoints pointing at nothing.

It cannot detect this, for two compounding reasons:

1. A device's slot inventory lives inside capability components, which per **T2** the kernel does not
   reliably have at all.
2. Even holding them, the kernel is **forbidden by its own membership test from interpreting them** —
   `Slot` is opaque, so *does `ch/11` exist on this unit* is not a question it may ask.

**Gap 1 is therefore not independent; it is blocked by the capability model.** Fixing T2 is a
prerequisite and dissolves part of it for free: with capabilities in the scan, slot survival is a
set comparison over data the kernel already holds, and it stays kind-free because it compares
provider-minted strings for equality without interpreting them.

What remains after that is a missing state. `RecoveryPhase` is `Nominal | Awaiting | Recovering |
PastDeadline | Retired`. An endpoint whose **device is present** but whose **slot is gone** is not
`AwaitingDevice` (the device is there), not `Nominal` (it is broken), and not `Retired` (nobody retired it).
Reassignment forces the endpoint-level analogue of `Displaced` — which is where it was always going to
appear once the audio document forced N:1.

**T5 — one `Claim` value cannot describe a device whose capabilities arrive over independent
transports.**

Art-Net output is connectionless; any number of controllers send and the node merges. RDM on the same
line is the opposite: E1.20 assumes **one discovery master**, with an explicit mute/un-mute protocol
to stop two controllers colliding mid-binary-search. One device, uncontended for output, exclusive for
management, and `Claim` is a single component on the device entity.

Not a DMX oddity — **the capability table in *The capability model* contains a second instance.**
`PTZ camera = SampleSource · Actuator` is in practice video over USB or NDI and control over VISCA
serial or HTTP. Zoom holding the video leaves PTZ control entirely free. One `Claim` cannot say that,
and `DeviceRecord.transport` is a **single** parent link, so such a device is not even a node in the
forest — it has two parents.

Two escapes:

- Move `Claim` to the endpoint. A type move; makes `Claim` N-per-device.
- **Rule: a device is the unit of claim and transport. Capabilities reached over independent
  transports are separate devices sharing a chassis, correlated by the kind crate.** Structurally
  free — the forest already handles it — but it **contradicts the capability table**, whose PTZ row is
  evidence for composition (see D5).

Recommend the rule. Counted as a type change because leaving both the table and the single `Claim` in
place is a live inconsistency a provider author will resolve wrongly.

**T6 — a provider renaming its slot vocabulary silently orphans every persisted binding.**

`DeviceKey.scheme` exists precisely so identity derivation can change: *"a plain `V1 -> V2` function
rewrites `scheme` and `value` and touches nothing else."* `EndpointRef { device, slot }` is the durable
binding — that is what `DeviceKey` being persistable is *for*. **Its device half is versioned and its
slot half is not**. (`DeviceProvider::migrate` was the vehicle for this at the time; it is struck — see *Provider contract*.)

The triggering refactor is already visible in the reference device. The Neo appends two capacitive
touchpoints to the **end of the button array** at indices 8 and 9, separated from real keys only by
`key_count()`. Any provider that later disambiguates that — `"key/3"` → `"keypad/r0c3"`, or splitting
touchpoints out of the key namespace — orphans every persisted binding, with no error, no migration
hook, and no way for the kernel to tell an orphaned endpoint from a valid one. A twelve-speaker layout
silently losing its channel map on upgrade is the same failure with worse consequences.

Give `Slot` the scheme treatment, or extend `migrate` to cover `EndpointRef`. Either is small; the
asymmetry is the defect. *(Known gap 3 concedes no compile-time validation; this is a different and
larger cost than validation.)*

**T7 — `Unreachable { since: Duration }` has no anchor, and BLE is where it matters.**

A BLE peripheral is *routinely* unreachable — advertising intervals, and a connection supervision
timeout up to 32 s — so this field is load-bearing here in a way it never is for a monitor. `Duration`
since when, measured by whom? The provider mints it, re-mints it every scan (scans are
whole-set), and two providers will disagree on whether it resets across a brief reappearance.

This design already solved it elsewhere and did not apply it here: *"reacquisition keys on the
topology revision, not elapsed time"* (§Resolving the four device facts, fact 1). Same fix —
`Unreachable { since: ProviderRevision }` — makes the value idempotent across scans, comparable
across providers, and turns a stated rule into an enforceable one. It is also the same anchor the fast
suspend/resume test case depends on. *(Interacts with known gap 2, which leaves the timing undesigned;
this fixes the representation, not the policy.)*

### Forces a rule

**R1 — `Displaced` is undecidable for every root device, including the one it was invented for.**

`Displaced` is *"a saved key matched nothing, but a unit of the right kind sits in the same slot on the
same transport."* **A device has no slot.** `Slot` belongs to `EndpointRef`; `DeviceRecord` has no slot
field and no ordering among siblings. For any root device — every local USB interface, every monitor,
the Stream Deck — `transport` is `None` on both sides and *same slot* is undefined.

This lands on the case `Displaced` was **created for**. Device fact 2 is a Stream Deck serial mutating
across replug (bitfocus/companion#1173): same kind `hid-panel`, same transport `None`, same slot —
there is no slot. The predicate is decidable only if *slot* quietly means *the USB port*, which is a
positional fallback: the nearest-neighbour fudge this crate exists to abolish, respelled in topology
instead of pixels.

**Replace position with uniqueness.** No new type:

> `Displaced` = same `kind`, same `transport` parent, saved key matched nothing, and **exactly one**
> unmatched live candidate of that kind under that parent.

Uniqueness is decidable, kind-free, and degrades correctly — two unmatched Scarletts is not
`Displaced`, it is `Unverified`, already defined as *"absent, or not unique among live units."* The
vocabulary is there. One candidate offers adoption; two ask a human.

**R2 — nothing decides whether a thing is a device or a slot, and the endpoint table gets DMX wrong.**

*Endpoints* says: *"DMX | one interface/universe | N fixtures at channel offsets."* Under RDM each
fixture independently has a UID (identity), can be absent while the universe is fine (presence), can be
the wrong model (T3), and changes its channel footprint when its personality changes (capabilities).
**Every kernel axis is per-device, and a DMX fixture needs every one of them.** Slots have none.

The machinery already exists and costs nothing: a fixture is a `DeviceRecord` with `transport:
Some(universe_key)` and `DeviceKey("fixture","rdm-uid","4D41:00A1B2")`, and its *channels* are the
slots. The forest absorbs it unchanged. Only the table is wrong.

What is missing is the rule, currently left to provider taste:

> **If it can independently be absent, be misidentified, or change its capabilities, it is a device.
> Otherwise it is a slot.**

It generalises without adjustment. A fixed output channel on an audio interface cannot independently
vanish → slot. A Dante or AVB networked speaker can → device. A Stream Deck key cannot → slot. A
Network Dock child panel can → device, which is what device fact 3 already concluded by hand. Same
shape as *The membership test* and belongs beside it.

**R3 — `Cohesion::SameHost` names a comparison the kernel cannot perform.**

The kernel holds `DeviceKey { kind, scheme, value }` and `transport: Option<DeviceKey>`. There is **no
host concept, and `value` is opaque by construction.** `SameDevice` is checkable — compare
`EndpointRef.device`. `SameHost` has nothing to compare, and a local USB interface is a root with
`transport: None`, so there is not even a parent.

Fix without a new type: **model the host as a root device and define same-host as "same root of the
transport chain."** The forest already computes it. It also makes multi-machine representable — which
known gap 2 and [[physical audio rendering architecture]] both flag as open — and makes
`Presence::Unreachable` propagate correctly, since a host going unreachable takes its devices with it
under the conjunctive rule already stated.

**R4 — the audio context needs a constraint that can be satisfied by construction, and a third
`Cohesion` tier cannot provide one.** *(Negative result: known gap 4 should close with no type
change.)*

Gap 4 guesses a scope tier between device and host. The CoreAudio case says the tier is not the
problem.

One output stream is opened against one device. Driving twelve speakers across two eight-channel
interfaces on one machine requires an **Aggregate Device** — a new CoreAudio device with its own UID, a
sub-device list, a designated clock master, and drift compensation. cpal will not span two devices with
one stream.

Run it through `Cohesion`: two interfaces, one machine → `SameHost` is **satisfied**, the kernel arms
the group, and it does not work. So `SameHost` is not merely uncheckable (R3), it is insufficient even
when checkable.

Does a `SameContext` variant rescue it? **No, and this is the refutation.** The aggregate device does
not exist at check time. Any variant naming it inherits the identical defect: the kernel needs the
device to exist in order to check the constraint, and the device only needs to exist *because of* the
constraint. Adding a tier moves the circularity without breaking it.

Once the aggregate is created it registers as an ordinary `DeviceKey("audio-interface","coreaudio-uid",
<aggregate uid>)`, all twelve channels become slots on one device, and plain `SameDevice` is correct
and sufficient. **`Cohesion` is a predicate over an existing topology and should stay one.** State that
satisfying a failed cohesion check — create the aggregate, re-register — is application work, which
*Ownership boundary* already assigns there (*"creating replacement entities and everything they
carry"*). **Gap 4 closes as no type change, with the reason recorded so it is not reopened.**

**R5 — two bindings to the same physical resource are accepted silently.**

Two speaker entities bound to `ch/7`: accepted. Two DMX fixtures whose channel ranges overlap — patch
37 with a 16-channel footprint, and patch 45 — accepted, because they are two distinct strings.
Overlapping patch is a routine rig error whose symptom is a fixture behaving insanely.

`Cohesion` exists to check exactly this class, and *The membership test* rules it kernel work: *"must
these endpoints share one device or one host? no — the reason does, the check does not."* **Collision
is the same shape of question and is unexpressible**, because `Slot` is opaque.

It is genuinely per-capability rather than universal: an audio output channel and a DMX channel range
are exclusive; a `Controls` button is deliberately **shared** — *open decision 3* has a keymap and a
tool-graph jack observing `key/3` at once. That variability is an argument for expressing it, not for
assuming it. Today the kernel permits both cases identically and silently.

**R6 — there is no departure action, and for a laser the safe state is neither the last intentional
state nor nothing.**

`Retain`/`Act` can retain a configuration and re-apply it. For a laser, a moving head, or any motorised
fixture there is a third configuration — beam off, shutter closed, axes parked — that must be **driven
on departure or inhibit**, not merely withheld. The recovery lifecycle has no departure action at all:
`AwaitingDevice` rests, and `Act::Never` is *"never touch the device."*

This is correctly provider-owned — the provider produces the scan, so it is first to know it lost
the unit, and can act unilaterally. But *Ownership boundary* claims the kernel *"owns the recovery
lifecycle and its closed transitions"*, and a lifecycle whose only transitions are acquire and retain is
not closed over a device that must be actively made safe. State it: **the kernel guarantees only that it
will not re-apply; driving to a safe state on departure or inhibit is provider work, and the kernel
neither sequences nor waits for it.** Without that sentence a provider author reasonably assumes the
kernel handles teardown, and for a laser that assumption is the incident.

### Documentation notes

**D1 — the Art-Net `NotApplicable` example is false, and the conclusion it supports gets stronger
without it.**

*Claim — the axis presence cannot carry* says *"Broadcast Art-Net and a display are `NotApplicable`,
not `Free`."* Art-Net nodes merge a bounded number of DMX sources — the spec's merge is two, HTP or
LTP — and report merge status in `ArtPollReply`. A third controller is not merged. That is a real,
observable contention state whose remedy is neither a permission grant nor a replug: *tell the operator
another console is on this universe*, with the other source's IP and short name as
`Contended { holder }`.

The membership count for `Claim` goes from four of seven to **five of seven**, so the section's
conclusion strengthens. But the sentence is offered as evidence of *natural absence* and a later reader
will lean on it. Replace the example with the self-describing broadcast sensor, which genuinely is
`NotApplicable`.

**D2 — the authored-presence formula is a provider default, not a kernel rule.**

*Open decision 1* states *"for authored devices presence is transitive through the transport —
`presence(interface) ∧ patched(address)`."* RDM makes authored presence independently observable: the
node answers, discovery runs, and the fixture at 37 is simply not there. Read as a rule, the kernel
would report `Present` for a fixture that is physically gone.

Nothing in the types forces the bad reading — `Presence` is *what the provider asserts* — but the
formula is written as a structural consequence. One clause fixes it: **that conjunction is the default a
provider uses when it has no better information, not a kernel invariant.**

**D3 — the stated acceptance test cannot detect four of the findings above.**

*v1 scope* item 3: *"Migrate screens, cameras, and monitor recovery. If any of the three gets worse, the
API is wrong."* **All three are one-slot devices.** None exercises N:1 endpoints, slot migration, slot
collision, cohesion, or reassignment — the machinery [[physical audio rendering architecture]] forced
into the design, and the source of T4, T6, R4, and R5. The test as stated passes with every one of them
unfixed.

Add a fourth migration target that is N:1. The Stream Deck qualifies — keys, encoders, LED rings, and
NFC on one device — and is already in the v1 slice, so the cost is naming it as an acceptance target
rather than only as a feature.

**D4 — the descriptor has no home in the provider contract.**

*Ownership boundary* gives the provider *"the descriptor — geometry, channel count, capability,
addressing, protocol family."* It appears in no type and no method. *Monitor round-trip* resolves
`MonitorInfo` geometry to *"provider descriptor plus blob"*, which reads as if the kernel plumbs it. It
does not, and should not. Say it: **descriptors live in provider-owned resources keyed by `DeviceKey`;
the kernel never carries one.** Its absence has a reader assuming a channel that does not exist — the
same assumption that produced T2.

**D5 — the PTZ row of the capability table implies one entity spanning two transports.**

Under T5 a PTZ camera is two devices. The audio-interface row (`AudioSink` N · `SampleSource` N) is
fine, because CoreAudio hog mode is per-device and the claim does not split. PTZ is the one row that
carries a claim boundary through it, and it is currently evidence for composition.

### What held

Attacked deliberately, did not break.

- **`Presence::{Present, Absent, Unreachable}`** survives all three devices, and the Absent/Unreachable
  split is *load-bearing* for Art-Net specifically: a silent node makes "fixture unplugged" and "node
  offline" genuinely indistinguishable, and the conjunctive parent rule gets it right with no special
  casing.
- **The forest, via `DeviceRecord.transport`.** It absorbed RDM fixtures-as-devices (R2), hosts (R3),
  Art-Net nodes, and network docks without modification. It is doing more work than the design credits
  it with — three recommended fixes above are *use the forest*.
- **`Captured::{Writable, Frozen}` over a single unguarded value.** Frozen DMX levels during node
  absence, a frozen laser projection blob, a frozen BLE calibration. No complaint at any point, and the
  temporal-gate versus provenance separation held under the audio case.
- **The `Retain` × `Act` decomposition.** T1 is about a missing *veto*, not about the axes.
  `{LastIntentional, OnRequest}` is exactly the cell a laser wants and exactly the cell the tri-state
  could not name. The design's strongest structural claim survives its most hostile device.
- **Poll-shaped with notification as a fast path.** `ArtPoll` cycles, RDM discovery sweeps, and BLE
  advertising are all polls. Three for three; monitors remain the outlier, as device fact 7 argued.
- **`DeviceKey.scheme`.** Its migration case appears unprompted on a device it was not designed
  against: a rig that starts hand-patched (`("fixture","patch","artnet/10.0.0.7/u1/c37")`) and later
  gains an RDM-capable node rewrites `scheme` and `value` to `("fixture","rdm-uid","4D41:00A1B2")` with
  a plain `V1 -> V2`. The binding survives and nothing else notices.

### Two negative results

Recorded as findings, because both *prevent* work.

1. **Known gap 4 closes with no type change.** A `SameContext` tier between device and host cannot
   work — the CoreAudio aggregate device does not exist at check time, so any variant naming it
   inherits the circularity (R4). `Cohesion` stays a predicate; satisfying a failed check is
   application work.
2. **Known gap 1 is blocked, not independent.** Reassignment needs slot-validity checking, which needs
   capabilities the kernel does not receive (T2 → T4). Sequence T2 first; part of gap 1 dissolves, and
   what remains is one missing endpoint-level phase rather than an undesigned area.

### Effect on the known-gaps list

| Gap | Status after refutation |
|---|---|
| 1 reassignment | blocked by T2; partly dissolves once capabilities are in the scan; one missing phase remains |
| 2 `Unreachable` timing | representation fixed by T7; policy still open |
| 3 `Slot(String)` | worse than stated — T6 (no migration hinge) and R5 (no exclusivity) are costs beyond validation |
| 4 `Cohesion` tier | **closed, no type change** (R4) |
| 5 `Displaced` adjudication | unchanged, and R1 must land first or the predicate is undecidable |
| 6 phase reference | not attacked |
| 7 claim arbitration | unchanged; T1's `Arming` axis is adjacent but does not cover requested seize |
| 8 capability timing | **closed by T2** — capabilities as scan data removes the need for a timing contract |

## Reconciliation

> **Historical review record (Phase E).** Kept for the reasoning. Where it disagrees with *Core types* or
> with a lettered decision (R, D, N), the later one wins; nothing in this section is normative.

*Migration proof* and *Adversarial refutation* were written in parallel against the same base. Read
against each other, **three of the four crossings are clean and one is a genuine conflict** that
neither result could have found alone. The conflict is item 2, and for a laser it is the difference
between a bug and an incident.

### 1. `Devices::armable()` over a co-reported device — fold, most-restrictive-wins

The refutation's gate reads `self.by_key.get(key)` and folds identity, presence, claim, and arming.
The proof made devices **co-reportable**: an equal `DeviceKey` from two providers merges into one
device with multiple contributors. So each folded axis now has N values, not one.

**Rejecting co-reporting for arming-bearing devices is not available.** "Is this the kind of device
where a second reporter is implausible" cannot be answered without knowing the device is a laser — it
fails *The membership test* outright. The gate must therefore fold, and the fold must be safe under
any contributor set.

> **Fold rule: the most restrictive contributor wins on every gate axis.** `Claim::NotApplicable` and
> `Arming::NotApplicable` are identity elements, never votes. Any contributor reporting `Contended`,
> `Blocked`, or `Inhibited` decides the device.

The justification is asymmetric information, and it is the same argument that made `Presence::Absent`
and `Presence::Unreachable` distinct. A provider that observes contention has information; a provider
that reports `NotApplicable` is saying *I have no notion of this axis*, which is silence, not
permission. Letting silence outvote observation is how a laser co-reported by a naive second provider
gets armed. The rule needs no new type and no kind knowledge — it is a fold over an enum with a
defined bottom.

Consistent with the proof's own duplicate-key rule, which resolves an ambiguous device set to the
*conservative* member (`Unverified`) rather than picking a winner. Same instinct, applied to a second
axis.

### 2. `Arming` versus start-plus-polled-`poll` — **the conflict**

The refutation's T1 gate is a **pre-fire check**. The proof made `apply` start an attempt that
`poll` polls across ticks. **An attempt is now in flight for an unbounded number of frames, and
nothing re-checks arming during it.** Gate at tick N, interlock opens at N+1, `poll` returns
`Done` at N+4, beam on. The T1 fix as written does not survive the proof's change to `apply`.

Neither section is wrong; the interaction is new. And the machinery to fix it already exists —
`Apply` re-checks `expected: DeviceId` and `revision: ProviderRevision` on every poll precisely because
the world can move under an in-flight attempt. **Arming and claim simply are not in that check.**

> **Rule: the gate is re-evaluated every `poll` poll, not only at authorisation.** An attempt whose
> device stops being armable is **aborted**, not merely stalled — it terminates without auto-resume,
> exactly as an `expected` mismatch already does.

```rust
pub enum ApplyProgress {
    Pending,
    Done,
    Failed(DeviceFault),
    /// The gate closed mid-attempt: claim lost, arming inhibited, or identity moved.
    /// Terminal. Never auto-retried — re-arming is a fresh authorisation.
    Aborted(GateFault),
}
```

Two consequences worth stating because both are easy to get backwards:

- **Abort does not drive the device.** The kernel stops; driving to a safe state is provider work per
  R6. A kernel that "aborts by applying the safe blob" would be issuing exactly the unreviewed
  automatic action T1 exists to prevent.
- **`Aborted` is not `PastDeadline`.** `PastDeadline` means *the deadline expired, not yet an error, ask
  again*. `Aborted` means *the authorisation is void*. Collapsing them re-permits the auto-retry.

This also makes the proof's own fact 4 rule enforceable: *a provider that must reset then render has
both steps inside one attempt and cannot report it is done between them*. If the interlock opens between
reset and render, the attempt is now required to die rather than complete.

### 3. `DeviceRecord.capabilities` over a co-reported device — union, and disagreement disarms

Two providers, two capability sets, one `DeviceKey`.

> **Union across contributors. But two contributors reporting the *same* capability with *different*
> slot sets is a disagreement, not a merge: the device becomes `Unverified`.**

Union alone is right for the common case and is a no-op for the motivating one — winit and xcap both
describe one display as `Surfaces` with one slot, and the union is that same single slot. It is also
the only rule that lets a provider contribute a capability its co-reporter cannot see.

Disagreement is different in kind. Sharing a `scheme` is an assertion that two providers *mean the
same identity space* (the proof's words). Two contributors that disagree about what slots the device
has are contradicting that assertion — the merge premise has failed, and the device is exactly as
ambiguous as the duplicate-key case the proof already resolves by disarming. Same resolution, same
reason, no new type. It also degrades safely: the failure of a scheme registry is a wrongly-merged
device, and this makes that failure loud instead of silent.

### 4. `identified()` rename — one live site, two false alarms

Checked. The only definition is *Core types*, `impl DeviceIdentity` — **renamed in place**. Prose uses
of the adjective "armable" in *Open decision 1*, *Resolving the four device facts* fact 2, and the
`WrongUnit` doc comment are describing the policy predicate, which is still called that; they are
correct as written and are **not** stale references to the method.

Neither *Migration proof* nor *Monitor round-trip* references `armable()` at all. Nothing to fix
there — the rename is contained.

### 5. Watchdog versus `ApplyProgress` — consistent, and the Essential bullet is fixed

The proof's split reads correctly against `ApplyProgress`. The kernel owns *did this attempt resolve
inside its deadline* (`Apply.deadline` bounding `Pending`) and *how many consecutive attempts have
failed* (`starved_opens` counts attempts, which needs no kind knowledge). The provider owns *what
counts as arrived*, which is precisely what returning `Done` from `poll` means — a delivered
frame for a screen, a stable position for a window, a completed image write for a panel. The contract
already encodes the split; the Essential list had not caught up.

The Essential bullet is **updated in place** to *bounded attempt with provider-supplied arrival evidence*.

One addition the refutation forces: the same poll now also carries `Aborted` (item 2), so the
provider-supplied evidence answers *did it work*, and the kernel answers *was it still allowed to*.
Those are different questions and only the second can be asked without kind knowledge — which is why
the split lands where it does.

### Net

One conflict (item 2), resolved by extending an existing re-check rather than adding a mechanism. Two
new fold rules for co-reported devices, both conservative, both no-new-type. One rename, contained.
One Essential bullet corrected. **No type from either result is withdrawn**, and the two results'
four combined type changes — `Arming`, `DeviceRecord.capabilities`, `scheme` as shared registry,
`apply`/`poll` — compose without further edits.

## The v1 slice — decisions

`## v1 scope` above states four targets and an estimate. This turns that into a decision: what ships, what does not, in what order, and what has to be true for it to work.

### What ships, what is deferred

The design now carries machinery no v1 device exercises. Shipping a type with no consumer is sometimes correct — it is correct when adding it later would force an audit of existing call sites, and wrong when it would be purely additive.

| Concept | v1 | Reason |
|---|---|---|
| `DeviceKey` / `DeviceId` / `DeviceIdentity` | **ships, exercised** | every kind uses it |
| Duplicate-key → `Unverified` | **ships, exercised** | two identical webcams is a live case, and this is what stops `nokhwa-name` presenting as strong |
| Co-reported devices (equal key, two providers) | **ships, exercised** | the whole 200 pt retirement depends on it — see risks |
| `Presence::{Present, Absent}` | **ships, exercised** | all four kinds |
| `Presence::Unreachable` | **ships, degenerate** | fact 7 makes it live: HID has no hot-plug, so "polled and gone" and "could not poll" are genuinely different in v1. Kernel treats it as not-armable and **never auto-decays** — decay has no v1 consumer because there are no remote nodes |
| `Claim` | **ships, exercised** | hidapi seizes by default; contention is the normal operating state, not an error |
| `Availability` (both axes) | **ships, exercised** | screens and cameras use `{LastIntentional, OnReturn}`; the primary camera's fix *is* this |
| `Retain::Declared` | **ships, no v1 producer** | no v1 device has a declared safe state — screens and cameras want their last state back. Ships because it is a third answer to *toward what*, and the alternative (drive the device to the safe state so it becomes the remembered one) is exactly the automatic action T1 forbids |
| `RecoveryPhase` / `Apply` / deadline | **ships, exercised** | replaces two hand-rolled generation guards |
| `DeviceProvider::{scan, capture, apply, poll}` | **ships, exercised** | four providers |
| `EndpointRef` / `Slot` | **ships, exercised only by Stream Deck** | the other three kinds are one-slot. This is why Stream Deck is a scope target and not a nice-to-have |
| `DeviceIdentity::Displaced` | **ships as a type, no adjudication** | Stream Deck serials genuinely mutate (companion#1173) — a v1 device fact. The variant must exist or a mutated serial silently becomes `Unverified` and the device disarms with no route back. v1 emits the event and logs; a human replugs. Who answers it, and whether an unanswered `Displaced` survives restart, is deferred |
| `DeviceIdentity::WrongUnit` | **ships, exercised** (corrected by D8) | a plain USB camera swap produces it — same port, different serial — so no RDM is required. D1's port-swap test is its v1 producer. It also keeps `Displaced` narrow: without it the pressure is to widen `Displaced`, whose adjudication (*adopt the candidate*) would move a patch to follow the wrong fixture |
| `Arming` veto | **ships, no v1 producer** | no interlock device in v1. Ships anyway because the refutation's point was that the *default* is unsafe without it — adding a veto later means auditing every `Act::OnReturn` site that already exists. Keep it minimal: a veto, no policy |
| `DeviceProvider::migrate` | **stub** | no v1 provider has a v2 blob. Default impl returns the blob unchanged |
| `DeviceKey` persistence | **serde only** | rigging produces durable keys in v1; the document format waits on [[versioned save format]]. Rigging must not block on another in-progress issue |
| `Cohesion` | **does not ship** | no v1 device has a cross-device grouping constraint; Stream Deck slots are all on one device, so the only reachable value is `None`. An enum with one value and no test is worse than its absence, and it attaches to a group rather than to any existing type — so adding it later is purely additive |
| Reassignment | **does not ship** | no v1 kind re-binds N entities to a swapped unit. `EndpointRef` keeps it expressible |
| `Unreachable` decay timing | **does not ship** | no remote nodes |

### Minimum published surface

The publish-chain ordering is stated in *Sequencing* above: screens and cameras migrate over the existing git dep, only `bevy_clerestory` waits, and nothing publishes until two real consumers have used it.

What `bevy_clerestory 0.3.0` needs present for it to compile (`0.2.0` shipped with the reconnect merge): `DeviceKey`, `DeviceId`, `DeviceIdentity`, `Presence`, `DeviceRecord`, `DeviceScan`, `EndpointRef`, `Slot`, `Availability`/`Retain`/`Act`, `RecoveryPhase`, `Apply`/`ApplyId`, `Captured`, `RigKey`, `Claim`, `Arming`, `DeviceProvider`/`ApplyProgress`, `RiggingPlugin`/`RiggingSet`/`add_device_provider`, and the public events. `Cohesion` is absent by the table above, which is the point of deciding it now rather than at publish time.

### Build order

**Superseded by *Sequencing*'s authoritative order (R3, R4a).** The correction is one insertion and one deletion: `bevy_clerestory` is refactored onto the kernel **immediately after** the kernel, as consumer #1, and the publish step no longer sits in the middle. Everything else below stands, and the reasoning for screens-before-Stream-Deck is unchanged.

1. **Kernel, no providers.** Pure logic against hand-built scans — no I/O, no Bevy device access, so it is unit-testable in full. Every rule in *Core types* gets a test here.
2. **`bevy_clerestory` refactored onto it (R3).** A working implementation moved onto the kernel — the only step that can decompose or kill `Retain` × `Act` before providers multiply. Carries the v3 → v4 state migration (D5).
3. **Screens provider — not Stream Deck.** Screens already have a working reconcile, a real platform id, a retry timer, and tests to port. It is the cheapest provider to write and the fastest signal that the kernel shape is wrong. Stream Deck has no existing code at all; making it the first provider would conflate "is the kernel right" with "does our HID stack work".
4. **Cameras, plus the saved-key migration.** Proves the hint-key path (a port-derived camera id is `Synthesized`, duplicates → `Unverified`) and the asynchronous `apply`/`poll` path against a second, differently-shaped backend.
5. **Stream Deck.** The only genuinely new work — enumeration, claim, reconnect, error classification, 1–2 days with nothing to copy — and the only multi-slot consumer.
6. **Publish `hana_rigging` → clerestory `0.3.0` → hana's pin bump → Phase 1.** Delivery only; it gates no development (R4a, R4b).

**The first real integration test sits between 4 and 5**, before Stream Deck and before publish: screens and cameras registered against one kernel, one retry pacer, a display and a webcam unplugged in the same run. That is the moment the crate's central claim — two independently written machines collapse into one — is either true or false. Everything after it is application of a proven core; everything before it is unvalidated.

### What could make v1 fail, ranked

1. ~~**Co-reported devices do not actually co-report.**~~ **Verified from source against the locked versions — this risk is retired for macOS.** The claim was that `bevy_clerestory`'s `native_monitor_id` and `xcap`'s `Monitor::id()` produce the same number for one display; the whole 200 pt retirement rests on it. Both sides bottom out in the same Core Graphics namespace, with no remapping and no enumeration index anywhere on either path:

   - **clerestory** (`monitors.rs:252-264`) is `u64::from(handle.native_id())` on macOS — a widening, nothing else.
   - **winit 0.30.13** (`platform_impl/macos/monitor.rs:112,145-147`) holds a `CFUUID` and computes `native_identifier()` as `CGDisplayGetDisplayIDFromUUID(uuid)` — a `CGDirectDisplayID`.
   - **xcap 0.9.6** (`macos/impl_monitor.rs:56-85,129-131`) fills `cg_direct_display_id` straight from `CGGetActiveDisplayList` and `id()` returns that field unchanged.

   Same integer, same source, same run. Hana's lockfile pins exactly these two versions, so this is a fact about the code that will build, not about the crates in general. **The two remaining exposures are version drift and platform**: a winit or xcap bump can change either path, and neither Windows (clerestory hashes a `String` name) nor Linux (X11/Wayland output ids) has been checked at all — so the co-report merge is a macOS-verified claim and an assumption everywhere else.

   **A different assumption is still open, and it is about persistence, not merging.** `CGDirectDisplayID` is assigned by the window server. Apple does not guarantee it is the same integer after a reboot or a re-plug, so it is sound as a *within-run* join key and questionable as the persisted `value` of a monitor's `DeviceKey`. The merge is safe; **durability across restarts is not established** — see *Known gaps*.
2. **Retired rig versus live texture handle.** Silent, and it corrupts rendering rather than erroring. `ScreenSession` entries are never removed today precisely so a material's binding stays valid; under the kernel those are two lifetimes. Needs the test the migration proof named.
3. **The `Arming` veto has the wrong shape.** It ships with no v1 producer, so a wrong shape is not discovered until a laser exists. Mitigated by keeping it a veto with no policy — the smallest thing that closes the hole.
4. **`Claim::Contended` cannot be detected, only inferred.** A seizing open *succeeds* over the running Elgato app while silently disabling its input queues, so "we opened it" does not tell us we should have. If contention is undetectable in practice, the axis has a producer problem. Ranked fourth because it degrades to today's behaviour rather than breaking anything new.
5. **Capability-model churn from the refutation.** Additive; lowest risk.

### The cut line

If this ships in half the time, this survives:

- The kernel: `DeviceKey` identity, presence diff, duplicate-key → `Unverified`, `Availability`, `RecoveryPhase` + `Apply` + deadline, `DeviceProvider` with `apply`/`poll`.
- **clerestory as consumer #1, plus the screens provider and the cameras provider.**
- The co-report verification from risk 1.

One provider proves nothing. The entire argument for the crate is that two independently written session machines and generation guards — byte-for-byte identical predicates in two files — collapse into one. A single-provider v1 is not a smaller version of this project; it is an unfalsifiable one.

~~Cut, in this order: monitor recovery (blocked on the publish chain anyway), Stream Deck, `Claim` …~~ **Corrected by R3/R4a: monitor recovery is no longer cuttable and its stated reason is void.** Nothing is blocked on the publish chain, and clerestory is consumer #1 — cutting it would remove the one step that tests the kernel against a working implementation. The revised cut order is: Stream Deck, then `Claim` (with Stream Deck gone there is no device whose contention we can detect), then `Displaced`, then `Arming`.

The cost of cutting Stream Deck, stated plainly: `EndpointRef` and `Slot` ship with no multi-slot consumer, so the N:1 machinery the audio model forced is carried untested. That is acceptable for a cut-down v1 and unacceptable for the full one.

Which surfaces the thing worth naming: **the Stream Deck is the reason for this crate but not the proof of it.** If time is short, prove it on clerestory, screens, and cameras first and ship the device second.

## Branch point and build sequence — decisions

**Index — the verdicts, so nothing below has to be read to know what was decided.** The subsections keep the evidence and the reasoning; their content is folded into *Sequencing*, *Core types*, *The v1 slice*, and *Worktree Placement*, which are the normative statements.

| # | Verdict | Status |
|---|---|---|
| R1 | Land the recovery work on `bevy_hana/main` before branching for rigging | **met** — gate G1, `1021c737` |
| R2 | `hana_rigging` lives in the `bevy_hana` monorepo; catalyst's `hana_hardware` is absorbed and deleted | decided; 29 files re-pointed (T2) |
| R3 | `bevy_clerestory` is **consumer #1**, refactored onto the kernel before any other provider | decided |
| R4a | The crates.io publish chain gates delivery, not development | decided |
| R4b | hana consumes `bevy_clerestory` by monorepo git rev, not crates.io `0.1.1` | decided; gate G3 |
| R5 | All hana-side changes land on `init/hana_catalyst` | decided; gates G6, G9 |
| R6 | `DeviceKey` is human-readable and carries proof-vs-hint in its type (`Reported`/`Synthesized`) | decided; refined by D6 |
| R7 | `DeviceId` adopts clerestory's runtime-handle convention and is never persisted | decided; **its compile-error guarantee withdrawn by D2** — the handle lives in a side table |
| R8 | An unidentifiable device never matches, including itself | decided; enforced by absence of `PartialEq` (D3) |
| R9 | Attachment-path evidence | **closed by D1** — no longer an open decision |

Reviewed 2026-07-29. *Sequencing* and *The v1 slice* were both written on the premise that **nothing of clerestory's recovery design is implemented.** That premise is false as of 2026-07-28. Two in-flight branches independently built large parts of this design:

- **`bevy_hana` `feature/reconnect`** (33 commits, 0 behind main) — `+37,101/−3,193` across 159 files, including `monitors/identity/{mod,native,edid,registry,configuration/*}`, `monitors/topology.rs`, `recovery/{registration,application_controlled,fallback_and_return,monitor_probe}`, `restore/{restore_attempt,settle_state,target_position/*}`, `persistence/captured_window_state.rs`. `src/monitors.rs` is deleted on that branch — the file *Migration proof* and the risk-1 verification both cite.
- **hana `init/hana_catalyst`** (67 ahead, 1 behind) — created `crates/hana_hardware`, 225 lines, already threaded through **29** source and manifest files (corrected by T2; the figure first recorded here was ~20).

Neither was built knowing a general hardware interface was coming. The resolution is **not** to treat them as competitors: `hana_rigging` is built first and both become consumers of it. These decisions settle where it branches from and in what order that happens.

The convergence is itself evidence. Three independent derivations landed on the same three concepts:

| this design | `hana_hardware` (catalyst) | `bevy_clerestory` (reconnect) |
|---|---|---|
| `DeviceKey` — persistable | `HardwareKey{kind,namespace,stable_id}` | `PanelFingerprint` — FNV‑1a over EDID / ColorSync UUID bytes |
| `DeviceId` — process-local | *(absent)* | `MonitorId(u64)`, documented *"must not be persisted"* |
| `DeviceIdentity::Unverified` | *(absent)* | `MonitorIdentity::Anonymous` — *"never matches anything, including itself"* |

### R1 — Fold the recovery work into the mainline before starting

**Decided: yes — `feature/reconnect` fast-forwards onto `bevy_hana/main` before `hana_rigging` branches.** The user is merging it in a separate session; treat post-merge `main` as the branch point.

Mechanically free: main is 0 commits ahead of that branch, so it is a fast-forward with no conflicts.

Rejected — **build `hana_rigging` on top of the unlanded branch**: turns a landable 33-commit branch into a 60+ commit branch that cannot land alone, and keeps `feature/widgets` and `feature/rubric` building on a main missing the recovery work.

Rejected — **start from pre-merge `main`**: the kernel would be designed against a clerestory that has none of this, so every judgment about *what the window library needs from a device registry* would be made without reading the code that needs it. The migration would then be designed twice.

Accepted cost: main briefly carries a monitor-identity implementation that `hana_rigging` will hollow out. Refactoring working, tested code behind a shared kernel is ordinary; refactoring code that was never exercised is not.

### R2 — Crate home, and `hana_hardware` is absorbed

**Decided: `hana_rigging` lives in the `bevy_hana` library monorepo at `crates/hana_rigging`; `hana_hardware` is absorbed and deleted, its ~20 call sites re-pointed.**

The home is **forced, not chosen.** `bevy_clerestory` is in that workspace and publishes to crates.io, and a crates.io crate can depend only on other crates.io crates — never on a crate that exists only in the hana app repo. A `hana_rigging` in the app repo could never be consumed by its largest consumer. The monorepo's `members = ["crates/*"]` glob means adding the crate needs no manifest edit.

That decides `hana_hardware`'s fate by elimination: it is in the app repo, so `hana_rigging` **cannot** depend on it — the dependency would point library → app, which is both backwards and unpublishable.

Rejected — **let both key types coexist**: the same physical monitor would then carry a different key type depending on which subsystem observes it (`HardwareKey` as a screen-capture source, `DeviceKey` as an output window). The two could never be compared, so the co-report merge would be impossible and the 200 pt geometry tolerance would survive. Retiring that tolerance is the largest single claim in this document.

Absorption is mechanically small because R6 gives `DeviceKey` `HardwareKey`'s exact shape: most call sites change only their `use` line. *When* those files are touched is R5.

### R3 — `bevy_clerestory` is consumer #1, not the last

**Decided: refactor `bevy_clerestory` onto the kernel first — before the screens provider and before cameras.** This supersedes the build order in *The v1 slice*, which put screens first and monitor recovery last.

Both original reasons for screens-first have expired. Screens were chosen because clerestory had nothing implemented — it now has the most complete implementation of the three. And monitor recovery was placed last because it was believed gated on the crates.io publish — R4 shows that gate does not exist during development.

Why clerestory first:

- **Same workspace, so a path dep.** Change a kernel type, rebuild, see the result. Screens and cameras live in the app repo and reach `hana_rigging` through a git dep, so every kernel change needs a commit and a pin bump before the consumer compiles against it.
- **It exercises the most of the kernel**, making it the harshest available test of the shape.
- **It forces the `Retain` × `Act` decomposition immediately.** clerestory shipped `FallbackAndReturn` as one combined behaviour; this design claims it decomposes into two independent axes plus a clerestory-private behaviour. That is the hardest decomposition in the document. Doing it first validates or kills the two-axis model *before* two more providers are built on it.

Accepted cost: clerestory is the largest of the three refactors, so the first milestone is also the slowest.

The cut line in *The v1 slice* is unchanged in substance — screens and cameras both still ship, because one consumer proves nothing — but its stated *reason* for cutting monitor recovery first ("blocked on the publish chain anyway") is void; see R4.

### R4a — The publish chain gates delivery, not development

**Decided: rewrite `## Sequencing` so the crates.io publish is the final delivery step, not a mid-sequence gate.** This is the second correction to that section; the first was still wrong.

The false premise was that `bevy_clerestory` cannot depend on `hana_rigging` until `hana_rigging` is published. Inside the monorepo it depends on it by **path plus version**, which compiles with nothing published. At release time the existing process rewrites path deps into version deps — the release branches carry a literal `chore: pin workspace path deps for publish` commit (see `release-bevy_clerestory-0.1.1`). Nothing in development waits on a publish.

What the publish actually gates: delivering clerestory changes **into the hana app**, which downloads `bevy_clerestory = "0.1.1"` from crates.io. That chain is publish `hana_rigging` → `bevy_clerestory 0.2.0` → bump hana's pin. See R4b, which may remove even this.

What it never gated: the screens and cameras providers. They are in the app repo and reach `hana_rigging` through the git dep hana already holds on the monorepo.

The standing decision — **do not publish until two real consumers have used it** — is retained, and is now free: no work sits behind it.

### R4b — hana consumes `bevy_clerestory` by git rev, not crates.io

**Decided: switch hana's `bevy_clerestory` dependency from crates.io `0.1.1` to the monorepo git rev it already uses for every other library crate.** With R4a this removes the publish from the critical path entirely: monitor recovery reaches the app the way `hana_lading` does, and the `hana_rigging` publish → `0.2.0` → pin-bump chain disappears.

The reason the inconsistency exists is **not** deliberate dogfooding of the published artifact: **not every crate has been migrated into the `bevy_hana` monorepo yet, and that migration is intended.** Git dependencies in the binary are an accepted temporary state during it. This decision therefore is not a new practice, it is the existing transitional one applied to one more crate.

Publishing `bevy_clerestory` to crates.io continues regardless — this changes only what hana consumes. The packaging check that crates.io consumption incidentally provided (files missing from the archive, features that only resolved as a path dep) is better served by a publish dry-run than by making the app the canary.

Timing: make the switch when the recovery work needs to reach the app, not before.

### R5 — All hana-side changes land on `init/hana_catalyst`

**Decided: the screens provider, the cameras provider, and the `hana_hardware` deletion are all done on `init/hana_catalyst` (worktree `/Users/natemccoy/rust/hana_tool_graph`) — none of it on hana `main`.**

The conflict surface that forced this, measured on `main...init/hana_catalyst`:

- **Camera path heavily rewritten** — `hana_video/src/camera.rs` (+330), `secondary.rs` (785 lines changed, net large deletion), `render.rs` deleted and split into `render/{mod,sessions,sources,diagnostics}` (+1,840), `stream/mod.rs` (+225). `camera.rs` is *also* uncommitted in that worktree, so it is changing live.
- **Screen-capture path untouched** — nothing under `hana_video/src/screen/` is modified, so *Migration proof*'s screens evidence (`screen/session.rs:295-299`) still holds. Only the cameras half needs re-verification against the rewrite.
- 23 uncommitted files in the worktree, including `crates/hana/src/hardware/camera.rs`.

Rejected — **split it (screens on `main`, cameras on catalyst)**: this was the initial recommendation and it is worse. Any hana change made on `main` gets merged into catalyst anyway, and catalyst is 67 ahead, so `main` is the stale side.

This **removes the only external scheduling dependency** in the sequence. The integration test — screens and cameras on one kernel, a display and a webcam unplugged in the same run, the moment the design is proven or falsified — no longer waits for catalyst to land, because it happens inside catalyst.

**Consequence on the dependency pin:** hana reaches the monorepo by git rev (R4b) and the kernel will initially live on an unlanded `feature/rigging`. So catalyst temporarily points its monorepo dep at `feature/rigging` rather than `main`, and re-pins to `main` once the kernel lands. One line in each direction; catalyst is briefly pinned to a branch instead of a mainline commit.

### R6 — `DeviceKey` is human-readable, and carries proof-vs-hint in the type

**Decided: `DeviceKey` takes `hana_hardware`'s human-readable shape, not clerestory's hashed `u64`, and the id is a nested enum so the outer type is always a key while the variant states how much the key proves.**

```rust
pub struct DeviceKey {
    kind: DeviceKind,        // Camera | Display | … (#[non_exhaustive])
    id:   DeviceIdSource,
}

pub enum DeviceIdSource {
    /// The unit reports an id unique to it — a serial number, an AVFoundation
    /// unique id, a ColorSync display UUID. A match is **proof** of sameness.
    Reported    { scheme: String, id: String },
    /// No unique id available; derived from traits the unit does report
    /// (model, resolution, hashed EDID). Two identical units produce the same
    /// value, so a match is a **hint**, not proof.
    Synthesized { scheme: String, digest: String },
}
```

Rejected — **clerestory's `PanelFingerprint`** (FNV‑1a `u64` over evidence bytes): a saved file holds an opaque number testable only for equality. The scheme is unrecoverable, so a fallback fingerprint and a real unique id share one number-space and can appear to match. And clerestory's own doc calls the hash match *"a strong hint rather than proof"* whose restored position is still range-checked — a deliberately-a-hint value cannot carry exact-match device sameness, which is what retires the 200 pt tolerance.

What the nested enum buys over a flat `{scheme, stable_id}` pair: **proof-versus-hint becomes compiler-enforced.** Reading the id requires matching the variant, so the kernel cannot silently treat a lookalike match as certainty. `Devices::armable()` requires `Reported` for **every** device — not only ones judged dangerous. **Corrected by T1:** an earlier draft of this sentence said "any dangerous fixture", which contradicted R8 and is unimplementable, because `DeviceKind` carries no danger classification and adding one would put device-specific policy in the kernel. A laser must never energize because two identical units hashed to the same digest, and the unconditional rule makes that a type rule rather than a comment. Restore is the separate, weaker gate — `Devices::restorable()` accepts an unambiguous `Synthesized` key, because restoring writes configuration the device already had while arming energizes an output.

It also settles where *no evidence at all* goes — Wayland withholding EDID, a virtual display reporting nothing. That is not a weak key, it is **no key**, so it lives in `DeviceIdentity::Unverified` rather than as a third variant here. A `DeviceKey` that exists always says something; the variant says how much. This narrows R8: the never-matches-itself rule attaches specifically to `Synthesized`.

Accepted costs: larger to store and compare than a `u64` (irrelevant at these device counts); clerestory's working fingerprint code must produce a scheme plus an id instead of a hash; and on Windows/X11 the raw material is EDID bytes, so that path records a hex-rendered digest as `Synthesized` — strictly more information than the bare number, but not human-readable there.

### R7 — `DeviceId` adopts clerestory's runtime handle, and the catalyst never-persist convention

**Decided: adopt `bevy_clerestory`'s `MonitorId(u64)` design as `DeviceId`, and enforce never-persist the way `hana_catalyst` already does — `Reflect`, no serde, private field — rather than by separating components.**

clerestory's type is *"an opaque process-local token… valid only for the lifetime of the current `App`… not derived from an evidence hash and must not be persisted"*, deriving `Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect` with no serde. Adopting it is free and there is no reason to design a second one.

The failure it exists to prevent: a save records runtime handle `7` for a projector; next launch enumeration hands `7` to a webcam; a lookup by runtime handle resolves the projector's saved binding to the webcam, compares equal, and proceeds. Nothing errors, nothing logs.

**This is a fourth independent derivation of the same idea.** `hana_catalyst/src/identity.rs` had already solved it: `ToolId` is stable, serde-bearing, private-field, minted only by validated spawn paths; `ToolDefId` is *"runtime-derived and… never serialized as authored graph state"*; `PortId` is *"runtime-derived, definition-local, and deliberately lacks serde support so definition reordering cannot change a saved connection"* — the identical failure mode, already fixed by the stricter mechanism.

**The convention, stated once so a fifth subsystem does not invent a fifth spelling:** the durable value gets serde and a private constructor; the runtime value gets `Reflect`, no serde, and a private field; resolution happens once at load. ~~Enforcement is a **compile error**, not a comment — a persisted component derives `Serialize`, so a non-serializable runtime handle cannot be a field of it. Rejected in favour of this: separating the runtime handle into its own component, which relies on remembering to keep them apart.~~ **Corrected by D2:** there is no compile error. hana saves through Moonshine, which serializes reflected components structurally without needing `Serialize`, and `DeviceId` must stay reflected so BRP can see it. What actually enforces the rule is that the runtime handle lives in no component at all — it lives in a side table on `Devices`, so the save path has nothing to walk. See `### D2 decided`.

`hana_rigging` shares the *pattern*, not the *types*. It cannot depend on catalyst's — catalyst is in the app repo, the same wrong direction as R2 — and the meanings differ: a tool id names an authored graph node, a device key names a physical unit that can be unplugged mid-run.

#### Deferred option: an associated-type trait — not adopted, with a stated trigger

Recorded because it is a real answer looking for its moment, and the moment is identifiable:

```rust
trait Identified {
    type Durable: Serialize + DeserializeOwned + Eq + Hash;
    type Runtime: Reflect + Copy + Eq + Hash;
    type Registry;
    fn resolve(d: &Self::Durable, reg: &Self::Registry) -> Option<Self::Runtime>;
}
```

What it gets right that a shared generic newtype cannot: each domain keeps its own concrete shapes — a newtype over an index on one side, a struct with `DeviceIdSource` on the other — while the trait states the *relationship* between them, and `type Durable: Serialize` turns half the convention into a checked bound.

Why it is not adopted now:

1. **It cannot state the half that matters.** Rust has no negative bounds, so `type Runtime` cannot require *not* `Serialize`. A sealed `NotPersisted` marker is a promise, not a proof. Encoding only the safe half is arguably worse than no trait, because the rule then *looks* captured and nobody checks the use site.
2. **No generic code would consume it.** `fn load_all<T: Identified>()` resolves, then handles failure — and failure means opposite things: a missing device is normal operation that must produce a policy decision, a missing tool definition is a validation error that aborts the load. The divergence is the whole function body; only the `resolve` call is generic.

**Trigger (sharpened by T9).** The earlier wording — "revisit when, not whether" and "if devices need catalyst's staging" — presumed the trait before any generic caller existed, and it named the wrong thing: catalyst's staging is a *concrete* sequence over its own graph, not an interface waiting for a second implementor. Write the trait only when all four of these hold of two **implemented** load paths:

1. Both hold deserialized durable references aside, unpublished, while loading.
2. Both resolve an entire batch against current registries, not one reference at a time.
3. Both validate cross-references across the batch before anything is published.
4. Both publish atomically **and their failure policy is compatible** — the same generic body can handle a failure in both.

Condition 4 is the one that fails today, and it fails on purpose: a missing device is normal operation that must produce a policy decision and leave the show running, while a missing tool definition aborts the load. As long as that difference stands, the two paths stay concrete and the trait stays unwritten. When it stops standing, extract only the interface that the shared function actually calls — not the whole relationship. Id types are twenty lines and not worth abstracting; a shared load *body* would be. See [[hana-identity-key-id-convention]].

### R8 — What an unidentifiable device is allowed to match

**Decided: a three-part rule.**

1. **No key at all (`DeviceIdentity::Unverified`) never matches anything, including another `Unverified`.** Adopted from clerestory verbatim — *"failing to identify two panels is not evidence that they are the same one."* The trap this avoids: modelling the state as `Option<DeviceKey>`, where derived equality makes `None == None` and two anonymous displays collapse into one panel, anchoring a saved position to whichever enumerated first.
2. **A `Synthesized` key match is sufficient to restore, never sufficient to arm.** Worst case for restore is a window landing on the wrong one of two identical displays — recoverable by dragging it. Worst case for arming is a fixture energizing because a lookalike matched, which is not. Refusing `Synthesized` outright was rejected: most consumer monitors on Windows and X11 have only EDID-derived ids, so window restore would stop working for the common case. This graduated middle is what R6's compiler-enforced variant exists to make un-bypassable — `Devices::armable()` requires `Reported`.
3. **The same `Synthesized` digest appearing twice in one scan demotes both to `Unverified`.** Two identical monitors, same digest: the system then knows it cannot tell them apart and falls back to positional restore rather than binding to whichever came first. This reuses the existing duplicate-key rule, so it is no new machinery.

### R9 — Attachment-path evidence — RAISED, NOT DECIDED

Raised by the user immediately after accepting R8: *"usually hardware has extra information that maybe we could utilize when we don't have certainty — like what physical device port or virtual device port or file or whatever it is attached through."*

This is a **third evidence axis** the design does not currently have, and it is orthogonal to the other two on purpose:

| evidence | distinguishes two identical units? | survives moving the cable? |
|---|---|---|
| the unit's own id (serial, ColorSync UUID) | no, if neither reports one | **yes** |
| attachment path (USB bus/port, DP‑1 vs HDMI‑2, DMX universe+address, device file) | **yes** | no |

**They fail in opposite directions**, which is what makes combining them useful — and exactly why the path must **not** be folded into `DeviceKey`. Fold it in and moving a cable changes the key of a device that was uniquely identifiable, losing a binding that previously worked. That would be a regression on the strong case in exchange for helping the weak one.

The shape this implies: attachment path is **secondary evidence**, recorded alongside the key and consulted only when the key is `Synthesized` or absent. It gives R8 rule 3 an escape hatch — same digest but *different connectors* means two distinct devices rather than a demotion, and a saved binding that also recorded the connector can pick the right one.

**A concrete reason to take this seriously in v1:** `hana_hardware`'s own doc example is `namespace: "avfoundation-unique-id", stable_id: "0x1424000"`. A hex value of that form is characteristic of a macOS **USB location id** — that is, an attachment path — not a serial number. If AVFoundation's `uniqueID` is location-derived for USB cameras, then hana's camera key **already** silently changes when a camera is moved to a different port, and today that is spelled as a `Reported` id when it is really `Synthesized` path evidence. **Verify this from the AVFoundation docs before writing the cameras provider** — it decides whether existing camera bindings survive a replug, and it is cheap to check.

Open sub-questions, none yet decided: whether the path is a field on `DeviceRecord` or its own reflected component; whether it is persisted (it must be, to disambiguate at load) and if so how a path is spelled portably across USB / DisplayPort / DMX / device files; and whether a path-only match with no key at all is ever allowed to bind.

## Worktree Placement

**Base:** `bevy_hana/main` at or after `1021c737` — the post-`feature/reconnect` mainline. New branch `feature/rigging`, worktree `/Users/natemccoy/rust/bevy_hana_rigging`. Chosen per R1; branching from pre-merge `main` or from the unlanded `feature/reconnect` were both rejected there.

Note the version arithmetic moved: `bevy_clerestory 0.2.0` shipped with the reconnect merge and the workspace is on `0.3.0-dev`, so the rigging-consuming clerestory release is **`0.3.0`**, not the `0.2.0` named in *The v1 slice* and *Sequencing*.

**Status:** Not created. No `feature/rigging` branch and no worktree exist yet; nothing has been generated. Design closed (phases A–G): R1–R9 decided (**R9 closed by D1**, not raised), D1–D8 and N1–N7 decided, T1–T9 reconciled into the sections above. Remaining before creation: compile the phased plan.

**Gates:**

Each row names how it is checked, who owns it, and what it blocks. A gate with no verification command is not a gate.

| # | Gate | How it is verified | Owner | Blocks | State |
| --- | --- | --- | --- | --- | --- |
| G1 | `feature/reconnect` merged into `bevy_hana/main` | `git merge-base --is-ancestor <reconnect-tip> main` | bevy_hana | everything | **MET** 2026-07-29 (`1021c737`; `bevy_clerestory 0.2.0` released, workspace on `0.3.0-dev`, reconnect worktree removed) |
| G2 | **Rewritten — the original was factually wrong (T2).** It said `init/hana_catalyst` re-points its `bevy_hana` git dep at `feature/rigging`. **There is no `bevy_hana` dependency to re-point**: catalyst's git deps point at `natepiano/hana.git`. The real gate is that catalyst consumes `hana_rigging` from **one** source, pinned to an exact immutable `rev` | `cargo tree -i hana_rigging` shows exactly one source; `Cargo.lock` committed; `git cat-file -e <rev>` on a pushed, fetchable commit; **no force-push** to the pinned branch | catalyst | all hana-side provider work (R5) | **Not met** |
| G3 | hana switches `bevy_clerestory` from crates.io `0.1.1` to the monorepo git rev (R4b) | `cargo tree -i bevy_clerestory` shows the git source; app builds clean | hana | only work that must reach the running app | **Not met** |
| G4 | AVFoundation `uniqueID` is a real unit serial versus a USB location id (R9) | reviewer evidence read against AVFoundation and IOKit | — | cameras provider | **RESOLVED by D1** — it is a USB location id for UVC devices, so camera keys are `Reported` only from a recovered unit serial and port-derived values become attachment evidence plus a `Synthesized` key. R9 closed |
| G5 | `init/hana_catalyst` landed on hana `main` | `git merge-base --is-ancestor init/hana_catalyst main` | hana | nothing in the build order — **independent (T4)**; needed before G2's pin can be retired | **Not met** |
| G6 | **Clean baseline in the catalyst worktree** — a migration cannot be verified on top of uncommitted work | `git status --porcelain` empty in `~/rust/hana_tool_graph` (23 dirty paths at last count, 139 insertions / 453 deletions across three overlapping files) | catalyst | the catalyst screens and cameras phases | **Not met** |
| G7 | **G2's pin retired** — the temporary `rev` replaced by a released or mainline source | `git merge-base --is-ancestor <pinned-rev> origin/main` | catalyst | publish | **Not met** |
| G8 | **clerestory persisted state migrates v3 → v4** (D5) | a v3 RON file loads, restores its windows to the same physical monitors, and is rewritten as v4 with classified keys | clerestory | the clerestory refactor phase | **Not met** |
| G9 | **Catalyst's saved `HardwareKey` values migrate** — the key is serialized in authored camera settings (`hana_mimesis_tools/src/camera_source.rs:54`), is the ownership-table key (`hana/src/hardware/ownership.rs:73`), and carries claim epochs, so R6's nested enum is **not wire-compatible** | a saved show authored before the migration loads and binds the same physical cameras | catalyst | catalyst cameras and key migration | **Not met** |
| G10 | **Scheme names are registered and unique** (D6) — a typo must fail at startup, not silently mint a second device | startup registration rejects duplicates and unknown names; test asserts both | hana_rigging | the second provider | **Not met** |
| G11 | **Publish dry-run passes for the whole chain** | `cargo publish --dry-run` for `hana_rigging`, then `bevy_clerestory` | bevy_hana | delivery only, no development (R4a) | **Not met** |

Only G1 is met and only G4 is resolved. G2 and G5 are independent of each other (T4): G5 is not a precondition for provider work, it is a precondition for retiring G2's pin, which is G7.

**Scope now:**

- `crates/hana_rigging` in the `bevy_hana` monorepo (R2) — kernel first, no providers, fully unit-testable against hand-built scans.
- Types as decided: `DeviceKey { kind, id: DeviceIdSource }` with `Reported`/`Synthesized` (R6); `DeviceId` as `Reflect` with no serde and a private field (R7); the three-part unidentifiable rule (R8).
- `bevy_clerestory` refactored onto the kernel as **consumer #1**, by path dep, no publish (R3, R4a). This is where `Retain` × `Act` either decomposes or dies.
- On `init/hana_catalyst` only (R5): screens provider, cameras provider, `hana_hardware` deleted with its call sites re-pointed, and the two-provider integration test — a display and a webcam unplugged in one run.
  - **Corrected by T2:** the deletion touches **29** source and manifest files, not the ~20 stated earlier. Counted in the catalyst worktree, so the phase estimate should carry 29.
  - Also from T2: the migration cannot be verified on top of uncommitted work — the catalyst worktree had 23 dirty paths, including three overlapping files. Committing it clean is gate **G6**.
- Stream Deck provider — the only multi-slot consumer, and the only genuinely new code.

**Scope deferred:**

- **R9 attachment-path evidence** — storing attachment path as a first-class third evidence axis. Must not be folded into `DeviceKey`. The *evidence question* underneath R9 is closed: D1 established that AVFoundation's `uniqueID` is a USB location id for UVC devices, so a camera key is `Reported` only from a recovered unit serial and the port-derived value is attachment evidence carrying a `Synthesized` key. What stays deferred is giving that evidence its own stored axis.
- crates.io publish of `hana_rigging`, then `bevy_clerestory 0.3.0`, then hana's dep bump — delivery only, gates no development (R4a).
- hana's [[restore primary window after reconnect]] Phase 1.
- The associated-type `Identified` trait (R7) — write it only when two implemented load paths share a batch-resolve-validate-publish body **with compatible failure policy** (T9's four conditions), then extract only the interface that body calls. See [[hana-identity-key-id-convention]].
- The does-not-ship column of *The v1 slice*: `Cohesion`, reassignment, `Unreachable` decay timing.

## Team review — cycle 1, auto-recorded

Four independent reviewers (sequencing/gates · type system · migration risk · coherence and runtime cost) read the whole document including R1–R9 and *Worktree Placement*. 22 raw findings, 17 after merging duplicates. **No reviewer challenged the premise** — none argued the crate should not be built.

The nine below have one correct outcome and change no intended behavior, so they are **accepted and recorded here** rather than asked about. The other eight are in *Proposed user decisions*.

**All nine are now applied to the sections above (2026-07-30).** What each one changed:

| # | Applied as |
|---|---|
| T1 | `Devices::armable()` requires `Reported` for **every** device; `restorable()` added as the separate weaker gate; R6's "any dangerous fixture" sentence corrected |
| T2 | catalyst migration restated as **29** files, in *Scope now* and in the branch-point section; G2 rewritten, clean-baseline gate added as G6 |
| T3 | *Executive summary*, *Core types*, *Sequencing*, *The v1 slice* build order, the cut line, and *Design status* reconciled; *Adversarial refutation* and *Reconciliation* marked historical; one authoritative build order; `0.2.0` → `0.3.0`; R1–R9 index added |
| T4 | G2 restated as an immutable pinned rev with a retirement condition, which became G7; G5 marked independent |
| T5 | camera half of *Migration proof* flagged for re-verification against the catalyst worktree; *Design status* Phase C qualified |
| T6 | known gaps 9 and 10 struck and corrected — identity is FNV-1a over EDID or the ColorSync UUID, not `CGDirectDisplayID` |
| T7 | the same-`AssetId` provider invariant promoted into *Conditions that must be built rather than assumed* |
| T8 | gates became a table with a verification command, owner, and what each blocks; six new gates |
| T9 | the `Identified` trait trigger sharpened to four conditions; "revisit when, not whether" deleted |

### T1 — R6 and R8 contradict each other on arming (critical)

R6 says `Devices::armable()` "can require `Reported` for any dangerous fixture" — conditional. R8 says a `Synthesized` match is **never** enough to arm — unconditional. No implementation can honor both, and `DeviceKind` carries no danger classification to branch on (adding one would break the membership rule). **R8 wins**, because it is the decision the user made: `armable()` requires `Reported` for every device, full stop. Restore gets its own separate predicate that accepts an unambiguous `Synthesized` key. R6's sentence and the `armable()` pseudocode are wrong as written and must be rewritten to match.

### T2 — The catalyst migration is bigger than R2 claims, and G2 describes a dependency that does not exist (critical)

Three corrections, all verified against the live tree:

- **Count.** R2 says "~20 call sites". Actual: **29 source and manifest files** on `init/hana_catalyst`. Re-inventory after the checkpoint below; do not carry the old number into the plan.
- **Not just imports.** `HardwareKey` is *serialized inside authored camera settings* (`hana_mimesis_tools/src/camera_source.rs:54`), is the key of the ownership table (`hana/src/hardware/ownership.rs:73`), and is carried alongside claim epochs that reject stale async open results. R6's nested enum is **not wire-compatible** with the flat key. So the migration needs a saved-settings conversion, not a find-and-replace — and the ownership epoch / stale-result tests stay in place until kernel equivalents pass.
- **G2 is factually wrong.** It says to re-point an existing `bevy_hana` dependency. Catalyst has no such dependency — its git deps point at `natepiano/hana.git`, and `bevy_clerestory` is still a crates.io dep. `hana_rigging` is a **new** dependency, not a re-point.

Prerequisite added: **the catalyst worktree must be committed clean before provider work starts.** It currently has 23 dirty paths, including both camera files the migration touches; three overlapping files alone hold 139 insertions and 453 deletions. Refactoring on top of that risks losing uncommitted work.

### T3 — Stale sections still prescribe the order and the API that R1–R9 replaced (important, 3-of-4 reviewer consensus)

The document reads as a design followed by a decision log, and the design half still contradicts the log. Every conflict found:

| section | says | should say |
|---|---|---|
| Executive summary | no kind enum; flat `DeviceKey{kind,scheme,value}` | `DeviceKind` + nested `DeviceIdSource` (R6) |
| Executive summary | clerestory "designed, not built"; screens migrate first | clerestory is implemented and is consumer #1 (R3) |
| `## Sequencing` | nothing implemented, nothing to extract, publish precedes clerestory | reconnect is merged; publish gates delivery only (R1, R4a) |
| old durable-key decision, `## Core types` | three `String`s, public fields, old constructors | R6's private nested representation |
| `## Core types` `ApplyProgress` | three variants | four — `Reconciliation` added terminal `Aborted` |
| `The v1 slice` build order + cut line | screens first, clerestory last, publish mid-delivery; recovery cut because publish blocks it | R3/R4a order; that cut reason is void |
| `0.2.0` at Sequencing, Minimum published surface, Build order, R4a, R4b | `0.2.0` | `0.3.0` — 0.2.0 shipped |
| `Design status` | camera proof complete, screens first | camera proof is stale (T5); clerestory first |
| closing sentence | "every open decision is resolved" | R9 is open |

**One authoritative build order**, replacing all others: kernel → clerestory → catalyst screens → catalyst cameras and key migration → integration test → Stream Deck → publish `hana_rigging` → publish clerestory `0.3.0`.

Reconcile all of it **before** compiling the phased plan; a compiler reading this document today can pick either branch. Keep `Adversarial refutation` and `Reconciliation` as explicitly historical review records, and strip the duplicated normative type definitions out of them. Reduce `Branch point and build sequence — decisions` to an R1–R9 index once its content is folded into the sections above. **Do not touch the `## Worktree Placement` heading or its labels** — a downstream command reads them by name.

### T4 — G2 needs an immutable revision and a correct retirement condition (important)

Cargo resolves `branch = "feature/rigging"` to **one commit** in `Cargo.lock` and does not follow later commits on that branch until `cargo update` runs; a rebase or force-push of a referenced commit breaks clean builds because the locked commit is no longer fetchable. Worse, catalyst already pins four monorepo crates at `rev = "df97b2b"` — if `hana_rigging` resolves at two different sources, Cargo builds **two incompatible `DeviceKey` types** that silently do not interoperate.

G2 becomes: pin an exact `rev`, commit the resulting `Cargo.lock`, never rebase or force-push a referenced commit, require the revision pushed and fetchable, and require every consumer of `DeviceKey` — including git-sourced `bevy_clerestory` — to resolve the *same* `hana_rigging` source. Retire G2 when the pinned commit is an ancestor of `origin/main`, catalyst points at a main commit containing it, and no feature-branch source remains in `Cargo.lock`. **G5 is not a precondition for retiring G2** — catalyst can re-pin before it lands — so G5 becomes independent.

### T5 — The camera half of *Migration proof* cites deleted code (important)

The screens citation is still exact: `screen/session.rs:295-299` still rejects a mismatched generation or a session no longer `Opening`. The camera citation is dead. `secondary.rs` is now a read-only compatibility projection with no installer at the cited lines, and current camera handling is:

- `OpenRequestId` + `SessionState::Opening` validation — `render/sessions.rs:280-286`
- opened-vs-requested `HardwareKey` verification — `render/sessions.rs:287-302`
- app-owned request-to-claim-token validation — `hana/src/hardware/camera.rs:294-312`
- settle by install or discard — `hana/src/hardware/camera.rs:386-409`

Main and secondary capture are **already unified**, camera keys already prefer AVFoundation identity with a fallback fingerprint, duplicate keys are already refused, and claims are already explicit. So the predicates are no longer byte-for-byte copies and several claimed migration wins already exist. Rebuild the camera half of the proof from the files above, keep the screens half, and state which checks move into the kernel versus stay provider-owned. Phase C is **not** complete for cameras.

### T6 — Known gaps 9 and 10 describe the pre-reconnect clerestory (important)

Gap 9 says the persisted monitor key is `CGDirectDisplayID`. It is not: clerestory persists an FNV‑1a fingerprint over ColorSync UUID bytes on macOS and serial-qualified EDID on Windows/X11. The numeric display id is only the input used to *fetch* the macOS UUID. Gap 10 says non-macOS identity is missing; in fact Windows maps `HMONITOR` through DisplayConfig to registry EDID, X11 reads RANDR EDID, Linux identifies fixed internal connectors, and only Wayland is anonymous. The `configuration/windows.rs` and `configuration/x11.rs` files deliver topology *events*, not identity evidence.

Replace gap 9 with: both monitor providers must derive the same durable key, and ColorSync provenance must be classified (see D1 — a serial-less panel makes its UUID port-derived). Replace gap 10 with a platform matrix — Windows and X11 cross-provider key equivalence, duplicate handling, configuration-change behavior, and Wayland's `Unverified` path.

### T7 — A valid texture handle can still render a permanently frozen image (important)

Migration risk 2's mitigation is right but its test is too weak. Today a screen session keeps the same `Handle<Image>`, reconnect writes new frames into that exact asset id, and the material holds a clone. After migration the material's clone keeps the old asset *alive and valid* even if provider cleanup drops its record and a returning rig allocates a second texture — so a test that only asserts "retirement did not invalidate the handle" passes while the panel shows a frozen picture forever, with no error anywhere.

Provider invariant: **once a feed publishes an image for a durable device key, every later session for that key writes to the same `AssetId<Image>` for the life of the application.** Keep capture-stream ownership separate from that retained texture record. The test must assert an unchanged asset id **and new pixel contents** after retire-then-reappear, including a resolution change and a case where the material holds the only outstanding clone.

### T8 — The gates need evidence, not just names (important)

R1–R9 rest on preconditions that no gate represents: that the branch point is clean, that flat-key serialization can be replaced without an adapter, that clerestory's private fallback phases survive the generic conversion, that release tooling handles path+version deps (no dry-run recorded), that hana already has the monorepo dependency route (it does not — T2), that work can overlap 23 dirty catalyst paths, that providers can recover proof provenance from their existing APIs (D1 shows two cases where they cannot), that duplicate demotion is order-independent, and that R9 is deferrable (D1 shows it is not, for cameras).

Turn G1–G5 into a table with, per gate: the verification command or test, the owner, the phase it blocks, and the recorded result. Add gates for the clerestory v3 state migration (D5), the catalyst saved-key migration (T2), the clean catalyst baseline (T2), the immutable dependency revision (T4), the publish dry-run, and the minimum R9 camera evidence (D1).

### T9 — The `Identified` revisit trigger does not match catalyst's real load path (minor)

R7's trigger says "when devices need the same load staging as catalyst." Catalyst does not have one durable/runtime pair matching the trait: `ToolId` resolves to an `Entity`, a stable definition string resolves to `ToolDefId`, `PortKey` resolves to `PortId`, and `GraphStaging` is a transactional graph-mutation API rather than generic load staging. `Graph::rebuild_runtime` does graph-specific definition, composite, jack, and dependency validation before atomic publication — none of it reusable for devices.

Sharpened trigger: **introduce the trait only once two implemented load paths share a generic operation** that holds deserialized durable references unpublished, resolves a whole batch against current registries, validates cross-references, and publishes atomically *with compatible failure policy*. Extract only the interface that generic function needs. While device absence stays a policy state and catalyst resolution failure aborts the load, the two paths stay concrete and the trait stays unwritten. Also drop "revisit when, not whether" — that presumes the trait before any generic consumer exists.

## Proposed user decisions

Eight findings change intended behavior, structure, or scope, so they are decisions rather than corrections. They are walked one at a time; answers get recorded here.

- **D1 — AVFoundation's camera id is attachment-derived, so R9 cannot stay deferred** (critical, two reviewers independently). **DECIDED** — see *D1 decided* below. This also decides R9's minimum, and R9 is no longer open.
- **D2 — `Reflect` defeats R7's no-serde rule; what replaces the compile error?** (critical). **DECIDED** — see *D2 decided* below. R7's compile-error guarantee is withdrawn.
- **D3 — `DeviceIdentity` duplicates `DeviceIdSource` and arming stays bypassable** (critical). **PROVISIONALLY ACCEPTED** — see *D3 decided* below, including the condition under which it may be simplified.
- **D4 — How much of clerestory's fallback state machine the kernel actually absorbs** (critical). Decision pending.
- **D5 — Clerestory's saved window state needs a v4 migration or it is discarded** (critical). Decision pending.
- **D6 — Validated newtypes for scheme, reported id, and digest** (important). Decision pending.
- **D7 — Let a provider report "nothing changed" without building a scan** (important). Decision pending.
- **D8 — Defer the `WrongUnit` identity variant** (minor). **Decided: kept and renamed — see `### D8 decided`.**

### D1 decided — collect every identifier the platform can give, cross-platform, and never use `Option` to say what is missing

The finding stands: for USB webcams the macOS "unique id" is a port location plus vendor/product code, so it changes when the camera moves ports and is *inherited* by a different camera plugged into the old port. The post-open verification passes anyway, so a saved camera panel can silently open the wrong physical camera. The same weakness exists on displays — a monitor that reports no serial gets a port-derived ColorSync UUID. **G4 is resolved. R9's minimum is decided here, so R9 is no longer an open decision.**

Four requirements, all binding:

**1. Cross-platform, with `cfg` confined to the smallest possible surface.** Serial recovery is not a macOS feature. Every platform has both halves of the same problem and the same tell:

| platform | where a real serial comes from | how you know it is port-derived instead |
|---|---|---|
| macOS | IOKit USB registry serial string | AVFoundation `uniqueID` is location + VID/PID |
| Windows | device instance id `USB\VID_x&PID_x\<serial>` | the third segment contains `&` — Windows substitutes a port path when the device reports no serial |
| Linux | udev/sysfs `ID_SERIAL_SHORT`, USB `serial` attribute | attribute absent; only the bus/port path exists |

So the shape is **one platform-free trait with one small `cfg` module per platform** that answers exactly two questions — *what serial does this unit report* and *what is its attachment path*. Nothing above that module is conditionally compiled: not `DeviceKey`, not the providers, not the kernel. A platform with no implementation returns the "cannot ask" answer (see requirement 4) rather than being absent from the build.

**2. Go the extra mile so a human cannot get it wrong.** hana will manage a lot of hardware, and the operator is not going to debug identity. That justifies the IOKit/SetupAPI/udev work up front rather than accepting the weak id. Where certainty is impossible, hana **accommodates** rather than guesses: it presents what it knows and asks, instead of binding to a probable match.

**3. Retain the whole evidence set, not just the winning value.** Every identifier the OS can supply is captured and kept on the device record: reported serial, OS-supplied unique id, attachment path *under whatever name that platform calls it*, vendor/product/model, and any synthesized digest. `DeviceKey` is **derived from** the strongest available evidence; the rest is not discarded. This gives three things at once — an **info panel** showing every identifier for any device, the material to adjudicate an ambiguous match, and the port path R9 needed, without inventing a separate mechanism for it.

**4. No `Option` in identity types — named enums that say what presence and absence mean.** `Option<Serial>` cannot distinguish *this unit exposes no serial* (permanent, a real property of the hardware) from *this platform cannot be asked* (a coverage gap to close). Those lead to different policy, so they are different named variants:

```rust
/// What the platform could learn about this unit's own serial number.
pub enum ReportedSerial {
    /// The unit reports a serial the platform treats as unique to it.
    Provided(SerialNumber),
    /// The unit exposes no serial; the platform substituted a port-derived
    /// value. Permanent for this hardware — not a bug to fix.
    NotExposedByUnit,
    /// This platform has no way to ask. A coverage gap, not a device property.
    PlatformCannotReport,
}
```

Every other evidence field follows the same rule. This is now a stated convention for the crate, and it applies to attachment path, capabilities, and the resolution verdict below.

**Consequences carried into the plan:**

- Camera keys are `Reported` **only** from a recovered unit serial. A port-derived value is never `Reported`; it becomes attachment evidence, and the key falls back to a descriptor-derived `Synthesized` value.
- The same reclassification applies to serial-less display UUIDs (T6's second check).
- Binding resolution returns a **verdict, not a boolean**: a reported-key mismatch is a *wrong unit even when the ports agree*; a port-only agreement is a *candidate* that restores nothing and arms nothing until a human confirms; a unique synthesized key may restore but never arm (R8 holds).
- New tests: port swap between two identical units, external capture device rebooted, and a unit that exposes no serial at all.
- Partly pre-decides **D6** (the evidence newtypes are now required, and each carries a named-enum presence type) and constrains **D3** (the verdict enum replaces boolean matching).

### D2 decided — runtime handles live in a side table, not on the entity; R7's compile-error guarantee is withdrawn

**The claim R7 rested on is false.** R7 said a `DeviceId` with no `Serialize` cannot reach a save file, because a serde-deriving component containing one would not compile. hana does not save through serde derives — it saves through Moonshine, which walks every reflected component and serializes it **structurally**, field by field, without requiring `Serialize` on the type. A private field is still reflected. So a reflected `DeviceIdentity::Discovered(DeviceId(7))` serializes cleanly with no compile error. And `DeviceId` must stay reflected, because that is how BRP sees anything.

**Decision: adopt catalyst's side-table pattern**, verified in `hana_catalyst/src/runtime_index.rs:29`. Catalyst does not use a non-serialized component — it keeps runtime handles off entities entirely:

- The entity carries only durable data. `Tool` (`identity.rs:26`) has `stable_id: ToolId`, `definition: String`, and settings. No runtime handle.
- `RuntimeIndex` is a plain struct owned by `Graph`, deriving `Clone, PartialEq, Eq, Debug, Default` and **nothing else** — no `Component`, no `Resource`, no `Reflect`, no serde. Its own doc: *"`Graph` excludes this index from serde and authored reflection."*

For rigging: **`DeviceId` lives in `Devices`**, alongside the key→state maps that already exist there. It goes on no component. This is chosen over a separate non-serialized component because of the strength of the guarantee:

| approach | why a handle cannot be saved | how it breaks |
|---|---|---|
| separate non-serialized component | it is on a save-path deny list | someone adds it to the saved set or edits the filter — no compile error |
| **side table in `Devices`** (chosen) | the save path walks components and there is no component holding it | it does not — there is no filter to misconfigure |

Cost accepted: a device entity cannot be queried in BRP to see its runtime handle; reading it requires a deliberate accessor. That cost is small because the **info panel from D1 needs none of it** — serials, port paths, and synthesized digests are durable evidence living on the entity. An operator never looks at a runtime handle.

Also required regardless:

- **Persist a projection, not live state** — a save-shaped record holding durable keys and policy only.
- **Static assertions** that `DeviceId` implements none of `Serialize`, `DeserializeOwned`, `FromReflect`, so the weaker protection cannot rot silently.
- **One integration test against hana's real `SaveWorld` configuration** asserting no runtime handle appears in the output and that load rebuilds handles from durable keys.
- **Withdraw the compile-error language from R7**, and correct `memory/project_hana_identity_convention.md`, which repeats it as "enforcement is a compile error, not a comment."

Rejected: `#[reflect(opaque, from_reflect = false)]`. It does close the hole, but BRP and Moonshine share the same reflection machinery, so the type cannot permit one and forbid the other — it would blind the inspector to exactly the values you inspect when a binding misbehaves.

Interacts with **D7**, which proposes folding the entity mapping into `DeviceState` — the same consolidation catalyst arrived at.

### D3 decided (provisional) — a post-reconciliation verdict plus authorization tokens

R6's proof-vs-hint label currently gates nothing. `Devices::armable()` inspects a status value and never looks at whether the key was `Reported` or `Synthesized`, and any caller can compare two `DeviceKey`s with `==` — an equality that carries no signal that the match was a guess. Separately, `DeviceIdentity` and `DeviceIdSource` describe overlapping facts and can contradict: `Discovered` asserts a reported unique id even over a synthesized key, `Authored` has no `DeviceIdSource` counterpart, and `Unverified` folds in absence that `Presence` already owns.

Adopted shape:

- **No type parameters on the public surface.** No `DeviceKey<Proven>`, no marker types, no sealed trait — they would thread type parameters through components, events, maps, and queries and improve no call site.
- **Ordinary `Eq`/`Hash` stays on `DeviceKey`**, used strictly for evidence grouping and duplicate detection. No non-reflexive `PartialEq` — that violates equality semantics and breaks `HashMap`.
- **`DeviceIdentity` is replaced by a verdict computed after reconciliation** — `Proven | RestoreOnly | Authored | Displaced | WrongUnit | Unverified(reason)`. It records the conclusion, not the raw evidence, so it cannot contradict the key. This is the same verdict shape D1 already committed to for binding resolution.
- **Reconciliation returns a crate-private `MatchStrength::{Proven, RestoreOnly, None, Ambiguous}`.** Scan multiplicity cannot be checked at compile time, but its *result* can be encoded once and carried.
- **Private `ArmAuthorization` / `RestoreAuthorization` tokens.** `Apply` construction and provider `apply` require one. Only `Proven` — or an `Authored` resolution a human explicitly accepted — mints the arm token; a unique `Synthesized` match mints only restore. Arming from a hint becomes unwritable rather than banned by convention.
- **No `PartialEq` on the verdict enum**, so `Unverified == Unverified` never becomes an attractive API. That is R8's never-self-match rule enforced by absence.

This is the machinery that makes **T1** true (`armable()` requires `Reported` for every device, not only dangerous ones).

**Provisional, with a stated escape condition.** Accepted on the understanding that token threading may prove unwieldy once real providers and real recovery scenarios exist. If it does, the part to reconsider is **token threading through every `apply` call**, not the verdict enum — the verdict is load-bearing for D1's resolution and costs nothing. A retreat position, if needed: keep the tokens at attempt construction only, where the arming decision is actually made, and let `apply` trust the attempt it was handed. Record any such change here rather than quietly loosening it in code.

### D4 decided — one typed provider trait with defaults, erased at registration into a reflected value

**Question:** are the activities clerestory performs on a window the same activities other hardware needs, and if so can one trait with associated types plus default implementations carry the shared part, with policy enums attached per device?

**Answer: yes for six of seven activities.** Taken from this design's own migration-proof sections:

| What clerestory does to a window | Generalizes? |
| --- | --- |
| Enumerate the monitors that exist right now | Yes — every provider scans and pushes a whole-set scan inward |
| Recognize one monitor durably across reboots and port swaps | Yes — the durable key, `Reported` vs `Synthesized` |
| Ask for a configuration: this monitor, this position, fullscreen | Yes — this **is** the associated `Parameters` type (N2) |
| Wait to see whether the request took effect, because the compositor may not honor it | Yes — `poll` plus `fulfillment` (N3) |
| Remember the configuration that was intentional, so a fallback cannot overwrite it | Yes — `Captured::{Writable, Frozen}` (N4) |
| React when the monitor vanishes and come back when it returns | Yes — the availability axes and attempts |
| Scale factor, logical vs physical pixels, work-area insets, the geometry tolerance | **No** — screen-specific; lives inside the screens provider's own `Parameters`, never in the kernel |

The non-generalizing row is exactly what the kernel membership test already excludes, so the split falls where the existing rule puts it. No new boundary is invented.

**Shape.** A provider author implements a trait with `type Parameters` and typed methods. `fulfillment` (N3) is **defaulted**: byte-equal observed-versus-requested means `AsRequested`, otherwise `StillConverging`. Simple devices inherit it; a camera overrides it so a 60 fps request satisfied by 59.94 counts as arrived, and a device that settles on something else returns `DeviceSubstituted`. Per-device behavior is the three policy enums (`RetryOn` N5, `OnAbort` N6, `OnSessionLoss` N7) alongside the existing `Retain`×`Act`, `Arming`, and `Claim` axes.

**Why `dyn` is required — and why the reason is not dispatch cost.** The kernel calls a provider a handful of times per reconcile, not per device per frame, so vtable cost is irrelevant and must not be used to justify contortions. A static registry (an enum of known providers) is nonetheless impossible: providers live in **other crates**. clerestory is provider #1, so `hana_rigging` would have to name it in a variant, inverting the dependency. Hence `Vec<Box<dyn DeviceProvider>>` stands.

**Object safety and the erasure.** An associated type cannot appear in the signature of a method reached through `dyn`, so `fn apply(&mut self, params: &Self::Parameters)` and `Box<dyn DeviceProvider>` cannot coexist. Registration resolves it: `add_device_provider` wraps the typed provider in an adapter — written once inside `hana_rigging` — that implements a separate object-safe trait. The provider author never sees the erased form; the kernel never sees a type it cannot know.

**`ProviderBlob(Vec<u8>)` is retired.** Serializing to bytes is only necessary for a value that leaves the process. This one does not: per D2 what gets persisted is durable keys plus policy, and authored intent ("fullscreen on the left projector") lives in the application's own saved components. `Captured` is only the kernel's live memory of what it last successfully applied, within one run. The erasure therefore keeps it a value:

```rust
/// The configuration a provider last successfully applied to one device.
///
/// The kernel stores this without interpreting it — knowing what a monitor or
/// camera configuration *is* would violate the membership test. Each variant
/// holds the provider's own `Parameters` erased to a reflected value, which the
/// provider recovers by downcast and BRP can display for debugging.
pub enum Captured {
    /// Safe to refresh from live readback.
    Writable(Box<dyn Reflect>),
    /// Absent or mid-attempt: live readback would poison the intent.
    Frozen(Box<dyn Reflect>),
}
```

`Box<dyn Reflect>` rather than `Box<dyn Any>`: the same erasure and the same absent serde tax, plus BRP can show a device's current configuration. Reflection blindness already cost a phantom regression during the asset-loading work, which settles the tradeoff. Consequence: `DeviceProvider::Parameters` carries a `Reflect` bound, satisfied by an ordinary derive. This does not conflict with D2 — D2's prohibition is on `DeviceId` being reflect-constructible, not on device configuration being inspectable.

**Doc edits this forces — applied.** The `Captured` definition and the provider contract in *Core types* / *Provider contract* are rewritten, the `ProviderBlob` newtype is gone, every normative mention is replaced, and `DeviceProvider::migrate` is struck as having nothing left to migrate. Remaining `ProviderBlob` occurrences are quotations inside dated records like this one.

### D5 decided — clerestory persisted state goes to v4 by conservative conversion with self-heal

Clerestory saves window layouts to a versioned RON file, currently **v3**. Each entry records the monitor two ways (`bevy_clerestory/src/persistence/window_state.rs:197-221`): `monitor_panel`, a fingerprint of the physical panel, and `monitor: usize`, an index used only when the fingerprint is absent or matches nothing live. The fingerprint is what survives a replug, a dock, or a driver update reordering displays — with only the index, a window anchored to the wrong panel lands some distance into the wrong screen and the next save records that as correct.

Moving identity to `DeviceKey` changes that field's type, so the format goes to **v4**.

**The conversion cannot be exact, for a reason worth recording.** The v3 fingerprint is a hash and does not record its own ingredients. Under D1 a display identifier is `Reported` only if the panel's own serial was part of it and `Synthesized` otherwise — a distinction the v3 file hashed away. No converter can classify a v3 fingerprint correctly.

**Decision: convert conservatively, then self-heal.**

1. Every v3 fingerprint becomes a `Synthesized` key carrying the same digest. Saved layouts keep working.
2. `Synthesized` cannot authorize automatic output (R8/T1), but it can drive restore — D1 already gave restore its own predicate accepting an unambiguous `Synthesized` key.
3. On the first successful live match, clerestory rewrites the entry with a properly classified key. Degraded classification therefore lasts exactly one launch.
4. v3's existing `Anonymous` case needs no conversion — no digest, falls back to the index, exactly as today.

Rejected: dropping the identity and relying on the index (loses replug protection on every previously-saved window until it is saved again), and accepting a reset (a visible regression for anyone with an arranged multi-projector layout).

The work is mechanical because `persistence/format.rs` documents the recipe in its module header: freeze the old structs, add the conversion, add an arm to `decode`, write only the newest version in `encode`, and add both a v4 round-trip test and an old-file load test.

**New gate for the T8 verification table:** a v3 file loads, restores its windows to the same physical monitors, and is rewritten as v4 with classified keys.

### D6 decided — three validated newtypes in the durable key; the digest becomes a `u64`

The durable key's identifying payload is currently bare `String`s: a `scheme` naming how the device was identified and a `value` holding the identifier. Three concepts with three different rules share one type.

| Concept | Rule | Type |
| --- | --- | --- |
| Scheme name | Must match **exactly** across providers. Load-bearing: retiring clerestory's 200 pt geometry tolerance depends on two providers enumerating one display merging into one device, and they merge by agreeing on the scheme. A case difference or typo silently yields two devices — the precise failure the retirement assumes cannot happen. | `SchemeName` — validated non-empty, lowercase ASCII and dashes, bounded length |
| Reported identifier | Whatever the platform said. Opaque, but it reaches a RON file and an info panel. | `ReportedId` — non-empty, bounded, no control characters |
| Digest | A hash. FNV-1a, i.e. **64 bits**, currently carried as a string: two heap allocations per key construction and text hashing on every lookup. | `Digest(u64)` — serialized as hex |

Each has a private field and a fallible constructor, so an invalid value cannot be built. This is also what closes the stringly-typed half of the performance finding: the allocations disappear as a **consequence** of making malformed digests unrepresentable, not as an optimization.

**Scheme names are registered at app-build time**, and registration rejects duplicates and unknown names — so a typo fails at startup instead of quietly producing a second device at runtime. `SchemeName` nonetheless stays a validated text newtype rather than a registry handle, because clerestory must read one back from a file where no registry exists.

**Cost, stated:** a `u64` digest fixes the width, so a provider later wanting a 128-bit hash forces a format change. Acceptable — R6 already chose FNV-1a, which is not cryptographic and does not need to be at single-digit device counts.

Rejected: bare strings validated only inside the key's own constructor. One function anyone bypasses by building the string elsewhere, and the digest stays two allocations.

### Type review gate — standing requirement

Accepted with the standing condition that **the types get reviewed as they are written, not only as designed here**. The phased plan must therefore surface each phase's new and changed types for review before that phase is considered done — the type surface is the reviewable artifact, not the diff. This is a checkpoint on implementation, not a re-opening of the decisions above.

### D7 decided — a provider may report "nothing new" without building a set; `DeviceScan` replaces `PresenceSnapshot`

Providers report presence by pushing the **whole set**, and the kernel derives arrivals and removals by comparing against what it holds. Whole-set rather than deltas is load-bearing — it is how a disappearance is noticed at all, and it is why a provider that loses track self-corrects on its next scan.

The defect is cadence, not shape. As written, the kernel asks every provider for its full set on every `RiggingSet::Reconcile`, i.e. every frame, so a provider is obliged to build an owned list — the list plus every key in it — 120 times a second even when it has not looked at the hardware. Nothing scans that fast and the shipped code does not pretend to: clerestory's monitor update runs every `Update` but returns before scanning unless a monitor entity or the display configuration changed; cameras enumerate every two seconds; a disconnected screen retries every three; the planned HID provider scans every ten. **The new contract was therefore more wasteful than the code it replaces, purely as an artifact of the method's shape.**

`Unchanged` is safe because it is a claim about the provider, not about the hardware: a camera provider between its two-second scans genuinely has no information, and the kernel holding its last known set is exactly today's behavior. A provider returning `Unchanged` while a device actually vanished is a provider bug of the same class as returning a stale set.

**Naming.** `DeviceScan::{Unchanged, Complete(DeviceSet)}`, replacing `PresenceSnapshot`, with the provider method renamed `snapshot()` → `scan()`. "Scan" is already this design's own verb — cameras enumerate, the HID provider scans, clerestory rescans on configuration change. `Complete` carries the rule a provider author must not get wrong: whole set, never a delta. `Presence` stays reserved for the existing per-device enum. Rejected: `PresenceSnapshot`, `DeviceSnapshot`, and `Snapshot` — **"snapshot" is struck from this design's vocabulary entirely** (see the terminology rule below); `DeviceCheck`, because its dominant variant would report that nothing was checked.

**Three related items from the same finding have one correct answer each and are recorded as consequences, not decisions:**

1. The entity moves **inside** `DeviceState` instead of a second `HashMap<DeviceKey, Entity>` — one map, one lookup.
2. `DeviceKey` resolves to `DeviceId` **once**, at reconciliation. Authorization, `armable`, attempts, and polling then use `DeviceId` plus the topology revision; durable-key hashing is confined to reconciliation, persistence, and rebinding.
3. **No global interning and no `Arc<str>`.** Catalyst interns definition names because evaluation compares them constantly; device identity has no comparable hot path once (2) holds. Interning now would be ceremony. If providers must retain a completed set across frames, `Arc<[DeviceRecord]>` is the smaller move — and only on evidence.

### Terminology — "snapshot" is struck

"Snapshot" says nothing about what the value is or when it was taken, and it had accumulated three different jobs in this document: the provider's report, the act of scanning, and the whole-set property. All 49 occurrences are replaced — **"scan"** for the act and the report, **"the whole set"** for the property. Applies to the code and to any future prose about this design.

### D8 decided — the variant ships in v1, renamed `Contradicted` → `WrongUnit`

**The situation, in plain terms.** A show is saved with one camera set up as the front-of-house feed. Next week the show is loaded, and hana must decide for each saved device whether it is actually connected. Sometimes the answer is neither yes nor no: last week the camera was in the left USB port; this week someone put a *different* camera in that same port. hana finds a camera in the right place whose serial is not the saved one.

Two possible behaviors. Treat it as close enough and hand the new camera the saved settings — the wrong camera now feeds front-of-house, and the next save records that as intended. Or say something specific: *there is a device in the right place and it is not yours*, bind nothing, and surface it. The second needs its own outcome, because reporting it as "your camera is missing" would be false — the port is not empty — and would hide the unit sitting in it.

**Decision: it ships in v1, exercised.** The review proposed deferring it on the grounds that only RDM-capable lighting hardware could produce it. That was true of the text as it stood and **D1 has since made it false**: D1 records that a reported-key mismatch is a wrong unit *even when the ports agree*, and names the v1 test — a port swap between two identical units. A USB camera swap produces it with no lighting hardware anywhere.

**Renamed `Contradicted` → `WrongUnit`** (10 sites). "Contradicted" describes evidence disagreeing; `WrongUnit` says what happened. The two lowercase prose uses of "contradicts"/"contradicted" that refer to document sections rather than to this variant are unchanged.

**Two stale claims corrected in place:** the v1 slice table row said *"ships as a type, no v1 producer — needs RDM to produce it"*, and the Phase F summary listed it among the producer-less types. Both now cite D1's port-swap test. The narrow-`Displaced` rationale survives as a second reason rather than the only one.

For completeness: nothing was at risk either way, since per N5 the verdict enum keeps `#[non_exhaustive]` (applications match on it), so adding the variant later would not have been a breaking change. It ships because v1 genuinely produces it.

## Naming decisions

An interstitial review taken during D4, because D4 adds one associated type, one method, and four policy enums to the provider trait and the vocabulary had to be settled first.

### N1 decided — hardware terminology inside, metaphor only where it is literally true

**The rule, already followed by this codebase:** `hana_lading` carries its metaphor in the crate name only — inside it is `StartupAssetSetPlugin`, `StartupPolicyPlugin`, `StartupLoadFailures`. `hana_diegetic` leans in because those UIs genuinely *are* diegetic. So: **lean in when the borrowed word is literally accurate for the thing; use plain hardware terms when it would be decorative.**

Applied to this crate, the metaphor is literally accurate for exactly one concept — `RigKey`, which line 72 defines as *"the **role**, not the hardware — 'the left projector'"*, outliving every unit that fills it. That is what a rig position is in the industry: the plot assigns positions, fixtures get swapped into them. Kept.

Everything else carrying `Rig` was decorative, and one case was wrong: `Devices` is `HashMap<DeviceKey, DeviceState>`, keyed by device and holding device state, so "rig" meant two different things in one crate — a role that outlives units, and a table of units.

| before | denotes | after |
|---|---|---|
| `DeviceProvider` | speaks to the OS for one hardware class | **`DeviceProvider`** — joins `DeviceKey`, `DeviceKind`, `DeviceRecord`, `DeviceIdentity`, `DeviceIdSource` |
| `Devices` | key → per-device state and entity | **`Devices`** |
| `DeviceState` | the kernel's per-device state | **`DeviceState`** |
| `add_device_provider` | registration | **`add_device_provider`** |
| `RigKey` | the role — "the left projector" | **`RigKey`**, unchanged — literally the industry term |
| `RiggingPlugin`, `RiggingSet::Reconcile` | the crate's plugin and schedule set | **unchanged** — Bevy convention names these after the crate |

Four renames. The rest of the design was already literal: `Arming` (lasers and pyro are genuinely armed), `Claim` (hidapi genuinely claims a device), `Slot`, `EndpointRef`, `Presence`, `Apply`, `ApplyProgress`. Rejected: a full stagecraft lean-in (`Rigger`, `Plot`, `Trim`, `Cue`), which would have forced decorative names onto five concepts to keep two accurate ones.

Apply these renames throughout when the stale sections are reconciled (T3), including in *Minimum published surface*.

### N2 decided — the associated type is `Parameters`, not `Config`

`DeviceProvider::Parameters` is what a provider asks the hardware for and reads back: window geometry and mode, camera format and frame rate, DMX values.

`Config` was rejected for a specific collision, not vagueness: **clerestory already uses "configuration" to mean monitor topology** (`configuration/windows.rs`, `configuration/x11.rs` are the display-arrangement listeners). Same subsystem, same word, different meaning.

`Parameters` is the show-control industry word — fixture parameters, DMX parameters, camera parameters — and it reads correctly in the comparison the method performs: requested parameters versus reported parameters. At use sites it is `P::Parameters`, scoped by the trait.

Also rejected:

- **`Preset`** — genuine vendor vocabulary (PTZ cameras and projectors ship presets), but it means *a stored set of values recalled later*. hana will plausibly want real presets; do not spend the word on "values currently requested."
- **`Settings`** — catalyst already uses it for tool settings in the same repo.

### N3 decided — `settle` becomes `poll`, `SettleStatus` becomes `ApplyProgress`, and the judgement is `fulfillment` returning a three-way `Fulfillment`

The old name `settle` named a mechanism rather than the question it answers. The contract (doc lines 505–552) is: `apply` **starts** work and returns immediately, then the method is *polled each reconcile until it resolves*. As written it returned `Pending | Settled | Failed(DeviceFault)`, plus the terminal `Aborted` that *Reconciliation* added. Nothing settles anything — it reports how an in-flight operation is going.

**What is being attempted is driving the device to the requested parameters** — not identifying it. Identification finished earlier, in reconciliation, and reports through D3's verdict. So the enum is named after the operation it reports on, and the three names tell one story:

```rust
fn apply(&mut self, …);                                  // starts it
fn poll(&mut self, world, attempt) -> ApplyProgress;     // reports on it
```

`ApplyProgress { Pending, Done, Failed(DeviceFault), Aborted, Substituted }`. Rejected: `ApplyProgress` (the two words overlap) and `DeviceIdentificationProgress` (points at the wrong subsystem).

`poll` also conflated two jobs — the I/O check and the judgement of whether what came back counts. D4 splits them. The judgement is pure, takes no `World`, and **returns a named three-way enum rather than a `bool`**:

```rust
/// Does what we observed amount to what we asked for?
fn fulfillment(&self, requested: &Self::Parameters, observed: &Self::Parameters)
    -> Fulfillment;

pub enum Fulfillment {
    /// Counts as arrived, even if not byte-equal.
    AsRequested,
    /// Not there yet; the device is still converging.
    StillConverging,
    /// The device settled on something else and will not change.
    DeviceSubstituted,
}
```

A noun accessor is used because `satisfies` promises a yes/no, so `satisfies(..) -> StillConverging` reads as a contradiction at the call site.

**The third variant is a behavioral gain, not a naming preference.** With a `bool`, a camera that negotiated 30 fps when 60 was requested, and a window position the OS refused, both collapse to `false` — which the kernel can only read as *not yet*, so it waits out the deadline and then reports a fault for an operation that already finished. That is the shape of the open camera capture-rate bug: `Closest(…,60)` does not take and nothing in the system can say so. `DeviceSubstituted` is where D1's info panel gets *"requested 60, device gave 30."*

Consequences: `ApplyProgress` gains a terminal `Substituted` outcome so a substitution is not reported as `Failed` — a substitution is a success with a different result. `poll` does I/O and reports progress; `fulfillment` is pure judgement with no world access; clerestory's fullscreen / OS-refused-position / X11 rules become `fulfillment` overrides rather than a private state machine. The design prose *"bounded attempt with provider-supplied arrival evidence"* becomes *"bounded attempt with provider-supplied evidence"* wherever it appears.

### N4 dissolved — no new type; `Captured::Frozen` already covers it

D4's most general behavior — *do not let a fallback configuration overwrite the remembered intentional one* — is already designed, at doc lines 494–506: `Captured::{Writable(ProviderBlob), Frozen(ProviderBlob)}`, with `Frozen` documented as *"Absent or mid-attempt: live readback would poison the intent"*, plus the rule *"Suppression is per key. A global pause … is a live bug in clerestory today and must not be carried across."*

Already a named enum, already per-device, and — importantly — a **kernel rule rather than a per-device policy**. Nobody configures it, because "overwrite my remembered setup with whatever the fallback did" is a setting with no good use. Confirmed as-is; D4's four policy enums drop to three.

### N5 decided — `RetryOn`, plain enum, breaking changes accepted

The first of the three surviving policy enums names **when a failed attempt is allowed to be tried again**.

```rust
pub enum RetryOn {
    /// The device set changed since the failed attempt. Cannot loop forever —
    /// no change, no retry.
    NewRevision,
    /// Fixed cadence, for failures that resolve without any device change.
    Interval(Duration),
}
```

Both variants are required, and revision-gating alone is insufficient: a camera held open by another application becomes free when that application quits. No device appears or disappears, so the topology revision never advances, so a revision-gated retry would never fire. That is the Elgato case in the open capture-rate bug. Conversely `Interval` alone would hammer a device whose failure is permanent until the set changes.

**`#[non_exhaustive]` is not used here, and a variant addition is accepted as a breaking change.** Three reasons:

1. `hana_rigging` starts at `0.1` and `bevy_clerestory` is pre-`0.3.0`. In `0.x` a minor bump *is* the breaking-change channel.
2. The breakage is the point. This design repeatedly chooses compile errors over convention — D3 mints private authorization tokens so arming from a hint cannot compile, T1 makes the arming rule structural. `#[non_exhaustive]` works against that: it forces a `_` arm downstream, and a `_` arm is exactly where a new variant is silently swallowed. Without the attribute the compiler hands over the list of sites to reconsider.
3. For `RetryOn` specifically it would be pure ceremony — downstream *constructs* the value and the kernel *matches* it, so no downstream match exists to break.

Cost, stated: when a variant is added, `hana_rigging` takes a minor bump, `bevy_clerestory` takes one too, and clerestory is published on crates.io, so any third-party consumer migrates.

**The attribute is kept only where a wildcard is semantically correct** — where a downstream integration genuinely should not have to handle variants outside its own domain. That is `DeviceKind` (a camera integration should not need a laser arm; `hana_hardware`'s `HardwareKind` already chose this, with a `compile_fail` doctest enforcing it) and the D3 identity verdict, which applications do match on. Everything downstream only constructs stays a plain enum.

### N6 decided — `OnAbort`, per-device, defaulting to `LeaveAsIs`

An attempt is in flight across ticks (`apply` starts, `poll` resolves), so the world can change underneath it. Three causes abandon one, and the device is left in whatever partial state it reached — a window moved to the new monitor but not yet fullscreen, a camera open but not at the requested rate.

**Two of the three causes decide themselves. These are kernel rules, not policy, and the policy must not be consulted for them:**

| Abort cause | Behavior | Why it is not a choice |
| --- | --- | --- |
| Claim lost | Never revert | We no longer have access to the device. Reverting is impossible, not undesirable. |
| Arming vetoed | Never revert | Reverting re-sends the pre-attempt configuration, which may have been *energized*. Undoing would re-energize a device whose interlock just opened. |
| Revision changed | Consult `OnAbort` | The device is still ours and still reachable. |

Only the third case is a genuine per-device choice, and the default follows from the cause: the device set just changed, so a fresh target is being computed within a tick or two. Putting the device back first is wasted work plus a visible intermediate state. Devices where a half-applied state is itself wrong — a window parked between two monitors — opt in.

```rust
/// What to do with a device when an attempt is abandoned and we can still
/// reach it.
///
/// Only consulted when the attempt was abandoned because the device set
/// changed. A lost claim makes reverting impossible and an arming veto makes it
/// unsafe, so the kernel decides both of those itself without reading this.
#[derive(Clone, Copy, Debug, Default)]
pub enum OnAbort {
    /// Leave the device in whatever partial state it reached and let the next
    /// reconcile drive it.
    ///
    /// Right for a device whose reconfiguration is slow or visible — a
    /// projector lamp takes tens of seconds — because the set change that
    /// abandoned this attempt is about to produce a new target anyway.
    #[default]
    LeaveAsIs,
    /// Re-apply the configuration captured before the attempt started.
    ///
    /// Right when a half-applied state is itself wrong: a window parked
    /// between two monitors is visibly broken, not merely stale.
    Revert,
}
```

`Revert` rather than `RollBack` — "roll back" is borrowed transaction vocabulary, and N1 keeps metaphor in the crate name. Not `Restore`, which this design already uses for loading a saved show (`RestoreOnly`, `RestoreAuthorization`).

### N7 decided — `OnSessionLoss`, per-device, defaulting to `Recreate`

For nearly every device, hana holds something local that represents its use of that device: a window on a monitor, a capture session on a camera, an open HID handle on a Stream Deck. That local thing can die while the device stays connected and reachable — the compositor destroys a window, macOS invalidates a capture session across sleep, a bus reset closes a handle.

A third, distinct situation: **not** device absence (the availability axes handle that) and **not** an abandoned attempt (N6 handles that). Nothing unplugged and nothing was in flight.

Default is to rebuild, because hana is show control and a black panel after a laptop wake is a failure, and the captured configuration is exactly what is needed to rebuild without consulting anyone. Two constraints on the rebuild path, recorded rather than left implicit:

- **It runs as an ordinary attempt**, so `RetryOn` (N5) gates it. A camera the OS has wedged dies again the instant it is reopened; without the gate, rebuilding is a spin.
- **It reuses the published image handle**, per the frozen-texture invariant (T7). Materials already bound to that asset survive the rebuild.

```rust
/// What to do when the local session for a still-present device dies on its
/// own — a window destroyed by the compositor, a capture session invalidated
/// across sleep, a HID handle closed by a bus reset.
///
/// Distinct from device absence, which the availability axes handle, and from
/// an abandoned attempt, which `OnAbort` handles. Nothing unplugged and
/// nothing was in flight.
#[derive(Clone, Copy, Debug, Default)]
pub enum OnSessionLoss {
    /// Open a new session and re-apply the captured configuration, without
    /// involving application code.
    ///
    /// A camera panel goes black for a frame after the laptop wakes and then
    /// repopulates. Runs as an ordinary attempt, so `RetryOn` gates it — a
    /// camera the OS has wedged will not be reopened in a loop. Reuses the
    /// published image handle, so bound materials survive the rebuild.
    #[default]
    Recreate,
    /// Report the loss and do nothing else, leaving the response to
    /// application code.
    ///
    /// For a device where the replacement is a decision, not a repeat — a
    /// window that should be placed differently the second time, or one the
    /// user should be asked about first.
    ReportOnly,
}
```

"Session" because this design and the shipped code already use it for both screens and cameras (`screen/session.rs`, `render/sessions.rs`); "handle" would be accurate for HID and wrong for a window. `ReportOnly` rather than reusing clerestory's `Recovery::ApplicationControlled` — same intent, but that name is already bound to the monitor-disappearance axis, and one name on two different axes reads as one mechanism.

**The three surviving D4 policy enums are now named and specified: `RetryOn` (N5), `OnAbort` (N6), `OnSessionLoss` (N7).**

### Doc comment standard — applies to every type and method in this design

Standing requirement, carried into the phased plan and binding on every delegate that writes this code.

Every public type, variant, field, and method carries the explanation that made the decision, not a restatement of its name. Specifically:

- **A type or method says what situation it exists for** and what goes wrong without it — `OnAbort`'s comment says an attempt is in flight across ticks and the world can change underneath it.
- **Every enum variant says what the system or the user would observe**, with the concrete device that motivates it — `LeaveAsIs` names the slow projector lamp, `Revert` names the window parked between monitors. A variant comment that only rephrases the variant name is a defect.
- **Where a kernel rule overrides a policy, the policy's own comment states the override** — `OnAbort` documents that it is not read after a lost claim or an arming veto, so nobody looking only at the type can conclude it governs all three causes.
- **Where a name was chosen against a plausible alternative, the rejected name and reason belong in the comment** when the alternative would mislead a reader — `ReportedSerial::NotExposedByUnit` says *"Permanent for this hardware — not a bug to fix"* for exactly this reason.
- **A `#[non_exhaustive]` attribute states why it is there** (per N5: downstream genuinely should not have to handle out-of-domain variants), and its absence needs no comment.

The test: a reader who has not read this document can answer, from the doc comments alone, why the type exists, what each choice makes the system do, and which choice fits their device.

## Design status

**Phase A (evidence) complete** — clerestory contract extraction, requirements sweep of all 374 issues (32 device classes, 37 abstraction breakers), camera/screens/lading prior art, direct-HID survey.

**Phase B (design) complete** — identity and durable designation, presence and scans, endpoints and cohesion, the two-axis availability policy, recovery and attempts, the provider contract, the ownership boundary and its membership test, the monitor round-trip proof, the claim axis, the capability model, and the Bevy shape. All three original open decisions are resolved, and a fourth that facts 5–8 opened.

**Phase C (migration proof) complete for monitors and screens; the camera half is pending re-verification (T5).** Monitors express on the core without loss; screens and cameras are proved against the code in *Migration proof*, which deletes two independently written copies of the same session machine and generation guard (the staleness predicate is byte-for-byte identical across `screen/session.rs:295-299` and `secondary.rs:342-346`), retires the 200 pt geometry tolerance, and inherits retry, TOCTOU verification, and typed contention for the primary camera. The acceptance test holds: none of the three gets worse.

The proof **changed two types**, which is the outcome to want from a proof rather than a ratification. `scheme` became a shared registry and devices may be co-reported, because two providers enumerate one display and without that the geometry tolerance relocates instead of dying. And `apply` became start-plus-polled-`poll`, because every existing open is backgrounded with a channel outcome and a blocking apply would mandate the main-thread stall `open_camera` is already criticised for. One refinement needed no type change: the started-but-silent watchdog splits into kernel-owned bounded attempt plus provider-supplied arrival evidence, because *has a frame arrived* fails the membership test.

**Phase D (adversarial refutation) complete** — see *Adversarial refutation*. Art-Net/RDM DMX, a laser with a safety interlock, and a self-describing BLE sensor were each expressed on the kernel as designed; 18 findings resulted, 7 forcing type changes. Two are structural. The laser showed the kernel re-energizing a beam with no veto anywhere in the design — fixed by an `Arming` axis and a single `Devices::armable()` gate that also pulls the orphaned `Claim::Contended` prose rule into code. The BLE sensor exposed that `DeviceRecord` carried no capability channel at all, so `DeviceArrived` was empty for **every** device and the capability model's own consumer example matched nothing — fixed by `DeviceRecord.capabilities`, inserted atomically with the entity. Six attacks were survived, including the `Retain`×`Act` decomposition under the laser. Known gaps 4 and 8 close outright; gap 1 is reclassified as blocked rather than open. Claims contradicted in earlier sections are reconciled in place.

**Remaining** — the v1 slice scope, and compilation into a phased plan.

**Phase E (reconciliation) complete** — see *Reconciliation*. The proof and the refutation have been read against each other line by line. Three of four crossings are clean; one is a real conflict neither could have found alone: `Arming` was specified as a pre-fire gate, and `apply`/`poll` put attempts in flight across ticks, so an interlock opening mid-attempt was not checked. Resolved by extending the existing `expected`/`revision` re-check to cover claim and arming on every settle poll, with a terminal `ApplyProgress::Aborted` that never auto-retries. Co-reported devices gained two conservative fold rules — most-restrictive-wins on the gate axes, union-with-disagreement-disarms on capabilities — both no-new-type. The `identified()` rename is contained to *Core types*, and the Essential bullet now reads *bounded attempt with provider-supplied arrival evidence*. No type from either result was withdrawn, and all four combined type changes compose.

**Phase F (v1 slice) complete** — see *The v1 slice*. Every type in the design is now marked ships-and-exercised, ships-with-no-producer, or does-not-ship, with the reason stated: a type ships without a v1 consumer only when adding it later would force an audit of call sites that already exist (`Arming`, `Retain::Declared`), and is cut when adding it later is purely additive (`Cohesion`, reassignment). `WrongUnit` was listed here as producer-less and **D8 corrected that** — D1's port-swap test produces it in v1. The build order puts **`bevy_clerestory` first as consumer #1, then the screens provider, and the Stream Deck late** (corrected by R3 — the section originally put screens first because clerestory was believed unimplemented). Screens have working code, a real platform id, and tests to port, so they are the fastest *provider* signal that the kernel shape is wrong, while the Stream Deck would conflate "is the kernel right" with "does our HID stack work". The first integration test sits before the Stream Deck and before publish. The publish chain was found to constrain much less than *Sequencing* claimed, and is now stated as a decision: **do not publish until two real consumers have used it.** Risk 1 — the co-report assumption the entire 200 pt retirement rests on — was **verified from source against hana's locked winit 0.30.13 and xcap 0.9.6**, and the residue moved to known gaps 9 and 10 — **both since corrected by T6**: identity is not `CGDirectDisplayID` at all but FNV-1a over EDID (Windows, X11) or the ColorSync UUID (macOS), so the open question is not integer persistence but whether **both providers mint the identical key on each platform**, plus classifying a serial-less panel's port-derived UUID per D1.

**Conditions that must be built rather than assumed**, carried forward into the plan: co-reported devices; the split lifetime between a retired device and a provider texture handle a material still binds — stated as a **provider invariant (T7): once a feed publishes an image for a durable device key, every later session for that key writes to the same `AssetId<Image>` for the life of the application**, with capture-stream ownership kept separate from that retained texture record, and a test asserting an **unchanged asset id together with new pixel contents** after retire-then-reappear, including a resolution change and a case where the material holds the only outstanding clone; the `Arming` axis and its mid-attempt re-check; and `DeviceRecord.capabilities` inserted atomically with the entity.

**Phase G (branch point, team review, decision walk) complete** — see *Branch point and build sequence*, *Team review — cycle 1*, *Proposed user decisions*, and *Naming decisions*. R1–R9 fix where the crate lives, what consumes it first, and what the publish chain gates; **R9 is closed by D1**, not deferred. D1–D8 settle identity evidence, where runtime handles live, the arming authorization, the provider trait and its erasure, the two data migrations, the durable-key newtypes, the "nothing new" scan, and the `WrongUnit` verdict. N1–N7 settle the vocabulary. T1–T9 have been reconciled into the sections above; the gates are now a table with a verification command for each row.

**Remaining before implementation:** compile this into a delegate-ready phased plan. The design is closed with three things the plan must carry forward:

1. **`### Doc comment standard`** — every type and method in the crate documents the situation it exists for, and every variant names the observable behavior plus the concrete device that motivates it.
2. **`### Type review gate`** — each phase surfaces its new and changed types for review before that phase closes.
3. **D3 is provisional.** If threading authorization tokens through every `apply` proves unwieldy against real providers, the retreat position is stated in D3: keep the tokens at `Apply` construction, where the arming decision is actually made. Record any such change in D3 rather than loosening it in code.

Every finding is folded in or recorded as a numbered known gap. The one deliberately unwritten thing is the `Identified` trait, whose four-condition trigger is stated in R7.
