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

The plugin includes a fixed-size performance editor (`1280x860`) with tabbed workflow:

- `Perform`: pull trigger, latch, tension map, and a 10-preset tension bank.
- `Rhythm`: time mode/division/rate plus swing, bias, rebound, and release snap.
- `Tone + Mod`: warp/character/space controls plus DSP mod-matrix source and route editing.
- `Safety + Out`: feedback, ducking, energy ceiling, output trim, and stage meters with peak hold.

The current `toybox` GUI backend is Windows-only, so GUI hosting is enabled on Windows builds and omitted on non-Windows targets. The modulation engine runs in DSP, so modulation remains active even when the GUI is closed.

## Live Tension Recipes

1. **Pre-drop coil**
- Set `Time Mode=Sync Div`, `Pull Division=1 Bar`, `Pull Latch=On`.
- Raise `Tension` to ~70%, `Tension Bias` to ~75%, `Release Snap` to ~60%.
- Trigger `Pull` one bar before the drop, then release at transition.

2. **Triplet panic push**
- Set `Pull Division=1/4T`, `Swing` around 20%, `Pull Quantize=1/16`.
- Use `Warp Motion` 55-65% and `Character=Dirty`.
- Drive repeated `Pull` hits for syncopated build pressure.

3. **Crush aftershock tail**
- Set `Character=Crush`, `Feedback` 30-40%, `Ducking` 30%+.
- Lower `Energy Ceiling` to ~50-60% for controlled aggression.
- Keep `Output Trim` near `-2 dB` while pushing `Tension` above 65%.

## Build

```bash
cargo test
cargo build --release
```
