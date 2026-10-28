pub struct SimpleStack<T> {
    items: Vec<T>,
}

impl<T> Default for SimpleStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SimpleStack<T> {
    #[inline]
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    #[inline]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            items: Vec::with_capacity(cap),
        }
    }

    #[inline]
    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }

    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }

    #[inline]
    pub fn current(&self) -> Option<&T> {
        self.items.last()
    }

    #[inline]
    pub fn length(&self) -> usize {
        self.items.len()
    }

    #[inline]
    pub fn inner(&self) -> &[T] {
        &self.items
    }
}

impl<T: PartialEq> SimpleStack<T> {
    #[inline]
    pub fn exists(&self, item: &T) -> bool {
        self.items.iter().any(|i| i == item)
    }
}
