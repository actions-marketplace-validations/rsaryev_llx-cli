use super::*;

impl Provider {
    /// How many of its models were decoded.
    pub(crate) fn decoded(&self) -> usize {
        let slots = self.models.values();
        slots.filter(|slot| slot.model.get().is_some()).count()
    }
}

impl PartialEq for Provider {
    /// The same when they say the same: the models compared as they read, not as they are written.
    fn eq(&self, other: &Self) -> bool {
        (&self.name, &self.env) == (&other.name, &other.env) && self.models().eq(other.models())
    }
}
