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
                buf[(size_of::<Self>() - byte_count)..].copy_from_slice(&bytes[..byte_count]);

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
