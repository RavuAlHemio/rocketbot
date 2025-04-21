//! Clone and own at the same time.

use std::borrow::Cow;


/// An extension trait that allows cloning a value and converting it into its owned variant.
pub trait ClownExt<T> {
    /// Return a cloned and owned version of this value.
    ///
    /// *Clowned* is derived from *clo*ned and *own*ed.
    fn clowned(self) -> T;
}

impl<'a, B: ToOwned + ?Sized> ClownExt<B::Owned> for &'a Cow<'a, B> {
    fn clowned(self) -> B::Owned {
        self.clone().into_owned()
    }
}

impl<'a, B: ToOwned + ?Sized> ClownExt<Option<B::Owned>> for Option<&'a Cow<'a, B>> {
    fn clowned(self) -> Option<B::Owned> {
        self
            .map(|val| val.clone().into_owned())
    }
}
