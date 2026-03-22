use std::io::Write;

/// Length-prefixed variable integer encoding.
pub trait VarInt: Sized {
    fn read_from(bytes: &[u8]) -> Option<(Self, usize)>;
    fn write<W: Write>(&self, w: &mut W) -> std::io::Result<()>;
}

macro_rules! impl_varint {
    ($t:ty) => {
        impl VarInt for $t {
            fn read_from(bytes: &[u8]) -> Option<(Self, usize)> {
                if bytes.is_empty() {
                    return None;
                }

                let byte_count = bytes[0] as usize;
                if byte_count > size_of::<Self>() {
                    return None;
                }

                let mut buf = [0u8; size_of::<Self>()];
                buf[(size_of::<Self>() - byte_count)..].copy_from_slice(&bytes[1..1 + byte_count]);

                Some((Self::from_be_bytes(buf), byte_count + 1))
            }

            fn write<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
                let mut size = size_of::<Self>();
                let be = self.to_be_bytes();
                for byte in be {
                    if byte == 0 {
                        size -= 1;
                    } else {
                        break;
                    }
                }
                w.write_all(&[size as u8])?;
                w.write_all(&be[(size_of::<Self>() - size)..])?;
                Ok(())
            }
        }
    };
}

impl_varint!(u16);
impl_varint!(u32);
impl_varint!(u64);
impl_varint!(u128);

macro_rules! impl_varint_be {
    ($t:ty) => {
        impl VarInt for $t {
            fn read_from(bytes: &[u8]) -> Option<(Self, usize)> {
                if bytes.is_empty() {
                    return None;
                }

                let byte_count = bytes[0] as usize;
                if byte_count > size_of::<Self>() {
                    return None;
                }

                let mut buf = [0u8; size_of::<Self>()];
                buf[(size_of::<Self>() - byte_count)..].copy_from_slice(&bytes[1..1 + byte_count]);

                Some((Self::from_bytes(buf), byte_count + 1))
            }

            fn write<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
                let mut size = size_of::<Self>();
                let be = self.to_bytes();
                for byte in be {
                    if byte == 0 {
                        size -= 1;
                    } else {
                        break;
                    }
                }
                w.write_all(&[size as u8])?;
                w.write_all(&be[(size_of::<Self>() - size)..])?;
                Ok(())
            }
        }
    };
}

impl_varint_be!(zerocopy::big_endian::U16);
impl_varint_be!(zerocopy::big_endian::U32);
impl_varint_be!(zerocopy::big_endian::U64);
impl_varint_be!(zerocopy::big_endian::U128);

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn roundtrip_u128(value: u128) {
            let mut buf = Vec::new();
            value.write(&mut buf).unwrap();
            let (decoded, bytes_read) = u128::read_from(&buf).unwrap();
            prop_assert_eq!(decoded, value);
            prop_assert_eq!(bytes_read, buf.len());
        }

        #[test]
        fn truncation_detected_for_out_of_range(value in (u32::MAX as u64 + 1)..=u64::MAX) {
            let mut buf = Vec::new();
            value.write(&mut buf).unwrap();
            prop_assert!(u32::read_from(&buf).is_none());
        }

        #[test]
        fn truncation_succeeds_within_range(value in 0u64..=(u32::MAX as u64)) {
            let mut buf = Vec::new();
            value.write(&mut buf).unwrap();
            let (decoded, _) = u32::read_from(&buf).unwrap();
            prop_assert_eq!(decoded, value as u32);
        }

        #[test]
        fn serialized_bytes_preserve_lexical_order(x: u128, y: u128) {
            let mut buf_x = Vec::new();
            let mut buf_y = Vec::new();
            x.write(&mut buf_x).unwrap();
            y.write(&mut buf_y).unwrap();
            prop_assert_eq!(x.cmp(&y), buf_x.cmp(&buf_y));
        }
    }
}
