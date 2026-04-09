use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceDepthVideo {
    width: usize,
    height: usize,
    frames: Vec<Vec<u16>>,
}

impl ReferenceDepthVideo {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ReferenceDatasetError> {
        if bytes.len() < 12 {
            return Err(ReferenceDatasetError::HeaderTooShort {
                actual_bytes: bytes.len(),
            });
        }

        let width = i32::from_le_bytes(bytes[0..4].try_into().expect("header width"));
        let height = i32::from_le_bytes(bytes[4..8].try_into().expect("header height"));
        let bytes_per_pixel =
            i32::from_le_bytes(bytes[8..12].try_into().expect("header bytes_per_pixel"));

        if width <= 0 || height <= 0 {
            return Err(ReferenceDatasetError::InvalidDimensions { width, height });
        }

        if bytes_per_pixel != 2 {
            return Err(ReferenceDatasetError::InvalidBytesPerPixel {
                actual: bytes_per_pixel,
            });
        }

        let width = width as usize;
        let height = height as usize;
        let frame_len = width * height;
        let frame_bytes = frame_len * 2;
        let payload = &bytes[12..];

        if payload.is_empty() {
            return Err(ReferenceDatasetError::MissingFrameData);
        }

        if payload.len() % frame_bytes != 0 {
            return Err(ReferenceDatasetError::TruncatedFrameData {
                frame_bytes,
                actual_bytes: payload.len(),
            });
        }

        let mut frames = Vec::with_capacity(payload.len() / frame_bytes);
        for chunk in payload.chunks_exact(frame_bytes) {
            let frame = chunk
                .chunks_exact(2)
                .map(|pixel| u16::from_le_bytes(pixel.try_into().expect("2 byte pixel")))
                .collect();
            frames.push(frame);
        }

        Ok(Self {
            width,
            height,
            frames,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn frame_len(&self) -> usize {
        self.width * self.height
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn frames(&self) -> impl ExactSizeIterator<Item = &[u16]> {
        self.frames.iter().map(Vec::as_slice)
    }

    pub fn into_frames(self) -> Vec<Vec<u16>> {
        self.frames
    }
}

pub fn read_reference_dataset(
    path: impl AsRef<Path>,
) -> Result<ReferenceDepthVideo, ReferenceDatasetError> {
    let bytes = fs::read(path.as_ref())?;
    ReferenceDepthVideo::from_bytes(&bytes)
}

#[derive(Debug)]
pub enum ReferenceDatasetError {
    Io(std::io::Error),
    HeaderTooShort {
        actual_bytes: usize,
    },
    InvalidDimensions {
        width: i32,
        height: i32,
    },
    InvalidBytesPerPixel {
        actual: i32,
    },
    MissingFrameData,
    TruncatedFrameData {
        frame_bytes: usize,
        actual_bytes: usize,
    },
}

impl Display for ReferenceDatasetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::HeaderTooShort { actual_bytes } => {
                write!(
                    f,
                    "reference dataset header must be at least 12 bytes, got {actual_bytes}"
                )
            }
            Self::InvalidDimensions { width, height } => {
                write!(f, "invalid reference dataset dimensions: {width}x{height}")
            }
            Self::InvalidBytesPerPixel { actual } => {
                write!(
                    f,
                    "reference dataset must contain 16-bit depth pixels, got {actual} bytes per pixel"
                )
            }
            Self::MissingFrameData => f.write_str("reference dataset does not contain any frames"),
            Self::TruncatedFrameData {
                frame_bytes,
                actual_bytes,
            } => write!(
                f,
                "reference dataset payload is not a whole number of frames: frame_bytes={frame_bytes}, actual_bytes={actual_bytes}"
            ),
        }
    }
}

impl Error for ReferenceDatasetError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ReferenceDatasetError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use super::{ReferenceDatasetError, ReferenceDepthVideo};

    #[test]
    fn reads_reference_bytes() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2_i32.to_le_bytes());
        bytes.extend_from_slice(&1_i32.to_le_bytes());
        bytes.extend_from_slice(&2_i32.to_le_bytes());
        bytes.extend_from_slice(&10_u16.to_le_bytes());
        bytes.extend_from_slice(&65_535_u16.to_le_bytes());
        bytes.extend_from_slice(&30_u16.to_le_bytes());
        bytes.extend_from_slice(&40_u16.to_le_bytes());

        let clip = ReferenceDepthVideo::from_bytes(&bytes).expect("valid clip");
        assert_eq!(clip.width(), 2);
        assert_eq!(clip.height(), 1);
        assert_eq!(clip.frame_count(), 2);
        assert_eq!(
            clip.frames().collect::<Vec<_>>(),
            vec![&[10, 65_535][..], &[30, 40][..]]
        );
    }

    #[test]
    fn rejects_non_16_bit_pixels() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_i32.to_le_bytes());
        bytes.extend_from_slice(&1_i32.to_le_bytes());
        bytes.extend_from_slice(&4_i32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);

        let err = ReferenceDepthVideo::from_bytes(&bytes).expect_err("invalid bpp is rejected");
        assert_eq!(
            err.to_string(),
            ReferenceDatasetError::InvalidBytesPerPixel { actual: 4 }.to_string()
        );
    }

    #[test]
    fn rejects_truncated_payload() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2_i32.to_le_bytes());
        bytes.extend_from_slice(&2_i32.to_le_bytes());
        bytes.extend_from_slice(&2_i32.to_le_bytes());
        bytes.extend_from_slice(&10_u16.to_le_bytes());

        let err =
            ReferenceDepthVideo::from_bytes(&bytes).expect_err("truncated payload is rejected");
        assert!(matches!(
            err,
            ReferenceDatasetError::TruncatedFrameData { .. }
        ));
    }
}
