#![no_std]

use allocator::{AllocError, AllocResult, BaseAllocator, ByteAllocator, PageAllocator};
use core::alloc::Layout;
use core::ptr::NonNull;

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///
pub struct EarlyAllocator<const SIZE: usize> {
    start: usize,
    b_pos: usize,
    b_count: usize,
    p_pos: usize,
    end: usize,
}

impl<const SIZE: usize> EarlyAllocator<SIZE> {
    pub const fn new() -> Self {
        Self {
            start: 0,
            b_pos: 0,
            b_count: 0,
            p_pos: 0,
            end: 0,
        }
    }
}

impl<const SIZE: usize> BaseAllocator for EarlyAllocator<SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        if self.end != 0 {
            panic!("early allocator has already been initialized!");
        }
        self.start = start;
        self.b_pos = start;
        self.b_count = 0;
        self.p_pos = start + size;
        self.end = start + size;
    }

    fn add_memory(&mut self, start: usize, size: usize) -> AllocResult {
        if size == 0 {
            return Ok(());
        }
        let end = start + size;
        if end == self.start {
            // extend the managed range backward
            self.start = start;
            self.b_pos = start;
            Ok(())
        } else if self.end == 0 {
            // not initialized yet
            self.start = start;
            self.b_pos = start;
            self.b_count = 0;
            self.p_pos = end;
            self.end = end;
            Ok(())
        } else {
            // not contiguous, cannot merge into one memory range
            Err(AllocError::InvalidParam)
        }
    }
}

impl<const SIZE: usize> ByteAllocator for EarlyAllocator<SIZE> {
    fn alloc(&mut self, layout: Layout) -> AllocResult<NonNull<u8>> {
        let align = layout.align().max(1);
        let pos = (self.b_pos + align - 1) & !(align - 1);
        let new_pos = pos + layout.size();
        if new_pos > self.p_pos {
            Err(AllocError::NoMemory)
        } else {
            self.b_pos = new_pos;
            self.b_count += 1;
            Ok(unsafe { NonNull::new_unchecked(pos as *mut u8) })
        }
    }

    fn dealloc(&mut self, _pos: NonNull<u8>, _layout: Layout) {
        assert!(
            self.b_count > 0,
            "double free detected in early allocator"
        );
        self.b_count -= 1;
        if self.b_count == 0 {
            // free bytes-used area when all byte allocations are dropped
            self.b_pos = self.start;
        }
    }

    fn total_bytes(&self) -> usize {
        self.end - self.start
    }

    fn used_bytes(&self) -> usize {
        self.b_pos - self.start
    }

    fn available_bytes(&self) -> usize {
        self.p_pos - self.b_pos
    }
}

impl<const SIZE: usize> PageAllocator for EarlyAllocator<SIZE> {
    const PAGE_SIZE: usize = SIZE;

    fn alloc_pages(&mut self, num_pages: usize, align_pow2: usize) -> AllocResult<usize> {
        let align = 1usize << align_pow2;
        let size = num_pages * Self::PAGE_SIZE;
        let pos = (self.p_pos & !(align - 1)) - size;
        if pos >= self.b_pos && self.p_pos >= pos {
            self.p_pos = pos;
            Ok(pos)
        } else {
            Err(AllocError::NoMemory)
        }
    }

    fn dealloc_pages(&mut self, _pos: usize, _num_pages: usize) {
        // for pages area, it will never be freed!
    }

    fn total_pages(&self) -> usize {
        (self.end - self.start) / Self::PAGE_SIZE
    }

    fn used_pages(&self) -> usize {
        (self.end - self.p_pos) / Self::PAGE_SIZE
    }

    fn available_pages(&self) -> usize {
        (self.p_pos - self.b_pos) / Self::PAGE_SIZE
    }
}
