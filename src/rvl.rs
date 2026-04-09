use crate::error::{CodecError, CodecResult};

pub fn encode(pixels: &[u16]) -> Vec<u8> {
    let samples = pixels
        .iter()
        .map(|&pixel| i32::from(pixel))
        .collect::<Vec<_>>();
    encode_signed(&samples)
}

pub fn decode(bytes: &[u8], num_pixels: usize) -> CodecResult<Vec<u16>> {
    decode_signed(bytes, num_pixels)?
        .into_iter()
        .map(|sample| {
            u16::try_from(sample).map_err(|_| CodecError::SampleOutOfRange { value: sample })
        })
        .collect()
}

pub(crate) fn encode_signed(samples: &[i32]) -> Vec<u8> {
    let mut writer = NibbleWriter::with_capacity(samples.len());
    let mut previous = 0_i32;
    let mut index = 0;

    while index < samples.len() {
        let zero_start = index;
        while index < samples.len() && samples[index] == 0 {
            index += 1;
        }
        writer.write_vle((index - zero_start) as u32);

        let nonzero_start = index;
        while index < samples.len() && samples[index] != 0 {
            index += 1;
        }
        writer.write_vle((index - nonzero_start) as u32);

        for &current in &samples[nonzero_start..index] {
            let delta = current - previous;
            writer.write_vle(zigzag_encode(delta));
            previous = current;
        }
    }

    writer.finish()
}

pub(crate) fn decode_signed(bytes: &[u8], num_pixels: usize) -> CodecResult<Vec<i32>> {
    if bytes.is_empty() && num_pixels == 0 {
        return Ok(Vec::new());
    }

    if bytes.len() % 4 != 0 {
        return Err(CodecError::InputNotWordAligned { len: bytes.len() });
    }

    let mut reader = NibbleReader::new(bytes);
    let mut previous = 0_i32;
    let mut output = Vec::with_capacity(num_pixels);

    while output.len() < num_pixels {
        let remaining = num_pixels - output.len();
        let zeros = reader.read_vle()? as usize;
        if zeros > remaining {
            return Err(CodecError::InvalidRunLength {
                zeros,
                nonzeros: 0,
                remaining_pixels: remaining,
            });
        }
        output.resize(output.len() + zeros, 0);

        let remaining = num_pixels - output.len();
        let nonzeros = reader.read_vle()? as usize;
        if nonzeros > remaining {
            return Err(CodecError::InvalidRunLength {
                zeros,
                nonzeros,
                remaining_pixels: remaining,
            });
        }

        for _ in 0..nonzeros {
            let delta = zigzag_decode(reader.read_vle()?);
            let current = previous + delta;
            output.push(current);
            previous = current;
        }
    }

    let trailing_bytes = reader.remaining_full_words();
    if trailing_bytes > 0 {
        return Err(CodecError::TrailingData {
            remaining_bytes: trailing_bytes,
        });
    }

    Ok(output)
}

fn zigzag_encode(value: i32) -> u32 {
    ((value << 1) ^ (value >> 31)) as u32
}

fn zigzag_decode(value: u32) -> i32 {
    ((value >> 1) as i32) ^ (-((value & 1) as i32))
}

struct NibbleWriter {
    bytes: Vec<u8>,
    word: u32,
    nibbles_written: u8,
}

impl NibbleWriter {
    fn with_capacity(pixel_count: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(pixel_count.max(4)),
            word: 0,
            nibbles_written: 0,
        }
    }

    fn write_vle(&mut self, mut value: u32) {
        loop {
            let mut nibble = value & 0x7;
            value >>= 3;
            if value != 0 {
                nibble |= 0x8;
            }

            self.word <<= 4;
            self.word |= nibble;
            self.nibbles_written += 1;

            if self.nibbles_written == 8 {
                self.bytes.extend_from_slice(&self.word.to_le_bytes());
                self.word = 0;
                self.nibbles_written = 0;
            }

            if value == 0 {
                break;
            }
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.nibbles_written > 0 {
            let shift = 4 * (8 - u32::from(self.nibbles_written));
            self.bytes
                .extend_from_slice(&(self.word << shift).to_le_bytes());
        }
        self.bytes
    }
}

struct NibbleReader<'a> {
    bytes: &'a [u8],
    byte_offset: usize,
    word: u32,
    nibbles_remaining: u8,
}

impl<'a> NibbleReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            byte_offset: 0,
            word: 0,
            nibbles_remaining: 0,
        }
    }

    fn read_vle(&mut self) -> CodecResult<u32> {
        let mut value = 0_u32;
        let mut shift = 0_u32;

        loop {
            let nibble = u32::from(self.next_nibble()?);
            value |= (nibble & 0x7) << shift;

            if (nibble & 0x8) == 0 {
                return Ok(value);
            }

            shift += 3;
            if shift >= 32 {
                return Err(CodecError::VariableLengthOverflow);
            }
        }
    }

    fn next_nibble(&mut self) -> CodecResult<u8> {
        if self.nibbles_remaining == 0 {
            let next_word = self
                .bytes
                .get(self.byte_offset..self.byte_offset + 4)
                .ok_or(CodecError::UnexpectedEndOfInput)?;
            self.word = u32::from_le_bytes(next_word.try_into().expect("4 byte chunk"));
            self.byte_offset += 4;
            self.nibbles_remaining = 8;
        }

        let nibble = ((self.word & 0xF000_0000) >> 28) as u8;
        self.word <<= 4;
        self.nibbles_remaining -= 1;
        Ok(nibble)
    }

    fn remaining_full_words(&self) -> usize {
        self.bytes.len().saturating_sub(self.byte_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};
    use crate::error::CodecError;

    fn assert_round_trip(pixels: &[u16]) {
        let encoded = encode(pixels);
        let decoded = decode(&encoded, pixels.len()).expect("decode succeeds");
        assert_eq!(decoded, pixels);
    }

    #[test]
    fn round_trips_empty_frame() {
        assert_round_trip(&[]);
    }

    #[test]
    fn round_trips_all_zero_frame() {
        assert_round_trip(&[0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn round_trips_mixed_frame() {
        assert_round_trip(&[0, 0, 900, 901, 0, 0, 905, 0, 920, 930, 0, 0, 0, 931]);
    }

    #[test]
    fn round_trips_descending_values_and_max_u16() {
        assert_round_trip(&[65_535, 65_000, 64_500, 0, 10, 9, 8, 0, 40_000]);
    }

    #[test]
    fn matches_reference_layout_for_single_pixel() {
        assert_eq!(encode(&[1]), vec![0x00, 0x00, 0x20, 0x01]);
    }

    #[test]
    fn rejects_misaligned_input() {
        let err = decode(&[1, 2, 3], 1).expect_err("misaligned bytes are rejected");
        assert_eq!(err, CodecError::InputNotWordAligned { len: 3 });
    }

    #[test]
    fn rejects_truncated_input() {
        let mut encoded = encode(&[0, 1, 2, 0, 3, 4, 5]);
        encoded.truncate(encoded.len() - 4);
        let err = decode(&encoded, 7).expect_err("truncated words are rejected");
        assert_eq!(err, CodecError::UnexpectedEndOfInput);
    }

    #[test]
    fn rejects_wrong_pixel_count() {
        let encoded = encode(&[0, 1, 2, 3, 0, 4]);
        let err = decode(&encoded, 5).expect_err("wrong pixel count is rejected");
        assert!(matches!(
            err,
            CodecError::InvalidRunLength { .. } | CodecError::TrailingData { .. }
        ));
    }

    #[test]
    fn signed_internal_path_round_trips_negative_deltas() {
        let samples = [0, 500, -200, 0, -10, 40];
        let encoded = super::encode_signed(&samples);
        let decoded = super::decode_signed(&encoded, samples.len()).expect("signed decode");
        assert_eq!(decoded, samples);
    }
}
