#![no_std]

use allocator::{
    BaseAllocator, BitmapPageAllocator, ByteAllocator, PageAllocator, TlsfByteAllocator,
};

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
pub struct EarlyAllocator<const PAGE_SIZE: usize> {
    byte_allocator: TlsfByteAllocator,
    page_allocator: BitmapPageAllocator<PAGE_SIZE>,
}

impl<const PAGE_SIZE: usize> EarlyAllocator<PAGE_SIZE> {
    pub const fn new() -> Self {
        Self {
            byte_allocator: TlsfByteAllocator::new(),
            page_allocator: BitmapPageAllocator::new(),
        }
    }
}

impl<const PAGE_SIZE: usize> BaseAllocator for EarlyAllocator<PAGE_SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        self.byte_allocator.init(start, size / 2);
        self.page_allocator.init(start + size / 2, size / 2);
    }

    fn add_memory(&mut self, _start: usize, _size: usize) -> allocator::AllocResult {
        unimplemented!()
    }
}

impl<const PAGE_SIZE: usize> ByteAllocator for EarlyAllocator<PAGE_SIZE> {
    fn alloc(
        &mut self,
        layout: core::alloc::Layout,
    ) -> allocator::AllocResult<core::ptr::NonNull<u8>> {
        self.byte_allocator.alloc(layout)
    }

    fn dealloc(&mut self, pos: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
        self.byte_allocator.dealloc(pos, layout);
    }

    fn total_bytes(&self) -> usize {
        self.byte_allocator.total_bytes()
    }

    fn used_bytes(&self) -> usize {
        self.byte_allocator.used_bytes()
    }

    fn available_bytes(&self) -> usize {
        self.byte_allocator.available_bytes()
    }
}

impl<const PAGE_SIZE: usize> PageAllocator for EarlyAllocator<PAGE_SIZE> {
    const PAGE_SIZE: usize = PAGE_SIZE;

    fn alloc_pages(
        &mut self,
        num_pages: usize,
        align_pow2: usize,
    ) -> allocator::AllocResult<usize> {
        self.page_allocator.alloc_pages(num_pages, align_pow2)
    }

    fn dealloc_pages(&mut self, pos: usize, num_pages: usize) {
        self.page_allocator.dealloc_pages(pos, num_pages);
    }

    fn total_pages(&self) -> usize {
        self.page_allocator.total_pages()
    }

    fn used_pages(&self) -> usize {
        self.page_allocator.used_pages()
    }

    fn available_pages(&self) -> usize {
        self.page_allocator.available_pages()
    }
}
