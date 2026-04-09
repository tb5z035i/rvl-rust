# `rvl`

Rust implementation of the RVL depth codec and the temporal TRVL-style stream codec described by the reference C/C++ project at <https://github.com/hanseuljun/temporal-rvl>.

The crate is library-first and is intended to be embedded by other Rust projects. The default build has no mandatory dependencies beyond the Rust standard library. Public frame samples use `u16`, which matches native `Z16` depth streams such as RealSense output.

## What is included

- A pure Rust RVL encoder/decoder for single depth frames.
- A stateful TRVL encoder/decoder layered on top of RVL for depth streams.
- A shared `DepthEncoder` / `DepthDecoder` façade for selecting either backend.
- A loader for the reference binary dataset layout used by the upstream C++ examples.
- A vendored sample clip for recorded integration tests.
- An opt-in RealSense live integration test behind the `realsense` feature.

## Usage

```rust
use rvl::{CodecMode, DepthDecoder, DepthEncoder, TrvlConfig};

let frame = vec![0_u16, 1200, 1201, 0, 1188];
let mut encoder = DepthEncoder::new(frame.len(), CodecMode::Trvl(TrvlConfig::lossless()));
let mut decoder = DepthDecoder::new(frame.len(), CodecMode::Trvl(TrvlConfig::lossless()));

let encoded = encoder.encode(&frame)?;
let decoded = decoder.decode(&encoded)?;

assert_eq!(decoded, frame);
# Ok::<(), rvl::CodecError>(())
```

For direct frame-by-frame RVL use you can call the low-level module directly:

```rust
let frame = vec![0_u16, 1000, 1001, 0, 1100];
let payload = rvl::rvl::encode(&frame);
let decoded = rvl::rvl::decode(&payload, frame.len())?;

assert_eq!(decoded, frame);
# Ok::<(), rvl::CodecError>(())
```

## TRVL defaults

`TrvlConfig::lossless()` keeps the codec lossless by using:

- `change_threshold = 0`
- `invalidation_threshold = 0`
- `keyframe_interval = 30`

Raising the thresholds can improve temporal compression but makes TRVL intentionally lossy.

## Recorded dataset format

The recorded-depth test loader expects the same binary layout used by the reference C++ code:

1. Little-endian `i32` width
2. Little-endian `i32` height
3. Little-endian `i32` bytes-per-pixel
4. Repeated depth frames as tightly packed little-endian `u16` pixels

The repository ships a sample clip at `tests/fixtures/reference_clip.bin`. To run the recorded integration test against your own clip instead, point `RVL_RECORDED_DEPTH_PATH` at a file with that same format.

## Test commands

Run the full default test suite:

```bash
cargo test
```

Run the batched recorded-depth test with timing and compression output:

```bash
cargo test recorded_depth_round_trip_is_lossless_and_reports_efficiency -- --nocapture
```

Run the same test against an external recorded clip:

```bash
RVL_RECORDED_DEPTH_PATH=/path/to/depth.bin \
cargo test recorded_depth_round_trip_is_lossless_and_reports_efficiency -- --nocapture
```

Run the live RealSense test with a connected D435i/D435if:

```bash
cargo test --features realsense live_realsense_depth_stream_round_trip -- --ignored --nocapture
```

The live test requires:

- `librealsense2` installed on the host
- a connected RealSense D435i/D435if depth camera
- enough USB power/bandwidth for the chosen stream profile

When the RealSense installation lives in a nonstandard path, the crate tries common library locations such as `/usr/local/lib` automatically. You can override the detected library directory with `RVL_REALSENSE_LIBDIR=/path/to/libdir`.

Optional environment variables for the live test:

- `RVL_REALSENSE_WIDTH` default `640`
- `RVL_REALSENSE_HEIGHT` default `480`
- `RVL_REALSENSE_FPS` default `30`
- `RVL_REALSENSE_FRAMES` default `120`
- `RVL_REALSENSE_LIBDIR` optional override for the `librealsense2` library directory
