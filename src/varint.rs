use std::io::Write;

pub trait VarInt: Sized {
    fn read_from(bytes: &[u8]) -> Option<(Self, &[u8])>;
    fn write<W: Write>(&self, w: &mut W) -> std::io::Result<()>;
}