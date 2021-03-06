use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::prelude::*;

use frontier::tier::Nested;

/// The frontier of the top level of some part of the commitment tree, which may be empty, but may
/// not be finalized or hashed.
#[derive(Derivative, Serialize, Deserialize)]
#[derivative(
    Debug(bound = "Item: Debug, Item::Complete: Debug"),
    Clone(bound = "Item: Clone, Item::Complete: Clone"),
    Default(bound = "")
)]
#[serde(bound(
    serialize = "Item: Serialize, Item::Complete: Serialize",
    deserialize = "Item: Deserialize<'de>, Item::Complete: Deserialize<'de>"
))]
pub struct Top<Item: Focus> {
    inner: Option<Nested<Item>>,
}

impl<Item: Focus> Top<Item> {
    /// Create a new top-level frontier tier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an item or its hash into this frontier tier.
    ///
    /// If the tier is full, return the input item without inserting it.
    #[inline]
    pub fn insert(&mut self, item: Item) -> Result<(), Item> {
        // Temporarily replace the inside with `None` (it will get put back right away, this is just
        // to satisfy the borrow checker)
        let inner = std::mem::take(&mut self.inner);

        let (result, inner) = if let Some(inner) = inner {
            if inner.is_full() {
                // Don't even try inserting when we know it will fail: this means that there is *no
                // implicit finalization* of the frontier, even when it is full
                (Err(item), inner)
            } else {
                // If it's not full, then insert the item into it (which we know will succeed)
                let inner = inner
                    .insert_owned(item)
                    .unwrap_or_else(|_| panic!("frontier is not full, so insert must succeed"));
                (Ok(()), inner)
            }
        } else {
            // If the tier was empty, create a new frontier containing only the inserted item
            let inner = Nested::new(item);
            (Ok(()), inner)
        };

        // Put the inner back
        self.inner = Some(inner);

        result
    }

    /// Update the currently focused `Item` (i.e. the most-recently-[`insert`](Self::insert)ed one),
    /// returning the result of the function.
    ///
    /// If this top-level tier is empty or the most recently inserted item is a hash, returns
    /// `None`.
    #[inline]
    pub fn update<T>(&mut self, f: impl FnOnce(&mut Item) -> T) -> Option<T> {
        self.inner.as_mut().and_then(|inner| inner.update(f))
    }

    /// Get a reference to the focused `Insert<Item>`, if there is one.
    ///
    /// If this top-level tier is empty or the focus is a hash, returns `None`.
    #[inline]
    pub fn focus(&self) -> Option<&Item> {
        if let Some(ref inner) = self.inner {
            inner.focus()
        } else {
            None
        }
    }

    /// Finalize the top tier into either a summary root hash or a complete tier.
    #[inline]
    pub fn finalize(self) -> Insert<complete::Top<Item::Complete>> {
        if let Some(inner) = self.inner {
            inner.finalize_owned().map(|inner| complete::Top { inner })
        } else {
            // The hash of an empty top-level tier is 1
            Insert::Hash(Hash::one())
        }
    }

    /// Check whether this top-level tier is full.
    #[inline]
    pub fn is_full(&self) -> bool {
        if let Some(ref inner) = self.inner {
            inner.is_full()
        } else {
            false
        }
    }

    /// Check whether this top-level tier is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_none()
    }
}

impl<Item: Focus> Height for Top<Item> {
    type Height = <Nested<Item> as Height>::Height;
}

impl<Item: Focus + GetPosition> GetPosition for Top<Item> {
    #[inline]
    fn position(&self) -> Option<u64> {
        if let Some(ref frontier) = self.inner {
            frontier.position()
        } else {
            Some(0)
        }
    }
}

impl<Item: Focus> GetHash for Top<Item> {
    #[inline]
    fn hash(&self) -> Hash {
        if let Some(ref inner) = self.inner {
            inner.hash()
        } else {
            Hash::zero()
        }
    }

    #[inline]
    fn cached_hash(&self) -> Option<Hash> {
        if let Some(ref inner) = self.inner {
            inner.cached_hash()
        } else {
            Some(Hash::zero())
        }
    }
}

impl<Item: Focus + Witness> Witness for Top<Item>
where
    Item::Complete: Witness<Item = Item::Item>,
{
    type Item = Item::Item;

    fn witness(&self, index: impl Into<u64>) -> Option<(AuthPath<Self>, Self::Item)> {
        if let Some(ref inner) = self.inner {
            inner.witness(index)
        } else {
            None
        }
    }
}

impl<Item: Focus + Forget> Forget for Top<Item>
where
    Item::Complete: ForgetOwned,
{
    fn forget(&mut self, index: impl Into<u64>) -> bool {
        if let Some(ref mut inner) = self.inner {
            inner.forget(index)
        } else {
            false
        }
    }
}
