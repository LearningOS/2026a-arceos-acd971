//! # The ArceOS Standard Library
//!
//! The [ArceOS] Standard Library is a mini-std library, with an interface similar
//! to rust [std], but calling the functions directly in ArceOS modules, instead
//! of using libc and system calls.
//!
//! These features are exactly the same as those in [axfeat], they are used to
//! provide users with the selection of features in axfeat, without import
//! [axfeat] additionally:
//!
//! ## Cargo Features
//!
//! - CPU
//!     - `smp`: Enable SMP (symmetric multiprocessing) support.
//!     - `fp_simd`: Enable floating point and SIMD support.
//! - Interrupts:
//!     - `irq`: Enable interrupt handling support.
//! - Memory
//!     - `alloc`: Enable dynamic memory allocation.
//!     - `alloc-tlsf`: Use the TLSF allocator.
//!     - `alloc-slab`: Use the slab allocator.
//!     - `alloc-buddy`: Use the buddy system allocator.
//!     - `paging`: Enable page table manipulation.
//!     - `tls`: Enable thread-local storage.
//! - Task management
//!     - `multitask`: Enable multi-threading support.
//!     - `sched_fifo`: Use the FIFO cooperative scheduler.
//!     - `sched_rr`: Use the Round-robin preemptive scheduler.
//!     - `sched_cfs`: Use the Completely Fair Scheduler (CFS) preemptive scheduler.
//! - Upperlayer stacks
//!     - `fs`: Enable file system support.
//!     - `myfs`: Allow users to define their custom filesystems to override the default.
//!     - `net`: Enable networking support.
//!     - `dns`: Enable DNS lookup support.
//!     - `display`: Enable graphics support.
//! - Device drivers
//!     - `bus-mmio`: Use device tree to probe all MMIO devices.
//!     - `bus-pci`: Use PCI bus to probe all PCI devices.
//!     - `driver-ramdisk`: Use the RAM disk to emulate the block device.
//!     - `driver-ixgbe`: Enable the Intel 82599 10Gbit NIC driver.
//!     - `driver-bcm2835-sdhci`: Enable the BCM2835 SDHCI driver (Raspberry Pi SD card).
//! - Logging
//!     - `log-level-off`: Disable all logging.
//!     - `log-level-error`, `log-level-warn`, `log-level-info`, `log-level-debug`,
//!       `log-level-trace`: Keep logging only at the specified level or higher.
//!
//! [ArceOS]: https://github.com/arceos-org/arceos

#![cfg_attr(all(not(test), not(doc)), no_std)]
#![feature(doc_cfg)]
#![feature(doc_auto_cfg)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
#[doc(no_inline)]
pub use alloc::{boxed, format, string, vec};

#[cfg(feature = "alloc")]
pub mod collections {
    //! Collection types, similar to [`std::collections`](https://doc.rust-lang.org/std/collections/).
    //!
    //! Map/Set types are provided by the [hashbrown] crate, and the others
    //! are re-exported from the `alloc` crate.
    //!
    //! [hashbrown]: https://crates.io/crates/hashbrown
    pub use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};

    use core::borrow::Borrow;
    use core::hash::{BuildHasherDefault, Hash, Hasher};

    /// A deterministic FNV-1a hasher, used as the default hasher of
    /// [`HashMap`] and [`HashSet`] since `std::collections::RandomState`
    /// is unavailable in `no_std` environments.
    #[derive(Default, Clone)]
    pub struct FnvHasher(u64);

    impl Hasher for FnvHasher {
        #[inline]
        fn finish(&self) -> u64 {
            self.0
        }
        #[inline]
        fn write(&mut self, bytes: &[u8]) {
            // FNV-1a 64-bit
            for &byte in bytes {
                self.0 ^= byte as u64;
                self.0 = self.0.wrapping_mul(0x100_0000_01b3);
            }
        }
    }

    pub type HashBuilder = BuildHasherDefault<FnvHasher>;

    /// A hash map implemented with open addressing, backed by the
    /// [hashbrown](https://crates.io/crates/hashbrown) crate.
    pub struct HashMap<K, V>(hashbrown::HashMap<K, V, HashBuilder>);

    impl<K, V> HashMap<K, V> {
        /// Creates an empty `HashMap`.
        pub fn new() -> Self {
            Self(hashbrown::HashMap::with_hasher(HashBuilder::default()))
        }

        /// Creates an empty `HashMap` with at least the specified capacity.
        pub fn with_capacity(capacity: usize) -> Self {
            Self(hashbrown::HashMap::with_capacity_and_hasher(
                capacity,
                HashBuilder::default(),
            ))
        }
    }

    impl<K: Hash + Eq, V> HashMap<K, V> {
        /// Inserts a key-value pair, returning the previous value of the key.
        pub fn insert(&mut self, k: K, v: V) -> Option<V> {
            self.0.insert(k, v)
        }

        /// Returns the number of elements in the map.
        pub fn len(&self) -> usize {
            self.0.len()
        }

        /// Returns `true` if the map contains no elements.
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        /// Clears the map, removing all key-value pairs.
        pub fn clear(&mut self) {
            self.0.clear()
        }

        /// Returns a reference to the value corresponding to the key.
        pub fn get<Q: Hash + Eq + ?Sized>(&self, k: &Q) -> Option<&V>
        where
            K: Borrow<Q>,
        {
            self.0.get(k)
        }

        /// Returns a mutable reference to the value corresponding to the key.
        pub fn get_mut<Q: Hash + Eq + ?Sized>(&mut self, k: &Q) -> Option<&mut V>
        where
            K: Borrow<Q>,
        {
            self.0.get_mut(k)
        }

        /// Returns `true` if the map contains a value for the specified key.
        pub fn contains_key<Q: Hash + Eq + ?Sized>(&self, k: &Q) -> bool
        where
            K: Borrow<Q>,
        {
            self.0.contains_key(k)
        }

        /// Removes a key from the map, returning its value.
        pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, k: &Q) -> Option<V>
        where
            K: Borrow<Q>,
        {
            self.0.remove(k)
        }

        /// An iterator visiting all key-value pairs in arbitrary order.
        pub fn iter(&self) -> hashbrown::hash_map::Iter<'_, K, V, HashBuilder> {
            self.0.iter()
        }

        /// An iterator visiting all keys in arbitrary order.
        pub fn keys(&self) -> hashbrown::hash_map::Keys<'_, K, V, HashBuilder> {
            self.0.keys()
        }

        /// An iterator visiting all values in arbitrary order.
        pub fn values(&self) -> hashbrown::hash_map::Values<'_, K, V, HashBuilder> {
            self.0.values()
        }
    }

    impl<K, V> Default for HashMap<K, V> {
        fn default() -> Self {
            Self::new()
        }
    }

    /// A hash set implemented with open addressing, backed by the
    /// [hashbrown](https://crates.io/crates/hashbrown) crate.
    pub struct HashSet<T>(hashbrown::HashSet<T, HashBuilder>);

    impl<T> HashSet<T> {
        /// Creates an empty `HashSet`.
        pub fn new() -> Self {
            Self(hashbrown::HashSet::with_hasher(HashBuilder::default()))
        }

        /// Creates an empty `HashSet` with at least the specified capacity.
        pub fn with_capacity(capacity: usize) -> Self {
            Self(hashbrown::HashSet::with_capacity_and_hasher(
                capacity,
                HashBuilder::default(),
            ))
        }
    }

    impl<T: Hash + Eq> HashSet<T> {
        /// Adds a value to the set, returning `true` if it was not present.
        pub fn insert(&mut self, value: T) -> bool {
            self.0.insert(value)
        }

        /// Returns the number of elements in the set.
        pub fn len(&self) -> usize {
            self.0.len()
        }

        /// Returns `true` if the set contains no elements.
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        /// Clears the set, removing all values.
        pub fn clear(&mut self) {
            self.0.clear()
        }

        /// Returns `true` if the set contains the specified value.
        pub fn contains<Q: Hash + Eq + ?Sized>(&self, value: &Q) -> bool
        where
            T: Borrow<Q>,
        {
            self.0.contains(value)
        }

        /// Removes a value from the set, returning whether it was present.
        pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, value: &Q) -> bool
        where
            T: Borrow<Q>,
        {
            self.0.remove(value)
        }

        /// An iterator visiting all elements in arbitrary order.
        pub fn iter(&self) -> hashbrown::hash_set::Iter<'_, T, HashBuilder> {
            self.0.iter()
        }
    }

    impl<T> Default for HashSet<T> {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[doc(no_inline)]
pub use core::{arch, cell, cmp, hint, marker, mem, ops, ptr, slice, str};

#[macro_use]
mod macros;

pub mod env;
pub mod io;
pub mod os;
pub mod process;
pub mod sync;
pub mod thread;
pub mod time;

#[cfg(feature = "fs")]
pub mod fs;
#[cfg(feature = "net")]
pub mod net;
