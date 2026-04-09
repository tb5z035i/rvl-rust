use crate::error::{CodecError, CodecResult};
use crate::rvl;
use crate::trvl::{TrvlConfig, TrvlDecoder, TrvlEncoder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecKind {
    Rvl,
    Trvl,
}

impl CodecKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rvl => "rvl",
            Self::Trvl => "trvl",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Key,
    Delta,
}

impl FrameKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Delta => "delta",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecMode {
    Rvl,
    Trvl(TrvlConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    codec: CodecKind,
    kind: FrameKind,
    pixel_count: usize,
    payload: Vec<u8>,
}

impl EncodedFrame {
    pub fn new(codec: CodecKind, kind: FrameKind, pixel_count: usize, payload: Vec<u8>) -> Self {
        Self {
            codec,
            kind,
            pixel_count,
            payload,
        }
    }

    pub const fn codec(&self) -> CodecKind {
        self.codec
    }

    pub const fn kind(&self) -> FrameKind {
        self.kind
    }

    pub const fn pixel_count(&self) -> usize {
        self.pixel_count
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn into_payload(self) -> Vec<u8> {
        self.payload
    }
}

#[derive(Debug, Clone)]
pub struct DepthEncoder {
    frame_len: usize,
    backend: EncoderBackend,
}

impl DepthEncoder {
    pub fn new(frame_len: usize, mode: CodecMode) -> Self {
        let backend = match mode {
            CodecMode::Rvl => EncoderBackend::Rvl,
            CodecMode::Trvl(config) => EncoderBackend::Trvl(TrvlEncoder::new(frame_len, config)),
        };
        Self { frame_len, backend }
    }

    pub fn rvl(frame_len: usize) -> Self {
        Self::new(frame_len, CodecMode::Rvl)
    }

    pub fn trvl(frame_len: usize, config: TrvlConfig) -> Self {
        Self::new(frame_len, CodecMode::Trvl(config))
    }

    pub fn frame_len(&self) -> usize {
        self.frame_len
    }

    pub fn codec(&self) -> CodecKind {
        match self.backend {
            EncoderBackend::Rvl => CodecKind::Rvl,
            EncoderBackend::Trvl(_) => CodecKind::Trvl,
        }
    }

    pub fn reset(&mut self) {
        if let EncoderBackend::Trvl(encoder) = &mut self.backend {
            encoder.reset();
        }
    }

    pub fn encode(&mut self, pixels: &[u16]) -> CodecResult<EncodedFrame> {
        if pixels.len() != self.frame_len {
            return Err(CodecError::FrameLengthMismatch {
                expected: self.frame_len,
                actual: pixels.len(),
            });
        }

        match &mut self.backend {
            EncoderBackend::Rvl => Ok(EncodedFrame::new(
                CodecKind::Rvl,
                FrameKind::Key,
                self.frame_len,
                rvl::encode(pixels),
            )),
            EncoderBackend::Trvl(encoder) => encoder.encode(pixels),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DepthDecoder {
    frame_len: usize,
    backend: DecoderBackend,
}

impl DepthDecoder {
    pub fn new(frame_len: usize, mode: CodecMode) -> Self {
        let backend = match mode {
            CodecMode::Rvl => DecoderBackend::Rvl,
            CodecMode::Trvl(_) => DecoderBackend::Trvl(TrvlDecoder::new(frame_len)),
        };
        Self { frame_len, backend }
    }

    pub fn rvl(frame_len: usize) -> Self {
        Self::new(frame_len, CodecMode::Rvl)
    }

    pub fn trvl(frame_len: usize, config: TrvlConfig) -> Self {
        Self::new(frame_len, CodecMode::Trvl(config))
    }

    pub fn frame_len(&self) -> usize {
        self.frame_len
    }

    pub fn codec(&self) -> CodecKind {
        match self.backend {
            DecoderBackend::Rvl => CodecKind::Rvl,
            DecoderBackend::Trvl(_) => CodecKind::Trvl,
        }
    }

    pub fn reset(&mut self) {
        if let DecoderBackend::Trvl(decoder) = &mut self.backend {
            decoder.reset();
        }
    }

    pub fn decode(&mut self, frame: &EncodedFrame) -> CodecResult<Vec<u16>> {
        if frame.pixel_count() != self.frame_len {
            return Err(CodecError::PixelCountMismatch {
                expected: self.frame_len,
                actual: frame.pixel_count(),
            });
        }

        match (&mut self.backend, frame.codec()) {
            (DecoderBackend::Rvl, CodecKind::Rvl) => {
                if frame.kind() != FrameKind::Key {
                    return Err(CodecError::DeltaFrameNotSupported { codec: "rvl" });
                }
                rvl::decode(frame.payload(), self.frame_len)
            }
            (DecoderBackend::Trvl(decoder), CodecKind::Trvl) => decoder.decode(frame),
            (DecoderBackend::Rvl, actual) => Err(CodecError::CodecMismatch {
                expected: "rvl",
                actual: actual.as_str(),
            }),
            (DecoderBackend::Trvl(_), actual) => Err(CodecError::CodecMismatch {
                expected: "trvl",
                actual: actual.as_str(),
            }),
        }
    }
}

#[derive(Debug, Clone)]
enum EncoderBackend {
    Rvl,
    Trvl(TrvlEncoder),
}

#[derive(Debug, Clone)]
enum DecoderBackend {
    Rvl,
    Trvl(TrvlDecoder),
}

#[cfg(test)]
mod tests {
    use super::{CodecKind, CodecMode, DepthDecoder, DepthEncoder, FrameKind};
    use crate::error::CodecError;
    use crate::trvl::TrvlConfig;

    #[test]
    fn rvl_facade_round_trips() {
        let mut encoder = DepthEncoder::new(5, CodecMode::Rvl);
        let mut decoder = DepthDecoder::new(5, CodecMode::Rvl);
        let frame = vec![0, 100, 101, 0, 65_535];

        let encoded = encoder.encode(&frame).expect("encode succeeds");
        assert_eq!(encoded.codec(), CodecKind::Rvl);
        assert_eq!(encoded.kind(), FrameKind::Key);
        assert_eq!(decoder.decode(&encoded).expect("decode succeeds"), frame);
    }

    #[test]
    fn trvl_facade_round_trips() {
        let config = TrvlConfig::lossless().with_keyframe_interval(2);
        let mut encoder = DepthEncoder::new(3, CodecMode::Trvl(config));
        let mut decoder = DepthDecoder::new(3, CodecMode::Trvl(config));

        let first = encoder.encode(&[10, 20, 65_535]).expect("first frame");
        let second = encoder.encode(&[10, 21, 65_534]).expect("second frame");

        assert_eq!(
            decoder.decode(&first).expect("first decode"),
            vec![10, 20, 65_535]
        );
        assert_eq!(
            decoder.decode(&second).expect("second decode"),
            vec![10, 21, 65_534]
        );
    }

    #[test]
    fn rejects_codec_mismatch() {
        let mut encoder = DepthEncoder::rvl(2);
        let mut decoder = DepthDecoder::trvl(2, TrvlConfig::lossless());
        let frame = encoder.encode(&[1, 2]).expect("encode succeeds");

        let err = decoder.decode(&frame).expect_err("mismatch is rejected");
        assert_eq!(
            err,
            CodecError::CodecMismatch {
                expected: "trvl",
                actual: "rvl"
            }
        );
    }
}
