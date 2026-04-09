use std::env;
use std::error::Error;
use std::mem;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rvl::io::read_reference_dataset;
use rvl::{CodecMode, DepthDecoder, DepthEncoder, TrvlConfig};

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

fn dataset_path() -> PathBuf {
    env::var_os("RVL_RECORDED_DEPTH_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference_clip.bin")
        })
}

fn run_round_trip(
    backend_name: &str,
    mode: CodecMode,
    frames: &[Vec<u16>],
    frame_len: usize,
) -> Result<(), Box<dyn Error>> {
    let mut encoder = DepthEncoder::new(frame_len, mode);
    let mut decoder = DepthDecoder::new(frame_len, mode);
    let mut stats = EfficiencyStats::default();

    for (index, frame) in frames.iter().enumerate() {
        let encode_started = Instant::now();
        let encoded = encoder.encode(frame)?;
        let encode_nanos = encode_started.elapsed().as_nanos();

        let decode_started = Instant::now();
        let decoded = decoder.decode(&encoded)?;
        let decode_nanos = decode_started.elapsed().as_nanos();

        assert_eq!(decoded, *frame, "{backend_name} corrupted frame {index}");

        stats.record_frame(
            frame_len,
            encoded.payload().len(),
            encode_nanos,
            decode_nanos,
        );
    }

    eprintln!(
        "backend={backend_name} frames={} raw_bytes={} encoded_bytes={} compression_ratio={:.3} avg_encode_ms={:.3} avg_decode_ms={:.3}",
        stats.frames,
        stats.raw_bytes,
        stats.encoded_bytes,
        stats.compression_ratio(),
        stats.average_encode_ms(),
        stats.average_decode_ms(),
    );

    Ok(())
}

#[test]
fn recorded_depth_round_trip_is_lossless_and_reports_efficiency() -> Result<(), Box<dyn Error>> {
    let path = dataset_path();
    let clip = read_reference_dataset(&path)?;
    let frames = clip
        .frames()
        .map(|frame| frame.to_vec())
        .collect::<Vec<_>>();

    assert!(
        !frames.is_empty(),
        "dataset must contain at least one frame"
    );

    eprintln!(
        "dataset={} width={} height={} frames={}",
        path.display(),
        clip.width(),
        clip.height(),
        clip.frame_count(),
    );

    run_round_trip("rvl", CodecMode::Rvl, &frames, clip.frame_len())?;
    run_round_trip(
        "trvl",
        CodecMode::Trvl(TrvlConfig::lossless()),
        &frames,
        clip.frame_len(),
    )?;

    Ok(())
}
