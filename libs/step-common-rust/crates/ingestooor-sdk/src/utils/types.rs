use std::ops::Deref;

#[derive(Clone)]
pub struct ImmutableVec<T>(Vec<T>);
impl<T> ImmutableVec<T> {
    pub fn new(vec: Vec<T>) -> Self {
        ImmutableVec(vec)
    }
}
impl<T> Deref for ImmutableVec<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
