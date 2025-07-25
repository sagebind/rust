//! Utilities for dynamic typing or type reflection.
//!
//! # `Any` and `TypeId`
//!
//! `Any` itself can be used to get a `TypeId`, and has more features when used
//! as a trait object. As `&dyn Any` (a borrowed trait object), it has the `is`
//! and `downcast_ref` methods, to test if the contained value is of a given type,
//! and to get a reference to the inner value as a type. As `&mut dyn Any`, there
//! is also the `downcast_mut` method, for getting a mutable reference to the
//! inner value. `Box<dyn Any>` adds the `downcast` method, which attempts to
//! convert to a `Box<T>`. See the [`Box`] documentation for the full details.
//!
//! Note that `&dyn Any` is limited to testing whether a value is of a specified
//! concrete type, and cannot be used to test whether a type implements a trait.
//!
//! [`Box`]: ../../std/boxed/struct.Box.html
//!
//! # Smart pointers and `dyn Any`
//!
//! One piece of behavior to keep in mind when using `Any` as a trait object,
//! especially with types like `Box<dyn Any>` or `Arc<dyn Any>`, is that simply
//! calling `.type_id()` on the value will produce the `TypeId` of the
//! *container*, not the underlying trait object. This can be avoided by
//! converting the smart pointer into a `&dyn Any` instead, which will return
//! the object's `TypeId`. For example:
//!
//! ```
//! use std::any::{Any, TypeId};
//!
//! let boxed: Box<dyn Any> = Box::new(3_i32);
//!
//! // You're more likely to want this:
//! let actual_id = (&*boxed).type_id();
//! // ... than this:
//! let boxed_id = boxed.type_id();
//!
//! assert_eq!(actual_id, TypeId::of::<i32>());
//! assert_eq!(boxed_id, TypeId::of::<Box<dyn Any>>());
//! ```
//!
//! ## Examples
//!
//! Consider a situation where we want to log a value passed to a function.
//! We know the value we're working on implements `Debug`, but we don't know its
//! concrete type. We want to give special treatment to certain types: in this
//! case printing out the length of `String` values prior to their value.
//! We don't know the concrete type of our value at compile time, so we need to
//! use runtime reflection instead.
//!
//! ```rust
//! use std::fmt::Debug;
//! use std::any::Any;
//!
//! // Logger function for any type that implements `Debug`.
//! fn log<T: Any + Debug>(value: &T) {
//!     let value_any = value as &dyn Any;
//!
//!     // Try to convert our value to a `String`. If successful, we want to
//!     // output the `String`'s length as well as its value. If not, it's a
//!     // different type: just print it out unadorned.
//!     match value_any.downcast_ref::<String>() {
//!         Some(as_string) => {
//!             println!("String ({}): {}", as_string.len(), as_string);
//!         }
//!         None => {
//!             println!("{value:?}");
//!         }
//!     }
//! }
//!
//! // This function wants to log its parameter out prior to doing work with it.
//! fn do_work<T: Any + Debug>(value: &T) {
//!     log(value);
//!     // ...do some other work
//! }
//!
//! fn main() {
//!     let my_string = "Hello World".to_string();
//!     do_work(&my_string);
//!
//!     let my_i8: i8 = 100;
//!     do_work(&my_i8);
//! }
//! ```
//!

#![stable(feature = "rust1", since = "1.0.0")]

use crate::{fmt, hash, intrinsics};

///////////////////////////////////////////////////////////////////////////////
// Any trait
///////////////////////////////////////////////////////////////////////////////

/// A trait to emulate dynamic typing.
///
/// Most types implement `Any`. However, any type which contains a non-`'static` reference does not.
/// See the [module-level documentation][mod] for more details.
///
/// [mod]: crate::any
// This trait is not unsafe, though we rely on the specifics of it's sole impl's
// `type_id` function in unsafe code (e.g., `downcast`). Normally, that would be
// a problem, but because the only impl of `Any` is a blanket implementation, no
// other code can implement `Any`.
//
// We could plausibly make this trait unsafe -- it would not cause breakage,
// since we control all the implementations -- but we choose not to as that's
// both not really necessary and may confuse users about the distinction of
// unsafe traits and unsafe methods (i.e., `type_id` would still be safe to call,
// but we would likely want to indicate as such in documentation).
#[stable(feature = "rust1", since = "1.0.0")]
#[rustc_diagnostic_item = "Any"]
pub trait Any: 'static {
    /// Gets the `TypeId` of `self`.
    ///
    /// If called on a `dyn Any` trait object
    /// (or a trait object of a subtrait of `Any`),
    /// this returns the `TypeId` of the underlying
    /// concrete type, not that of `dyn Any` itself.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::{Any, TypeId};
    ///
    /// fn is_string(s: &dyn Any) -> bool {
    ///     TypeId::of::<String>() == s.type_id()
    /// }
    ///
    /// assert_eq!(is_string(&0), false);
    /// assert_eq!(is_string(&"cookie monster".to_string()), true);
    /// ```
    #[stable(feature = "get_type_id", since = "1.34.0")]
    fn type_id(&self) -> TypeId;
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<T: 'static + ?Sized> Any for T {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
}

///////////////////////////////////////////////////////////////////////////////
// Extension methods for Any trait objects.
///////////////////////////////////////////////////////////////////////////////

#[stable(feature = "rust1", since = "1.0.0")]
impl fmt::Debug for dyn Any {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Any").finish_non_exhaustive()
    }
}

// Ensure that the result of e.g., joining a thread can be printed and
// hence used with `unwrap`. May eventually no longer be needed if
// dispatch works with upcasting.
#[stable(feature = "rust1", since = "1.0.0")]
impl fmt::Debug for dyn Any + Send {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Any").finish_non_exhaustive()
    }
}

#[stable(feature = "any_send_sync_methods", since = "1.28.0")]
impl fmt::Debug for dyn Any + Send + Sync {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Any").finish_non_exhaustive()
    }
}

impl dyn Any {
    /// Returns `true` if the inner type is the same as `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn is_string(s: &dyn Any) {
    ///     if s.is::<String>() {
    ///         println!("It's a string!");
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// is_string(&0);
    /// is_string(&"cookie monster".to_string());
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        // Get `TypeId` of the type this function is instantiated with.
        let t = TypeId::of::<T>();

        // Get `TypeId` of the type in the trait object (`self`).
        let concrete = self.type_id();

        // Compare both `TypeId`s on equality.
        t == concrete
    }

    /// Returns some reference to the inner value if it is of type `T`, or
    /// `None` if it isn't.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn print_if_string(s: &dyn Any) {
    ///     if let Some(string) = s.downcast_ref::<String>() {
    ///         println!("It's a string({}): '{}'", string.len(), string);
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// print_if_string(&0);
    /// print_if_string(&"cookie monster".to_string());
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        if self.is::<T>() {
            // SAFETY: just checked whether we are pointing to the correct type, and we can rely on
            // that check for memory safety because we have implemented Any for all types; no other
            // impls can exist as they would conflict with our impl.
            unsafe { Some(self.downcast_ref_unchecked()) }
        } else {
            None
        }
    }

    /// Returns some mutable reference to the inner value if it is of type `T`, or
    /// `None` if it isn't.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn modify_if_u32(s: &mut dyn Any) {
    ///     if let Some(num) = s.downcast_mut::<u32>() {
    ///         *num = 42;
    ///     }
    /// }
    ///
    /// let mut x = 10u32;
    /// let mut s = "starlord".to_string();
    ///
    /// modify_if_u32(&mut x);
    /// modify_if_u32(&mut s);
    ///
    /// assert_eq!(x, 42);
    /// assert_eq!(&s, "starlord");
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            // SAFETY: just checked whether we are pointing to the correct type, and we can rely on
            // that check for memory safety because we have implemented Any for all types; no other
            // impls can exist as they would conflict with our impl.
            unsafe { Some(self.downcast_mut_unchecked()) }
        } else {
            None
        }
    }

    /// Returns a reference to the inner value as type `dyn T`.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(downcast_unchecked)]
    ///
    /// use std::any::Any;
    ///
    /// let x: Box<dyn Any> = Box::new(1_usize);
    ///
    /// unsafe {
    ///     assert_eq!(*x.downcast_ref_unchecked::<usize>(), 1);
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// The contained value must be of type `T`. Calling this method
    /// with the incorrect type is *undefined behavior*.
    #[unstable(feature = "downcast_unchecked", issue = "90850")]
    #[inline]
    pub unsafe fn downcast_ref_unchecked<T: Any>(&self) -> &T {
        debug_assert!(self.is::<T>());
        // SAFETY: caller guarantees that T is the correct type
        unsafe { &*(self as *const dyn Any as *const T) }
    }

    /// Returns a mutable reference to the inner value as type `dyn T`.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(downcast_unchecked)]
    ///
    /// use std::any::Any;
    ///
    /// let mut x: Box<dyn Any> = Box::new(1_usize);
    ///
    /// unsafe {
    ///     *x.downcast_mut_unchecked::<usize>() += 1;
    /// }
    ///
    /// assert_eq!(*x.downcast_ref::<usize>().unwrap(), 2);
    /// ```
    ///
    /// # Safety
    ///
    /// The contained value must be of type `T`. Calling this method
    /// with the incorrect type is *undefined behavior*.
    #[unstable(feature = "downcast_unchecked", issue = "90850")]
    #[inline]
    pub unsafe fn downcast_mut_unchecked<T: Any>(&mut self) -> &mut T {
        debug_assert!(self.is::<T>());
        // SAFETY: caller guarantees that T is the correct type
        unsafe { &mut *(self as *mut dyn Any as *mut T) }
    }
}

impl dyn Any + Send {
    /// Forwards to the method defined on the type `dyn Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn is_string(s: &(dyn Any + Send)) {
    ///     if s.is::<String>() {
    ///         println!("It's a string!");
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// is_string(&0);
    /// is_string(&"cookie monster".to_string());
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        <dyn Any>::is::<T>(self)
    }

    /// Forwards to the method defined on the type `dyn Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn print_if_string(s: &(dyn Any + Send)) {
    ///     if let Some(string) = s.downcast_ref::<String>() {
    ///         println!("It's a string({}): '{}'", string.len(), string);
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// print_if_string(&0);
    /// print_if_string(&"cookie monster".to_string());
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        <dyn Any>::downcast_ref::<T>(self)
    }

    /// Forwards to the method defined on the type `dyn Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn modify_if_u32(s: &mut (dyn Any + Send)) {
    ///     if let Some(num) = s.downcast_mut::<u32>() {
    ///         *num = 42;
    ///     }
    /// }
    ///
    /// let mut x = 10u32;
    /// let mut s = "starlord".to_string();
    ///
    /// modify_if_u32(&mut x);
    /// modify_if_u32(&mut s);
    ///
    /// assert_eq!(x, 42);
    /// assert_eq!(&s, "starlord");
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        <dyn Any>::downcast_mut::<T>(self)
    }

    /// Forwards to the method defined on the type `dyn Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(downcast_unchecked)]
    ///
    /// use std::any::Any;
    ///
    /// let x: Box<dyn Any> = Box::new(1_usize);
    ///
    /// unsafe {
    ///     assert_eq!(*x.downcast_ref_unchecked::<usize>(), 1);
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// The contained value must be of type `T`. Calling this method
    /// with the incorrect type is *undefined behavior*.
    #[unstable(feature = "downcast_unchecked", issue = "90850")]
    #[inline]
    pub unsafe fn downcast_ref_unchecked<T: Any>(&self) -> &T {
        // SAFETY: guaranteed by caller
        unsafe { <dyn Any>::downcast_ref_unchecked::<T>(self) }
    }

    /// Forwards to the method defined on the type `dyn Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(downcast_unchecked)]
    ///
    /// use std::any::Any;
    ///
    /// let mut x: Box<dyn Any> = Box::new(1_usize);
    ///
    /// unsafe {
    ///     *x.downcast_mut_unchecked::<usize>() += 1;
    /// }
    ///
    /// assert_eq!(*x.downcast_ref::<usize>().unwrap(), 2);
    /// ```
    ///
    /// # Safety
    ///
    /// The contained value must be of type `T`. Calling this method
    /// with the incorrect type is *undefined behavior*.
    #[unstable(feature = "downcast_unchecked", issue = "90850")]
    #[inline]
    pub unsafe fn downcast_mut_unchecked<T: Any>(&mut self) -> &mut T {
        // SAFETY: guaranteed by caller
        unsafe { <dyn Any>::downcast_mut_unchecked::<T>(self) }
    }
}

impl dyn Any + Send + Sync {
    /// Forwards to the method defined on the type `Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn is_string(s: &(dyn Any + Send + Sync)) {
    ///     if s.is::<String>() {
    ///         println!("It's a string!");
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// is_string(&0);
    /// is_string(&"cookie monster".to_string());
    /// ```
    #[stable(feature = "any_send_sync_methods", since = "1.28.0")]
    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        <dyn Any>::is::<T>(self)
    }

    /// Forwards to the method defined on the type `Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn print_if_string(s: &(dyn Any + Send + Sync)) {
    ///     if let Some(string) = s.downcast_ref::<String>() {
    ///         println!("It's a string({}): '{}'", string.len(), string);
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// print_if_string(&0);
    /// print_if_string(&"cookie monster".to_string());
    /// ```
    #[stable(feature = "any_send_sync_methods", since = "1.28.0")]
    #[inline]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        <dyn Any>::downcast_ref::<T>(self)
    }

    /// Forwards to the method defined on the type `Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn modify_if_u32(s: &mut (dyn Any + Send + Sync)) {
    ///     if let Some(num) = s.downcast_mut::<u32>() {
    ///         *num = 42;
    ///     }
    /// }
    ///
    /// let mut x = 10u32;
    /// let mut s = "starlord".to_string();
    ///
    /// modify_if_u32(&mut x);
    /// modify_if_u32(&mut s);
    ///
    /// assert_eq!(x, 42);
    /// assert_eq!(&s, "starlord");
    /// ```
    #[stable(feature = "any_send_sync_methods", since = "1.28.0")]
    #[inline]
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        <dyn Any>::downcast_mut::<T>(self)
    }

    /// Forwards to the method defined on the type `Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(downcast_unchecked)]
    ///
    /// use std::any::Any;
    ///
    /// let x: Box<dyn Any> = Box::new(1_usize);
    ///
    /// unsafe {
    ///     assert_eq!(*x.downcast_ref_unchecked::<usize>(), 1);
    /// }
    /// ```
    /// # Safety
    ///
    /// The contained value must be of type `T`. Calling this method
    /// with the incorrect type is *undefined behavior*.
    #[unstable(feature = "downcast_unchecked", issue = "90850")]
    #[inline]
    pub unsafe fn downcast_ref_unchecked<T: Any>(&self) -> &T {
        // SAFETY: guaranteed by caller
        unsafe { <dyn Any>::downcast_ref_unchecked::<T>(self) }
    }

    /// Forwards to the method defined on the type `Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(downcast_unchecked)]
    ///
    /// use std::any::Any;
    ///
    /// let mut x: Box<dyn Any> = Box::new(1_usize);
    ///
    /// unsafe {
    ///     *x.downcast_mut_unchecked::<usize>() += 1;
    /// }
    ///
    /// assert_eq!(*x.downcast_ref::<usize>().unwrap(), 2);
    /// ```
    /// # Safety
    ///
    /// The contained value must be of type `T`. Calling this method
    /// with the incorrect type is *undefined behavior*.
    #[unstable(feature = "downcast_unchecked", issue = "90850")]
    #[inline]
    pub unsafe fn downcast_mut_unchecked<T: Any>(&mut self) -> &mut T {
        // SAFETY: guaranteed by caller
        unsafe { <dyn Any>::downcast_mut_unchecked::<T>(self) }
    }
}

///////////////////////////////////////////////////////////////////////////////
// TypeID and its methods
///////////////////////////////////////////////////////////////////////////////

/// A `TypeId` represents a globally unique identifier for a type.
///
/// Each `TypeId` is an opaque object which does not allow inspection of what's
/// inside but does allow basic operations such as cloning, comparison,
/// printing, and showing.
///
/// A `TypeId` is currently only available for types which ascribe to `'static`,
/// but this limitation may be removed in the future.
///
/// While `TypeId` implements `Hash`, `PartialOrd`, and `Ord`, it is worth
/// noting that the hashes and ordering will vary between Rust releases. Beware
/// of relying on them inside of your code!
///
/// # Danger of Improper Variance
///
/// You might think that subtyping is impossible between two static types,
/// but this is false; there exists a static type with a static subtype.
/// To wit, `fn(&str)`, which is short for `for<'any> fn(&'any str)`, and
/// `fn(&'static str)`, are two distinct, static types, and yet,
/// `fn(&str)` is a subtype of `fn(&'static str)`, since any value of type
/// `fn(&str)` can be used where a value of type `fn(&'static str)` is needed.
///
/// This means that abstractions around `TypeId`, despite its
/// `'static` bound on arguments, still need to worry about unnecessary
/// and improper variance: it is advisable to strive for invariance
/// first. The usability impact will be negligible, while the reduction
/// in the risk of unsoundness will be most welcome.
///
/// ## Examples
///
/// Suppose `SubType` is a subtype of `SuperType`, that is,
/// a value of type `SubType` can be used wherever
/// a value of type `SuperType` is expected.
/// Suppose also that `CoVar<T>` is a generic type, which is covariant over `T`
/// (like many other types, including `PhantomData<T>` and `Vec<T>`).
///
/// Then, by covariance, `CoVar<SubType>` is a subtype of `CoVar<SuperType>`,
/// that is, a value of type `CoVar<SubType>` can be used wherever
/// a value of type `CoVar<SuperType>` is expected.
///
/// Then if `CoVar<SuperType>` relies on `TypeId::of::<SuperType>()` to uphold any invariants,
/// those invariants may be broken because a value of type `CoVar<SuperType>` can be created
/// without going through any of its methods, like so:
/// ```
/// type SubType = fn(&());
/// type SuperType = fn(&'static ());
/// type CoVar<T> = Vec<T>; // imagine something more complicated
///
/// let sub: CoVar<SubType> = CoVar::new();
/// // we have a `CoVar<SuperType>` instance without
/// // *ever* having called `CoVar::<SuperType>::new()`!
/// let fake_super: CoVar<SuperType> = sub;
/// ```
///
/// The following is an example program that tries to use `TypeId::of` to
/// implement a generic type `Unique<T>` that guarantees unique instances for each `Unique<T>`,
/// that is, and for each type `T` there can be at most one value of type `Unique<T>` at any time.
///
/// ```
/// mod unique {
///     use std::any::TypeId;
///     use std::collections::BTreeSet;
///     use std::marker::PhantomData;
///     use std::sync::Mutex;
///
///     static ID_SET: Mutex<BTreeSet<TypeId>> = Mutex::new(BTreeSet::new());
///
///     // TypeId has only covariant uses, which makes Unique covariant over TypeAsId 🚨
///     #[derive(Debug, PartialEq)]
///     pub struct Unique<TypeAsId: 'static>(
///         // private field prevents creation without `new` outside this module
///         PhantomData<TypeAsId>,
///     );
///
///     impl<TypeAsId: 'static> Unique<TypeAsId> {
///         pub fn new() -> Option<Self> {
///             let mut set = ID_SET.lock().unwrap();
///             (set.insert(TypeId::of::<TypeAsId>())).then(|| Self(PhantomData))
///         }
///     }
///
///     impl<TypeAsId: 'static> Drop for Unique<TypeAsId> {
///         fn drop(&mut self) {
///             let mut set = ID_SET.lock().unwrap();
///             (!set.remove(&TypeId::of::<TypeAsId>())).then(|| panic!("duplicity detected"));
///         }
///     }
/// }
///
/// use unique::Unique;
///
/// // `OtherRing` is a subtype of `TheOneRing`. Both are 'static, and thus have a TypeId.
/// type TheOneRing = fn(&'static ());
/// type OtherRing = fn(&());
///
/// fn main() {
///     let the_one_ring: Unique<TheOneRing> = Unique::new().unwrap();
///     assert_eq!(Unique::<TheOneRing>::new(), None);
///
///     let other_ring: Unique<OtherRing> = Unique::new().unwrap();
///     // Use that `Unique<OtherRing>` is a subtype of `Unique<TheOneRing>` 🚨
///     let fake_one_ring: Unique<TheOneRing> = other_ring;
///     assert_eq!(fake_one_ring, the_one_ring);
///
///     std::mem::forget(fake_one_ring);
/// }
/// ```
#[derive(Clone, Copy, Eq, PartialOrd, Ord)]
#[stable(feature = "rust1", since = "1.0.0")]
#[lang = "type_id"]
pub struct TypeId {
    /// This needs to be an array of pointers, since there is provenance
    /// in the first array field. This provenance knows exactly which type
    /// the TypeId actually is, allowing CTFE and miri to operate based off it.
    /// At runtime all the pointers in the array contain bits of the hash, making
    /// the entire `TypeId` actually just be a `u128` hash of the type.
    pub(crate) data: [*const (); 16 / size_of::<*const ()>()],
}

// SAFETY: the raw pointer is always an integer
#[stable(feature = "rust1", since = "1.0.0")]
unsafe impl Send for TypeId {}
// SAFETY: the raw pointer is always an integer
#[stable(feature = "rust1", since = "1.0.0")]
unsafe impl Sync for TypeId {}

#[stable(feature = "rust1", since = "1.0.0")]
#[rustc_const_unstable(feature = "const_type_id", issue = "77125")]
impl const PartialEq for TypeId {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        #[cfg(miri)]
        return crate::intrinsics::type_id_eq(*self, *other);
        #[cfg(not(miri))]
        {
            let this = self;
            crate::intrinsics::const_eval_select!(
                @capture { this: &TypeId, other: &TypeId } -> bool:
                if const {
                    crate::intrinsics::type_id_eq(*this, *other)
                } else {
                    // Ideally we would just invoke `type_id_eq` unconditionally here,
                    // but since we do not MIR inline intrinsics, because backends
                    // may want to override them (and miri does!), MIR opts do not
                    // clean up this call sufficiently for LLVM to turn repeated calls
                    // of `TypeId` comparisons against one specific `TypeId` into
                    // a lookup table.
                    // SAFETY: We know that at runtime none of the bits have provenance and all bits
                    // are initialized. So we can just convert the whole thing to a `u128` and compare that.
                    unsafe {
                        crate::mem::transmute::<_, u128>(*this) == crate::mem::transmute::<_, u128>(*other)
                    }
                }
            )
        }
    }
}

impl TypeId {
    /// Returns the `TypeId` of the generic type parameter.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::{Any, TypeId};
    ///
    /// fn is_string<T: ?Sized + Any>(_s: &T) -> bool {
    ///     TypeId::of::<String>() == TypeId::of::<T>()
    /// }
    ///
    /// assert_eq!(is_string(&0), false);
    /// assert_eq!(is_string(&"cookie monster".to_string()), true);
    /// ```
    #[must_use]
    #[stable(feature = "rust1", since = "1.0.0")]
    #[rustc_const_unstable(feature = "const_type_id", issue = "77125")]
    pub const fn of<T: ?Sized + 'static>() -> TypeId {
        const { intrinsics::type_id::<T>() }
    }

    fn as_u128(self) -> u128 {
        let mut bytes = [0; 16];

        // This is a provenance-stripping memcpy.
        for (i, chunk) in self.data.iter().copied().enumerate() {
            let chunk = chunk.addr().to_ne_bytes();
            let start = i * chunk.len();
            bytes[start..(start + chunk.len())].copy_from_slice(&chunk);
        }
        u128::from_ne_bytes(bytes)
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl hash::Hash for TypeId {
    #[inline]
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        // We only hash the lower 64 bits of our (128 bit) internal numeric ID,
        // because:
        // - The hashing algorithm which backs `TypeId` is expected to be
        //   unbiased and high quality, meaning further mixing would be somewhat
        //   redundant compared to choosing (the lower) 64 bits arbitrarily.
        // - `Hasher::finish` returns a u64 anyway, so the extra entropy we'd
        //   get from hashing the full value would probably not be useful
        //   (especially given the previous point about the lower 64 bits being
        //   high quality on their own).
        // - It is correct to do so -- only hashing a subset of `self` is still
        //   compatible with an `Eq` implementation that considers the entire
        //   value, as ours does.
        let data =
        // SAFETY: The `offset` stays in-bounds, it just moves the pointer to the 2nd half of the `TypeId`.
        // Only the first ptr-sized chunk ever has provenance, so that second half is always
        // fine to read at integer type.
            unsafe { crate::ptr::read_unaligned(self.data.as_ptr().cast::<u64>().offset(1)) };
        data.hash(state);
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl fmt::Debug for TypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "TypeId({:#034x})", self.as_u128())
    }
}

/// Returns the name of a type as a string slice.
///
/// # Note
///
/// This is intended for diagnostic use. The exact contents and format of the
/// string returned are not specified, other than being a best-effort
/// description of the type. For example, amongst the strings
/// that `type_name::<Option<String>>()` might return are `"Option<String>"` and
/// `"std::option::Option<std::string::String>"`.
///
/// The returned string must not be considered to be a unique identifier of a
/// type as multiple types may map to the same type name. Similarly, there is no
/// guarantee that all parts of a type will appear in the returned string: for
/// example, lifetime specifiers are currently not included. In addition, the
/// output may change between versions of the compiler.
///
/// The current implementation uses the same infrastructure as compiler
/// diagnostics and debuginfo, but this is not guaranteed.
///
/// # Examples
///
/// ```rust
/// assert_eq!(
///     std::any::type_name::<Option<String>>(),
///     "core::option::Option<alloc::string::String>",
/// );
/// ```
#[must_use]
#[stable(feature = "type_name", since = "1.38.0")]
#[rustc_const_unstable(feature = "const_type_name", issue = "63084")]
pub const fn type_name<T: ?Sized>() -> &'static str {
    const { intrinsics::type_name::<T>() }
}

/// Returns the type name of the pointed-to value as a string slice.
///
/// This is the same as `type_name::<T>()`, but can be used where the type of a
/// variable is not easily available.
///
/// # Note
///
/// Like [`type_name`], this is intended for diagnostic use and the exact output is not
/// guaranteed. It provides a best-effort description, but the output may change between
/// versions of the compiler.
///
/// In short: use this for debugging, avoid using the output to affect program behavior. More
/// information is available at [`type_name`].
///
/// Additionally, this function does not resolve trait objects. This means that
/// `type_name_of_val(&7u32 as &dyn Debug)` may return `"dyn Debug"`, but will not return `"u32"`
/// at this time.
///
/// # Examples
///
/// Prints the default integer and float types.
///
/// ```rust
/// use std::any::type_name_of_val;
///
/// let s = "foo";
/// let x: i32 = 1;
/// let y: f32 = 1.0;
///
/// assert!(type_name_of_val(&s).contains("str"));
/// assert!(type_name_of_val(&x).contains("i32"));
/// assert!(type_name_of_val(&y).contains("f32"));
/// ```
#[must_use]
#[stable(feature = "type_name_of_val", since = "1.76.0")]
#[rustc_const_unstable(feature = "const_type_name", issue = "63084")]
pub const fn type_name_of_val<T: ?Sized>(_val: &T) -> &'static str {
    type_name::<T>()
}
