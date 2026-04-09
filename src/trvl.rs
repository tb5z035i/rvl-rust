use crate::codec::{CodecKind, EncodedFrame, FrameKind};
use crate::error::{CodecError, CodecResult};
use crate::rvl;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrvlConfig {
    pub change_threshold: u16,
    pub invalidation_threshold: usize,
    pub keyframe_interval: usize,
}

impl TrvlConfig {
    pub const fn new(
        change_threshold: u16,
        invalidation_threshold: usize,
        keyframe_interval: usize,
    ) -> Self {
        Self {
            change_threshold,
            invalidation_threshold,
            keyframe_interval,
        }
    }

    pub const fn lossless() -> Self {
        Self::new(0, 0, 30)
    }

    pub const fn with_change_threshold(mut self, change_threshold: u16) -> Self {
        self.change_threshold = change_threshold;
        self
    }

    pub const fn with_invalidation_threshold(mut self, invalidation_threshold: usize) -> Self {
        self.invalidation_threshold = invalidation_threshold;
        self
    }

    pub const fn with_keyframe_interval(mut self, keyframe_interval: usize) -> Self {
        self.keyframe_interval = keyframe_interval;
        self
    }
}

impl Default for TrvlConfig {
    fn default() -> Self {
        Self::lossless()
    }
}

#[derive(Debug, Clone)]
pub struct TrvlEncoder {
    frame_len: usize,
    config: TrvlConfig,
    pixels: Vec<PixelState>,
    frames_encoded: usize,
}

impl TrvlEncoder {
    pub fn new(frame_len: usize, config: TrvlConfig) -> Self {
        Self {
            frame_len,
            config,
            pixels: vec![PixelState::default(); frame_len],
            frames_encoded: 0,
        }
    }

    pub fn config(&self) -> TrvlConfig {
        self.config
    }

    pub fn reset(&mut self) {
        self.pixels.fill(PixelState::default());
        self.frames_encoded = 0;
    }

    pub fn encode(&mut self, pixels: &[u16]) -> CodecResult<EncodedFrame> {
        let frame_kind = if self.frames_encoded == 0
            || (self.config.keyframe_interval != 0
                && self.frames_encoded % self.config.keyframe_interval == 0)
        {
            FrameKind::Key
        } else {
            FrameKind::Delta
        };
        self.encode_with_kind(pixels, frame_kind)
    }

    pub fn encode_with_kind(
        &mut self,
        pixels: &[u16],
        frame_kind: FrameKind,
    ) -> CodecResult<EncodedFrame> {
        validate_frame_len(self.frame_len, pixels.len())?;

        let payload = match frame_kind {
            FrameKind::Key => self.encode_keyframe(pixels),
            FrameKind::Delta => self.encode_delta(pixels)?,
        };

        self.frames_encoded += 1;
        Ok(EncodedFrame::new(
            CodecKind::Trvl,
            frame_kind,
            self.frame_len,
            payload,
        ))
    }

    fn encode_keyframe(&mut self, pixels: &[u16]) -> Vec<u8> {
        for (state, &pixel) in self.pixels.iter_mut().zip(pixels.iter()) {
            state.value = pixel;
            state.invalid_count = usize::from(pixel == 0);
        }
        rvl::encode(pixels)
    }

    fn encode_delta(&mut self, pixels: &[u16]) -> CodecResult<Vec<u8>> {
        if self.frames_encoded == 0 {
            return Err(CodecError::MissingReferenceFrame);
        }

        let mut diffs = Vec::with_capacity(self.frame_len);
        for (state, &raw_value) in self.pixels.iter_mut().zip(pixels.iter()) {
            let previous = i32::from(state.value);
            update_pixel(
                state,
                raw_value,
                self.config.change_threshold,
                self.config.invalidation_threshold,
            );

            let current = i32::from(state.value);
            let diff = current - previous;
            diffs.push(diff);
        }

        Ok(rvl::encode_signed(&diffs))
    }
}

#[derive(Debug, Clone)]
pub struct TrvlDecoder {
    frame_len: usize,
    previous: Vec<u16>,
    has_reference: bool,
}

impl TrvlDecoder {
    pub fn new(frame_len: usize) -> Self {
        Self {
            frame_len,
            previous: vec![0; frame_len],
            has_reference: false,
        }
    }

    pub fn reset(&mut self) {
        self.previous.fill(0);
        self.has_reference = false;
    }

    pub fn decode(&mut self, frame: &EncodedFrame) -> CodecResult<Vec<u16>> {
        if frame.codec() != CodecKind::Trvl {
            return Err(CodecError::CodecMismatch {
                expected: "trvl",
                actual: frame.codec().as_str(),
            });
        }

        if frame.pixel_count() != self.frame_len {
            return Err(CodecError::PixelCountMismatch {
                expected: self.frame_len,
                actual: frame.pixel_count(),
            });
        }

        self.decode_payload(frame.payload(), frame.kind())
    }

    pub fn decode_payload(
        &mut self,
        payload: &[u8],
        frame_kind: FrameKind,
    ) -> CodecResult<Vec<u16>> {
        match frame_kind {
            FrameKind::Key => {
                self.previous = rvl::decode(payload, self.frame_len)?;
                self.has_reference = true;
                Ok(self.previous.clone())
            }
            FrameKind::Delta => {
                if !self.has_reference {
                    return Err(CodecError::MissingReferenceFrame);
                }

                let diffs = rvl::decode_signed(payload, self.frame_len)?;
                for (previous, diff) in self.previous.iter_mut().zip(diffs.iter()) {
                    let current = i32::from(*previous) + *diff;
                    *previous = u16::try_from(current)
                        .map_err(|_| CodecError::SampleOutOfRange { value: current })?;
                }
                Ok(self.previous.clone())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PixelState {
    value: u16,
    invalid_count: usize,
}

fn validate_frame_len(expected: usize, actual: usize) -> CodecResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(CodecError::FrameLengthMismatch { expected, actual })
    }
}

fn update_pixel(
    pixel: &mut PixelState,
    raw_value: u16,
    change_threshold: u16,
    invalidation_threshold: usize,
) {
    if pixel.value == 0 {
        if raw_value > 0 {
            pixel.value = raw_value;
        }
        return;
    }

    if raw_value == 0 {
        pixel.invalid_count += 1;
        if pixel.invalid_count >= invalidation_threshold {
            pixel.value = 0;
            pixel.invalid_count = 0;
        }
        return;
    }

    pixel.invalid_count = 0;
    if pixel.value.abs_diff(raw_value) > change_threshold {
        pixel.value = raw_value;
    }
}

#[cfg(test)]
mod tests {
    use super::{TrvlConfig, TrvlDecoder, TrvlEncoder};
    use crate::codec::{CodecKind, FrameKind};
    use crate::error::CodecError;

    #[test]
    fn lossless_mode_round_trips_a_sequence() {
        let frames = vec![
            vec![0, 0, 1000, 1100, 0, 1200],
            vec![0, 0, 1001, 1100, 0, 1201],
            vec![0, 0, 1002, 1110, 0, 1201],
            vec![0, 0, 0, 1110, 0, 1203],
            vec![65_535, 65_500, 0, 1110, 0, 1203],
        ];

        let config = TrvlConfig::lossless().with_keyframe_interval(2);
        let mut encoder = TrvlEncoder::new(frames[0].len(), config);
        let mut decoder = TrvlDecoder::new(frames[0].len());

        for (index, frame) in frames.iter().enumerate() {
            let encoded = encoder.encode(frame).expect("encode succeeds");
            let decoded = decoder.decode(&encoded).expect("decode succeeds");
            assert_eq!(decoded, *frame, "frame {index} changed");
        }
    }

    #[test]
    fn first_frame_is_keyframe_and_interval_repeats() {
        let config = TrvlConfig::lossless().with_keyframe_interval(2);
        let mut encoder = TrvlEncoder::new(2, config);

        let first = encoder.encode(&[1, 2]).expect("first frame");
        let second = encoder.encode(&[1, 3]).expect("second frame");
        let third = encoder.encode(&[1, 4]).expect("third frame");

        assert_eq!(first.codec(), CodecKind::Trvl);
        assert_eq!(first.kind(), FrameKind::Key);
        assert_eq!(second.kind(), FrameKind::Delta);
        assert_eq!(third.kind(), FrameKind::Key);
    }

    #[test]
    fn delta_requires_reference_frame() {
        let mut encoder = TrvlEncoder::new(2, TrvlConfig::lossless());
        let err = encoder
            .encode_with_kind(&[1, 2], FrameKind::Delta)
            .expect_err("delta before keyframe is rejected");
        assert_eq!(err, CodecError::MissingReferenceFrame);
    }

    #[test]
    fn lossy_threshold_can_freeze_small_changes() {
        let config = TrvlConfig::lossless()
            .with_change_threshold(5)
            .with_keyframe_interval(0);
        let mut encoder = TrvlEncoder::new(2, config);
        let mut decoder = TrvlDecoder::new(2);

        let first = encoder.encode(&[100, 200]).expect("keyframe");
        let second = encoder.encode(&[103, 198]).expect("delta");

        assert_eq!(
            decoder.decode(&first).expect("first decode"),
            vec![100, 200]
        );
        assert_eq!(
            decoder.decode(&second).expect("second decode"),
            vec![100, 200]
        );
    }
}
