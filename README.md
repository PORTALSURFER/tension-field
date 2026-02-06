# Tension Field

Tension Field is a `toybox`-based CLAP audio effect focused on slow elastic
motion for techno atmospheres, synth drones, and stretched tails.

## Concept

The effect uses a transport-aware dual warp engine:

- Elastic Buffer: variable-speed micro time-warp with controllable grain vs continuity.
- Tension Warp: spectral drag with damping and harmonic smear tied to the same gesture.
- Space Stage: width + diffusion to fill stereo space while staying stable.
- DSP Mod Matrix: audio-thread modulation sources routed to performance-critical parameters.
- Transport Clock: host-synced timing, quantized pull launches, and swing-aware phase warping.

Signal flow:

`Input -> Pre-emphasis -> Elastic Buffer -> Tension Warp -> Space Stage -> Output`

## Main controls

- `Tension`: global stretch force.
- `Time Mode`: free-Hz or host-synced divisions.
- `Pull Rate` / `Pull Division`: gesture speed in free or synced mode.
- `Swing`: synced timing groove offset.
- `Pull Shape`: Linear, Rubber, Ratchet, Wave, Pulse.
- `Pull Latch`: keeps pull active after trigger.
- `Pull Quantize`: delayed launch to note-grid boundaries.
- `Grain`: continuous tape-like to textured elastic grains.
- `Pitch Coupling`: how much pitch follows stretch velocity.
- `Warp Color`: Neutral, Dark Drag, Bright Shear.
- `Warp Motion`: movement depth for spectral drift.
- `Width`: stereo decorrelation amount.
- `Diffusion`: short dense smear after the warp.
- `Air Damping`: pull-linked high-frequency damping.
- `Air Comp`: restores top-end when damping is active.
- `Pull Direction`: backward to forward pull mapping.
- `Elasticity`: viscous to springy behavior.
- `Pull`: momentary trigger for manual pull/release gestures.
- `Rebound`: release response after pull release.
- `Character`: Clean, Dirty, Crush.
- `Feedback`: controlled post-warp feedback for sustained textures.
- `Ducking`: input-reactive feedback attenuation.
- `Output Trim`: post-space gain trim.
- `Mod Matrix`: two sources (`A`, `B`) with bipolar route depths to tension, direction, grain, width, warp motion, and feedback.

## Editor UI

The plugin includes a fixed-size performance editor (`1280x860`) with:

- Left panel: gesture controls plus four mode cards.
- Center panel: 2D Tension Map (`Pull Direction` x `Elasticity`) with live trace.
- Right panel: space/character controls and a macro-first slow mod bank.
- Bottom strip: detailed meters (input, Elastic, Warp, Space, feedback, output, tension activity).

The current `toybox` GUI backend is Windows-only, so GUI hosting is enabled on Windows builds and omitted on non-Windows targets. The modulation engine runs in DSP, so modulation remains active even when the GUI is closed.

## Build

```bash
cargo test
cargo build --release
```
