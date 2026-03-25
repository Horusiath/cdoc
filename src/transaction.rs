use crate::mutation::Mutation;

#[derive(Debug)]
pub struct ReadOnlyTransaction {}

impl ReadOnlyTransaction {}

#[derive(Debug)]
pub struct ReadWriteTransaction {}

impl ReadWriteTransaction {
    /// Returns a globally unique peer identifier used by this transaction as an update origin.
    pub fn pid(&self) -> crate::PID {
        todo!()
    }

    /// Applies a mutation to an underlying database structure.
    pub fn apply(&mut self, mutation: Mutation) -> crate::Result<()> {
        todo!()
    }

    pub fn commit(self) -> crate::Result<()> {
        todo!()
    }

    pub fn abort(self) -> crate::Result<()> {
        todo!()
    }
}
