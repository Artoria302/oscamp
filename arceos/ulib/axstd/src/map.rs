use core::cmp;
use core::fmt;
use core::hash::{BuildHasher, Hash, Hasher};
use core::marker::PhantomData;
use core::mem;
use core::ptr;

use arceos_api::random::ax_random;

use hashbrown::hash_map as base;

pub struct HashMap<K, V, S = RandomState> {
    base: base::HashMap<K, V, S>,
}

impl<K, V> HashMap<K, V, RandomState> {
    #[inline]
    #[must_use]
    pub fn new() -> HashMap<K, V, RandomState> {
        Self {
            base: base::HashMap::default(),
        }
    }
}

impl<K, V, S> HashMap<K, V, S> {
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            base: self.base.iter(),
        }
    }

    pub fn len(&self) -> usize {
        self.base.len()
    }

    pub fn clear(&mut self) {
        self.base.clear();
    }
}

impl<K, V, S> HashMap<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        self.base.insert(k, v)
    }
}

pub struct Iter<'a, K: 'a, V: 'a> {
    base: base::Iter<'a, K, V>,
}

impl<K, V> Clone for Iter<'_, K, V> {
    #[inline]
    fn clone(&self) -> Self {
        Iter {
            base: self.base.clone(),
        }
    }
}

impl<K: fmt::Debug, V: fmt::Debug> fmt::Debug for Iter<'_, K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.clone()).finish()
    }
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    #[inline]
    fn next(&mut self) -> Option<(&'a K, &'a V)> {
        self.base.next()
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.base.size_hint()
    }
    #[inline]
    fn count(self) -> usize {
        self.base.len()
    }
    #[inline]
    fn fold<B, F>(self, init: B, f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.base.fold(init, f)
    }
}

#[derive(Clone)]
pub struct RandomState {
    k0: u64,
    k1: u64,
}

impl RandomState {
    #[inline]
    #[must_use]
    pub fn new() -> RandomState {
        let r = ax_random();
        let k0 = (r >> 64) as u64;
        let k1 = r as u64;
        RandomState { k0, k1 }
    }
}

impl Default for RandomState {
    #[inline]
    fn default() -> RandomState {
        RandomState::new()
    }
}

impl fmt::Debug for RandomState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RandomState").finish_non_exhaustive()
    }
}

impl BuildHasher for RandomState {
    type Hasher = DefaultHasher;
    #[inline]
    #[allow(deprecated)]
    fn build_hasher(&self) -> DefaultHasher {
        DefaultHasher(SipHasher13::new_with_keys(self.k0, self.k1))
    }
}

#[allow(deprecated)]
#[derive(Clone, Debug)]
pub struct DefaultHasher(SipHasher13);

impl DefaultHasher {
    #[inline]
    #[allow(deprecated)]
    #[must_use]
    pub fn new() -> DefaultHasher {
        DefaultHasher(SipHasher13::new_with_keys(0, 0))
    }
}

impl Default for DefaultHasher {
    #[inline]
    fn default() -> DefaultHasher {
        DefaultHasher::new()
    }
}

impl Hasher for DefaultHasher {
    #[inline]
    fn write(&mut self, msg: &[u8]) {
        self.0.write(msg)
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0.finish()
    }
}

#[derive(Debug, Clone, Default)]
pub struct SipHasher13 {
    hasher: MyHasher<Sip13Rounds>,
}

#[derive(Debug)]
struct MyHasher<S: Sip> {
    k0: u64,
    k1: u64,
    length: usize, // how many bytes we've processed
    state: State,  // hash State
    tail: u64,     // unprocessed bytes le
    ntail: usize,  // how many bytes in tail are valid
    _marker: PhantomData<S>,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct State {
    // v0, v2 and v1, v3 show up in pairs in the algorithm,
    // and simd implementations of SipHash will use vectors
    // of v02 and v13. By placing them in this order in the struct,
    // the compiler can pick up on just a few simd optimizations by itself.
    v0: u64,
    v2: u64,
    v1: u64,
    v3: u64,
}

macro_rules! compress {
    ($state:expr) => {{
        compress!($state.v0, $state.v1, $state.v2, $state.v3)
    }};
    ($v0:expr, $v1:expr, $v2:expr, $v3:expr) => {{
        $v0 = $v0.wrapping_add($v1);
        $v1 = $v1.rotate_left(13);
        $v1 ^= $v0;
        $v0 = $v0.rotate_left(32);
        $v2 = $v2.wrapping_add($v3);
        $v3 = $v3.rotate_left(16);
        $v3 ^= $v2;
        $v0 = $v0.wrapping_add($v3);
        $v3 = $v3.rotate_left(21);
        $v3 ^= $v0;
        $v2 = $v2.wrapping_add($v1);
        $v1 = $v1.rotate_left(17);
        $v1 ^= $v2;
        $v2 = $v2.rotate_left(32);
    }};
}

macro_rules! load_int_le {
    ($buf:expr, $i:expr, $int_ty:ident) => {{
        debug_assert!($i + mem::size_of::<$int_ty>() <= $buf.len());
        let mut data = 0 as $int_ty;
        ptr::copy_nonoverlapping(
            $buf.as_ptr().add($i),
            &mut data as *mut _ as *mut u8,
            mem::size_of::<$int_ty>(),
        );
        data.to_le()
    }};
}

#[inline]
unsafe fn u8to64_le(buf: &[u8], start: usize, len: usize) -> u64 {
    debug_assert!(len < 8);
    let mut i = 0; // current byte index (from LSB) in the output u64
    let mut out = 0;
    if i + 3 < len {
        // SAFETY: `i` cannot be greater than `len`, and the caller must guarantee
        // that the index start..start+len is in bounds.
        out = unsafe { load_int_le!(buf, start + i, u32) } as u64;
        i += 4;
    }
    if i + 1 < len {
        // SAFETY: same as above.
        out |= (unsafe { load_int_le!(buf, start + i, u16) } as u64) << (i * 8);
        i += 2
    }
    if i < len {
        // SAFETY: same as above.
        out |= (unsafe { *buf.get_unchecked(start + i) } as u64) << (i * 8);
        i += 1;
    }
    //FIXME(fee1-dead): use debug_assert_eq
    debug_assert!(i == len);
    out
}

impl SipHasher13 {
    #[inline]
    pub fn new_with_keys(key0: u64, key1: u64) -> SipHasher13 {
        SipHasher13 {
            hasher: MyHasher::new_with_keys(key0, key1),
        }
    }
}

impl<S: Sip> MyHasher<S> {
    #[inline]
    fn new_with_keys(key0: u64, key1: u64) -> MyHasher<S> {
        let mut state = MyHasher {
            k0: key0,
            k1: key1,
            length: 0,
            state: State {
                v0: 0,
                v1: 0,
                v2: 0,
                v3: 0,
            },
            tail: 0,
            ntail: 0,
            _marker: PhantomData,
        };
        state.reset();
        state
    }

    #[inline]
    fn reset(&mut self) {
        self.length = 0;
        self.state.v0 = self.k0 ^ 0x736f6d6570736575;
        self.state.v1 = self.k1 ^ 0x646f72616e646f6d;
        self.state.v2 = self.k0 ^ 0x6c7967656e657261;
        self.state.v3 = self.k1 ^ 0x7465646279746573;
        self.ntail = 0;
    }
}

impl Hasher for SipHasher13 {
    #[inline]
    fn write(&mut self, msg: &[u8]) {
        self.hasher.write(msg)
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hasher.finish()
    }
}

impl<S: Sip> Hasher for MyHasher<S> {
    #[inline]
    fn write(&mut self, msg: &[u8]) {
        let length = msg.len();
        self.length += length;

        let mut needed = 0;

        if self.ntail != 0 {
            needed = 8 - self.ntail;
            // SAFETY: `cmp::min(length, needed)` is guaranteed to not be over `length`
            self.tail |= unsafe { u8to64_le(msg, 0, cmp::min(length, needed)) } << (8 * self.ntail);
            if length < needed {
                self.ntail += length;
                return;
            } else {
                self.state.v3 ^= self.tail;
                S::c_rounds(&mut self.state);
                self.state.v0 ^= self.tail;
                self.ntail = 0;
            }
        }

        // Buffered tail is now flushed, process new input.
        let len = length - needed;
        let left = len & 0x7; // len % 8

        let mut i = needed;
        while i < len - left {
            // SAFETY: because `len - left` is the biggest multiple of 8 under
            // `len`, and because `i` starts at `needed` where `len` is `length - needed`,
            // `i + 8` is guaranteed to be less than or equal to `length`.
            let mi = unsafe { load_int_le!(msg, i, u64) };

            self.state.v3 ^= mi;
            S::c_rounds(&mut self.state);
            self.state.v0 ^= mi;

            i += 8;
        }

        // SAFETY: `i` is now `needed + len.div_euclid(8) * 8`,
        // so `i + left` = `needed + len` = `length`, which is by
        // definition equal to `msg.len()`.
        self.tail = unsafe { u8to64_le(msg, i, left) };
        self.ntail = left;
    }

    #[inline]
    fn finish(&self) -> u64 {
        let mut state = self.state;

        let b: u64 = ((self.length as u64 & 0xff) << 56) | self.tail;

        state.v3 ^= b;
        S::c_rounds(&mut state);
        state.v0 ^= b;

        state.v2 ^= 0xff;
        S::d_rounds(&mut state);

        state.v0 ^ state.v1 ^ state.v2 ^ state.v3
    }
}

impl<S: Sip> Clone for MyHasher<S> {
    #[inline]
    fn clone(&self) -> MyHasher<S> {
        MyHasher {
            k0: self.k0,
            k1: self.k1,
            length: self.length,
            state: self.state,
            tail: self.tail,
            ntail: self.ntail,
            _marker: self._marker,
        }
    }
}

impl<S: Sip> Default for MyHasher<S> {
    #[inline]
    fn default() -> MyHasher<S> {
        MyHasher::new_with_keys(0, 0)
    }
}

#[doc(hidden)]
trait Sip {
    fn c_rounds(_: &mut State);
    fn d_rounds(_: &mut State);
}

#[derive(Debug, Clone, Default)]
struct Sip13Rounds;

impl Sip for Sip13Rounds {
    #[inline]
    fn c_rounds(state: &mut State) {
        compress!(state);
    }

    #[inline]
    fn d_rounds(state: &mut State) {
        compress!(state);
        compress!(state);
        compress!(state);
    }
}
