#![cfg(feature = "realsense")]

use std::env;
use std::error::Error;
use std::io;
use std::mem;
use std::time::{Duration, Instant};

use realsense_rust::config::Config;
use realsense_rust::context::Context;
use realsense_rust::frame::{DepthFrame, PixelKind};
use realsense_rust::kind::{Rs2Format, Rs2StreamKind};
use realsense_rust::pipeline::InactivePipeline;
use rvl::{DepthDecoder, DepthEncoder, TrvlConfig};

#[derive(Debug, Default)]
struct EfficiencyStats {
    frames: usize,
    raw_bytes: usize,
    encoded_bytes: usize,
    encode_nanos: u128,
    decode_nanos: u128,
}

impl EfficiencyStats {
    fn record_frame(
        &mut self,
        frame_len: usize,
        encoded_bytes: usize,
        encode_nanos: u128,
        decode_nanos: u128,
    ) {
        self.frames += 1;
        self.raw_bytes += frame_len * mem::size_of::<u16>();
        self.encoded_bytes += encoded_bytes;
        self.encode_nanos += encode_nanos;
        self.decode_nanos += decode_nanos;
    }

    fn compression_ratio(&self) -> f64 {
        self.raw_bytes as f64 / self.encoded_bytes.max(1) as f64
    }

    fn average_encode_ms(&self) -> f64 {
        self.encode_nanos as f64 / self.frames.max(1) as f64 / 1_000_000.0
    }

    fn average_decode_ms(&self) -> f64 {
        self.decode_nanos as f64 / self.frames.max(1) as f64 / 1_000_000.0
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn depth_pixels(frame: &DepthFrame) -> Result<Vec<u16>, Box<dyn Error>> {
    frame
        .iter()
        .map(|pixel| match pixel {
            PixelKind::Z16 { depth } => Ok(*depth),
            other => Err(io::Error::other(format!(
                "unexpected depth pixel representation: {other:?}"
            ))
            .into()),
        })
        .collect()
}

fn run_backend(
    backend_name: &str,
    pixels: &[u16],
    encoder: &mut DepthEncoder,
    decoder: &mut DepthDecoder,
    stats: &mut EfficiencyStats,
    frame_index: usize,
) -> Result<(), Box<dyn Error>> {
    let encode_started = Instant::now();
    let encoded = encoder.encode(pixels)?;
    let encode_nanos = encode_started.elapsed().as_nanos();

    let decode_started = Instant::now();
    let decoded = decoder.decode(&encoded)?;
    let decode_nanos = decode_started.elapsed().as_nanos();

    assert_eq!(
        decoded, pixels,
        "{backend_name} corrupted live frame {frame_index}"
    );
    stats.record_frame(
        pixels.len(),
        encoded.payload().len(),
        encode_nanos,
        decode_nanos,
    );
    Ok(())
}

#[test]
#[ignore = "requires librealsense2 and a connected D435i/D435if depth camera"]
fn live_realsense_depth_stream_round_trip() -> Result<(), Box<dyn Error>> {
    let width = env_usize("RVL_REALSENSE_WIDTH", 640);
    let height = env_usize("RVL_REALSENSE_HEIGHT", 480);
    let fps = env_usize("RVL_REALSENSE_FPS", 30);
    let frame_limit = env_usize("RVL_REALSENSE_FRAMES", 120);
    let frame_len = width * height;

    let context = Context::new()?;
    let mut config = Config::new();
    config.enable_stream(
        Rs2StreamKind::Depth,
        None,
        width,
        height,
        Rs2Format::Z16,
        fps,
    )?;

    let inactive = InactivePipeline::try_from(&context)?;
    let mut pipeline = inactive.start(Some(config))?;

    let mut rvl_encoder = DepthEncoder::rvl(frame_len);
    let mut rvl_decoder = DepthDecoder::rvl(frame_len);
    let mut trvl_encoder = DepthEncoder::trvl(frame_len, TrvlConfig::lossless());
    let mut trvl_decoder = DepthDecoder::trvl(frame_len, TrvlConfig::lossless());
    let mut rvl_stats = EfficiencyStats::default();
    let mut trvl_stats = EfficiencyStats::default();

    for frame_index in 0..frame_limit {
        let composite = pipeline.wait(Some(Duration::from_secs(2)))?;
        let depth_frame = composite
            .frames_of_type::<DepthFrame>()
            .into_iter()
            .next()
            .ok_or_else(|| {
                io::Error::other(format!("missing depth frame at index {frame_index}"))
            })?;
        let pixels = depth_pixels(&depth_frame)?;

        run_backend(
            "rvl",
            &pixels,
            &mut rvl_encoder,
            &mut rvl_decoder,
            &mut rvl_stats,
            frame_index,
        )?;
        run_backend(
            "trvl",
            &pixels,
            &mut trvl_encoder,
            &mut trvl_decoder,
            &mut trvl_stats,
            frame_index,
        )?;
    }

    let _inactive = pipeline.stop();

    eprintln!(
        "live backend=rvl frames={} raw_bytes={} encoded_bytes={} compression_ratio={:.3} avg_encode_ms={:.3} avg_decode_ms={:.3}",
        rvl_stats.frames,
        rvl_stats.raw_bytes,
        rvl_stats.encoded_bytes,
        rvl_stats.compression_ratio(),
        rvl_stats.average_encode_ms(),
        rvl_stats.average_decode_ms(),
    );
    eprintln!(
        "live backend=trvl frames={} raw_bytes={} encoded_bytes={} compression_ratio={:.3} avg_encode_ms={:.3} avg_decode_ms={:.3}",
        trvl_stats.frames,
        trvl_stats.raw_bytes,
        trvl_stats.encoded_bytes,
        trvl_stats.compression_ratio(),
        trvl_stats.average_encode_ms(),
        trvl_stats.average_decode_ms(),
    );

    Ok(())
}
