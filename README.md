# Tension Field

Tension Field is a `toybox`-based CLAP audio effect focused on slow elastic
motion for techno atmospheres, synth drones, and stretched tails.

## Concept

The effect uses a dual warp engine:

- Elastic Buffer: variable-speed micro time-warp with controllable grain vs continuity.
- Tension Warp: spectral drag with damping and harmonic smear tied to the same gesture.
- Space Stage: width + diffusion to fill stereo space while staying stable.

Signal flow:

`Input -> Pre-emphasis -> Elastic Buffer -> Tension Warp -> Space Stage -> Output`

## Main controls

- `Tension`: global stretch force.
- `Pull Rate`: gesture speed in Hz (0.02 to 2.0).
- `Pull Shape`: Linear, Rubber, Ratchet, Wave.
- `Hold`: suspends the current tension state.
- `Grain`: continuous tape-like to textured elastic grains.
- `Pitch Coupling`: how much pitch follows stretch velocity.
- `Width`: stereo decorrelation amount.
- `Diffusion`: short dense smear after the warp.
- `Air Damping`: pull-linked high-frequency damping.
- `Air Comp`: restores top-end when damping is active.
- `Pull Direction`: backward (left) to forward (right) pull mapping.
- `Elasticity`: viscous to springy behavior.
- `Pull`: momentary trigger for manual pull/release gestures.
- `Rebound`: release response after `Pull` is released.
- `Clean Dirty`: cleaner processing or added texture.
- `Feedback`: controlled post-warp feedback for sustained textures.

## Build

```bash
cargo test
cargo build --release
```
