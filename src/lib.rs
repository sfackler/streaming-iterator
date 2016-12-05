//! Streaming iterators.
//!
//! The iterator APIs in the Rust standard library do not allow elements to be yielded which borrow
//! from the iterator itself. That means, for example, that the `std::io::Lines` iterator must
//! allocate a new `String` for each line rather than reusing an internal buffer. The
//!`StreamingIterator` trait instead provides access to elements being iterated over only by
//! reference rather than by value.
//!
//! `StreamingIterator`s cannot be used in Rust `for` loops, but `while let` loops offer a similar
//! level of ergonomics:
//!
//! ```ignore
//! while let Some(item) = iter.next() {
//!     // work with item
//! }
//! ```
//!
//! While the standard `Iterator` trait's functionality is based off of the `next` method,
//! `StreamingIterator`'s functionality is based off of a pair of methods: `advance` and `get`. This
//! essentially splits the logic of `next` in half (in fact, `StreamingIterator`'s `next` method
//! does nothing but call `advance` followed by `get`).
//!
//! This is required because of Rust's lexical handling of borrows (more specifically a lack of
//! single entry, multiple exit borrows). If `StreamingIterator` was defined like `Iterator` with
//! just a required `next` method, operations like `filter` would be impossible to define.
#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(all(test, feature = "std"))]
extern crate core;

/// An interface for dealing with streaming iterators.
pub trait StreamingIterator {
    /// The type of the elements being iterated over.
    type Item: ?Sized;

    /// Advances the iterator to the next element.
    ///
    /// Iterators start just before the first element, so this should be called before `get`.
    ///
    /// The behavior of calling this method after the end of the iterator has been reached is
    /// unspecified.
    fn advance(&mut self);

    /// Returns a reference to the current element of the iterator.
    ///
    /// The behavior of calling this method before `advance` has been called is unspecified.
    fn get(&self) -> Option<&Self::Item>;

    /// Advances the iterator and returns the next value.
    ///
    /// The behavior of calling this method after the end of the iterator has been reached is
    /// unspecified.
    ///
    /// The default implementation simply calls `advance` followed by `get`.
    #[inline]
    fn next(&mut self) -> Option<&Self::Item> {
        self.advance();
        (*self).get()
    }

    /// Returns the bounds on the remaining length of the iterator.
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }

    /// Borrows an iterator, rather than consuming it.
    ///
    /// This is useful to allow the application of iterator adaptors while still retaining ownership
    /// of the original adaptor.
    #[inline]
    fn by_ref(&mut self) -> &mut Self {
        self
    }

    /// Consumes the iterator, counting the number of remaining elements and returning it.
    #[inline]
    fn count(mut self) -> usize
        where Self: Sized
    {
        let mut count = 0;
        while let Some(_) = self.next() {
            count += 1;
        }
        count
    }

    /// Creates an iterator which uses a closure to determine if an element should be yielded.
    #[inline]
    fn filter<F>(self, f: F) -> Filter<Self, F>
        where Self: Sized,
              F: FnMut(&Self::Item) -> bool
    {
        Filter {
            it: self,
            f: f,
        }
    }

    /// Creates an iterator which both filters and maps by applying a closure to elements.
    #[inline]
    fn filter_map<B, F>(self, f: F) -> FilterMap<Self, B, F>
        where Self: Sized,
              F: FnMut(&Self::Item) -> Option<B>
    {
        FilterMap {
            it: self,
            f: f,
            item: None,
        }
    }

    /// Creates an iterator which is "well behaved" at the beginning and end of iteration
    ///
    /// The behavior of calling `get` before iteration has been started, and of continuing to call
    /// `advance` after `get` has returned `None` is normally unspecified, but this guarantees that
    /// `get` will return `None` in both cases.
    #[inline]
    fn fuse(self) -> Fuse<Self>
        where Self: Sized
    {
        Fuse {
            it: self,
            state: FuseState::Start,
        }
    }

    /// Creates an iterator which transforms elements of this iterator by passing them to a closure.
    #[inline]
    fn map<B, F>(self, f: F) -> Map<Self, B, F>
        where Self: Sized,
              F: FnMut(&Self::Item) -> B
    {
        Map {
            it: self,
            f: f,
            item: None,
        }
    }

    /// Consumes the first `n` elements of the iterator, returning the next one.
    #[inline]
    fn nth(&mut self, n: usize) -> Option<&Self::Item> {
        for _ in 0..n {
            self.advance();
            if self.get().is_none() {
                return None;
            }
        }
        self.next()
    }

    /// Creates a normal, non-streaming, iterator with elements produced by calling `to_owned` on
    /// the elements of this iterator.
    ///
    /// Requires the `std` feature.
    #[cfg(feature = "std")]
    #[inline]
    fn owned(self) -> Owned<Self>
        where Self: Sized,
              Self::Item: ToOwned
    {
        Owned(self)
    }
}

impl<'a, I: ?Sized> StreamingIterator for &'a mut I
    where I: StreamingIterator
{
    type Item = I::Item;

    #[inline]
    fn advance(&mut self) {
        (**self).advance()
    }

    #[inline]
    fn get(&self) -> Option<&Self::Item> {
        (**self).get()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (**self).size_hint()
    }

    #[inline]
    fn next(&mut self) -> Option<&Self::Item> {
        (**self).next()
    }
}

/// Turns a normal, non-streaming iterator into a streaming iterator.
#[inline]
pub fn convert<I>(it: I) -> Convert<I>
    where I: Iterator
{
    Convert {
        it: it,
        item: None,
    }
}

/// A streaming iterator which yields elements from a normal, non-streaming, iterator.
#[derive(Clone)]
pub struct Convert<I>
    where I: Iterator
{
    it: I,
    item: Option<I::Item>,
}

impl<I> StreamingIterator for Convert<I>
    where I: Iterator
{
    type Item = I::Item;

    #[inline]
    fn advance(&mut self) {
        self.item = self.it.next();
    }

    #[inline]
    fn get(&self) -> Option<&I::Item> {
        self.item.as_ref()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.it.size_hint()
    }

    #[inline]
    fn count(self) -> usize {
        self.it.count()
    }
}

/// A streaming iterator which filters the elements of a streaming iterator with a predicate.
pub struct Filter<I, F> {
    it: I,
    f: F,
}

impl<I, F> StreamingIterator for Filter<I, F>
    where I: StreamingIterator,
          F: FnMut(&I::Item) -> bool
{
    type Item = I::Item;

    #[inline]
    fn advance(&mut self) {
        while let Some(i) = self.it.next() {
            if (self.f)(i) {
                break;
            }
        }
    }

    #[inline]
    fn get(&self) -> Option<&I::Item> {
        self.it.get()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.it.size_hint().1)
    }
}

/// An iterator which both filters and maps elements of a streaming iterator with a closure.
pub struct FilterMap<I, B, F> {
    it: I,
    f: F,
    item: Option<B>,
}

impl<I, B, F> StreamingIterator for FilterMap<I, B, F>
    where I: StreamingIterator,
          F: FnMut(&I::Item) -> Option<B>
{
    type Item = B;

    #[inline]
    fn advance(&mut self) {
        loop {
            match self.it.next() {
                Some(i) => {
                    if let Some(i) = (self.f)(i) {
                        self.item = Some(i);
                        break;
                    }
                }
                None => {
                    self.item = None;
                    break;
                }
            }
        }
    }

    #[inline]
    fn get(&self) -> Option<&B> {
        self.item.as_ref()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.it.size_hint().1)
    }
}

#[derive(Copy, Clone)]
enum FuseState {
    Start,
    Middle,
    End,
}

/// A streaming iterator which is well-defined before and after iteration.
#[derive(Clone)]
pub struct Fuse<I> {
    it: I,
    state: FuseState,
}

impl<I> StreamingIterator for Fuse<I>
    where I: StreamingIterator
{
    type Item = I::Item;

    #[inline]
    fn advance(&mut self) {
        match self.state {
            FuseState::Start => {
                self.it.advance();
                self.state = match self.it.get() {
                    Some(_) => FuseState::Middle,
                    None => FuseState::End,
                };
            }
            FuseState::Middle => {
                self.it.advance();
                if let None = self.it.get() {
                    self.state = FuseState::End;
                }
            }
            FuseState::End => {}
        }
    }

    #[inline]
    fn get(&self) -> Option<&I::Item> {
        match self.state {
            FuseState::Start | FuseState::End => None,
            FuseState::Middle => self.it.get(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.it.size_hint()
    }

    #[inline]
    fn next(&mut self) -> Option<&I::Item> {
        match self.state {
            FuseState::Start => {
                match self.it.next() {
                    Some(i) => {
                        self.state = FuseState::Middle;
                        Some(i)
                    }
                    None => {
                        self.state = FuseState::End;
                        None
                    }
                }
            }
            FuseState::Middle => {
                match self.it.next() {
                    Some(i) => Some(i),
                    None => {
                        self.state = FuseState::End;
                        None
                    }
                }
            }
            FuseState::End => None,
        }
    }

    #[inline]
    fn count(self) -> usize {
        match self.state {
            FuseState::Start | FuseState::Middle => self.it.count(),
            FuseState::End => 0,
        }
    }
}

/// A streaming iterator which transforms the elements of a streaming iterator.
pub struct Map<I, B, F> {
    it: I,
    f: F,
    item: Option<B>,
}

impl<I, B, F> StreamingIterator for Map<I, B, F>
    where I: StreamingIterator,
          F: FnMut(&I::Item) -> B
{
    type Item = B;

    #[inline]
    fn advance(&mut self) {
        self.item = self.it.next().map(&mut self.f);
    }

    #[inline]
    fn get(&self) -> Option<&B> {
        self.item.as_ref()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.it.size_hint()
    }
}

/// A normal, non-streaming, iterator which converts the elements of a streaming iterator into owned
/// versions.
///
/// Requires the `std` feature.
#[cfg(feature = "std")]
#[derive(Clone)]
pub struct Owned<I>(I);

#[cfg(feature = "std")]
impl<I> Iterator for Owned<I>
    where I: StreamingIterator,
          I::Item: Sized + ToOwned
{
    type Item = <I::Item as ToOwned>::Owned;

    #[inline]
    fn next(&mut self) -> Option<<I::Item as ToOwned>::Owned> {
        self.0.next().map(ToOwned::to_owned)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

#[cfg(test)]
mod test {
    use core::fmt::Debug;

    use super::*;

    fn test<I>(mut it: I, expected: &[I::Item])
        where I: StreamingIterator,
              I::Item: Sized + PartialEq + Debug,
    {
        for item in expected {
            it.advance();
            assert_eq!(it.get(), Some(item));
            assert_eq!(it.get(), Some(item));
        }
        it.advance();
        assert_eq!(it.get(), None);
        assert_eq!(it.get(), None);
    }

    #[test]
    fn test_convert() {
        let items = [0, 1];
        let it = convert(items.iter().cloned());
        test(it, &items);
    }

    #[test]
    fn count() {
        let items = [0, 1, 2, 3];
        let it = convert(items.iter());
        assert_eq!(it.count(), 4);
    }

    #[test]
    fn filter() {
        let items = [0, 1, 2, 3];
        let it = convert(items.iter().cloned()).filter(|x| x % 2 == 0);
        test(it, &[0, 2]);
    }

    #[test]
    fn fuse() {
        struct Flicker(i32);

        impl StreamingIterator for Flicker {
            type Item = i32;

            fn advance(&mut self) {
                self.0 += 1;
            }

            fn get(&self) -> Option<&i32> {
                if self.0 % 4 == 3 {
                    None
                } else {
                    Some(&self.0)
                }
            }
        }

        let mut it = Flicker(0).fuse();
        assert_eq!(it.get(), None);
        it.advance();
        assert_eq!(it.get(), Some(&1));
        assert_eq!(it.get(), Some(&1));
        it.advance();
        assert_eq!(it.get(), Some(&2));
        assert_eq!(it.get(), Some(&2));
        it.advance();
        assert_eq!(it.get(), None);
        assert_eq!(it.get(), None);
        it.advance();
        assert_eq!(it.get(), None);
        assert_eq!(it.get(), None);
    }

    #[test]
    fn map() {
        let items = [0, 1];
        let it = convert(items.iter().map(|&i| i as usize)).map(|&i| i as i32);
        test(it, &items);
    }

    #[test]
    fn nth() {
        let items = [0, 1];
        let it = convert(items.iter().cloned());
        assert_eq!(it.clone().nth(0), Some(&0));
        assert_eq!(it.clone().nth(1), Some(&1));
        assert_eq!(it.clone().nth(2), None);
    }

    #[test]
    fn filter_map() {
        let items = [0u8, 1, 1, 2, 4];
        let it = convert(items.iter())
            .filter_map(|&&i| {
                if i % 2 == 0 {
                    Some(i)
                } else {
                    None
                }
            });
        test(it, &[0, 2, 4])
    }

    #[test]
    #[cfg(feature = "std")]
    fn owned() {
        let items = [0, 1];
        let it = convert(items.iter().cloned()).owned();
        assert_eq!(it.collect::<Vec<_>>(), items);
    }
}
