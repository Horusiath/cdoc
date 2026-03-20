use std::io::Write;

pub trait VarInt: Sized {
    fn read_from(bytes: &[u8]) -> Option<(Self, &[u8])>;
    fn write<W: Write>(&self, w: &mut W) -> std::io::Result<()>;
}

impl VarInt for u64 {
    fn read_from(bytes: &[u8]) -> Option<(Self, &[u8])> {
        todo!()
    }

    fn write<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        todo!()
    }
}