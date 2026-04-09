use std::error::Error;
use std::fmt::{self, Display, Formatter};

pub type CodecResult<T> = Result<T, CodecError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    FrameLengthMismatch {
        expected: usize,
        actual: usize,
    },
    PixelCountMismatch {
        expected: usize,
        actual: usize,
    },
    InputNotWordAligned {
        len: usize,
    },
    UnexpectedEndOfInput,
    InvalidRunLength {
        zeros: usize,
        nonzeros: usize,
        remaining_pixels: usize,
    },
    VariableLengthOverflow,
    SampleOutOfRange {
        value: i32,
    },
    TrailingData {
        remaining_bytes: usize,
    },
    CodecMismatch {
        expected: &'static str,
        actual: &'static str,
    },
    DeltaFrameNotSupported {
        codec: &'static str,
    },
    MissingReferenceFrame,
}

impl Display for CodecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameLengthMismatch { expected, actual } => {
                write!(
                    f,
                    "frame length mismatch: expected {expected} pixels, got {actual}"
                )
            }
            Self::PixelCountMismatch { expected, actual } => {
                write!(
                    f,
                    "pixel count mismatch: expected {expected} pixels, got {actual}"
                )
            }
            Self::InputNotWordAligned { len } => {
                write!(
                    f,
                    "RVL input length must be a multiple of 4 bytes, got {len}"
                )
            }
            Self::UnexpectedEndOfInput => f.write_str("unexpected end of encoded RVL input"),
            Self::InvalidRunLength {
                zeros,
                nonzeros,
                remaining_pixels,
            } => write!(
                f,
                "invalid RVL run length: zeros={zeros}, nonzeros={nonzeros}, remaining_pixels={remaining_pixels}"
            ),
            Self::VariableLengthOverflow => {
                f.write_str("variable-length integer exceeded the supported RVL range")
            }
            Self::SampleOutOfRange { value } => {
                write!(f, "sample value {value} is outside the supported u16 range")
            }
            Self::TrailingData { remaining_bytes } => {
                write!(
                    f,
                    "encoded frame has {remaining_bytes} unread trailing bytes"
                )
            }
            Self::CodecMismatch { expected, actual } => {
                write!(
                    f,
                    "codec mismatch: decoder expects {expected}, frame uses {actual}"
                )
            }
            Self::DeltaFrameNotSupported { codec } => {
                write!(f, "{codec} does not support delta frames")
            }
            Self::MissingReferenceFrame => {
                f.write_str("cannot process a delta frame before a keyframe")
            }
        }
    }
}

impl Error for CodecError {}
