use core::mem;

/// Extract the metadata component of a pointer.
#[inline]
pub const fn metadata<T: ?Sized>(ptr: *const T) -> usize {
  unsafe { PtrRepr::empty().set_const(ptr).components.metadata }
}

/// Forms a (possibly-wide) raw pointer from a data address and metadata.
#[inline]
pub const fn from_raw_parts<T: ?Sized>(address: *const (), metadata: usize) -> *const T {
  unsafe { PtrRepr::with_components(address, metadata).const_ptr }
}

/// Performs the same functionality as [`from_raw_parts`], except that a
/// raw `*mut` pointer is returned, as opposed to a raw `*const` pointer.
#[inline]
pub const fn from_raw_parts_mut<T: ?Sized>(address: *mut (), metadata: usize) -> *mut T {
  unsafe { PtrRepr::with_components(address, metadata).mut_ptr }
}

/// Extract the address component of a non-dangling pointer.
// #[inline]
pub(super) const fn addr_of<T: ?Sized>(ptr: *const T) -> usize {
  // Compute `ptr` raw pointer `ptr`.
  let ptr = ptr.cast::<()>();
  // SAFETY: This no-op trick ensure that `ptr` has a provenance:
  // This will result in a compile-error if `ptr` is a dangling pointer (has no provenance).
  let _ = unsafe { core::ptr::addr_of!((*(ptr as *const (u8, ()))).1) };
  // SAFETY: This is just an inlined `p.addr()` (which is not a `const fn` so we cannot call it).
  // - This is safe at runtime as `ptr` point to a valid non-dangling reference, hence the pointer
  //   has a provenance.
  // - During const eval, it is unsound to perform this transmutation if the pointer has no
  //   provenance; the above trick ensure just that.
  unsafe { mem::transmute(ptr) }
}

#[repr(C)]
union PtrRepr<T: ?Sized> {
  const_ptr: *const T,
  mut_ptr: *mut T,
  components: PtrComponents,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct PtrComponents {
  address: *const (),
  metadata: usize,
}

impl<T: ?Sized> PtrRepr<T> {
  const fn empty() -> Self {
    Self::empty_with(core::ptr::null())
  }

  const fn empty_with(address: *const ()) -> Self {
    Self::with_components(address, 0)
  }

  const fn with_components(address: *const (), metadata: usize) -> Self {
    Self { components: PtrComponents { address, metadata } }
  }

  const fn set_const(mut self, const_ptr: *const T) -> Self {
    self.const_ptr = const_ptr;
    self
  }
}
