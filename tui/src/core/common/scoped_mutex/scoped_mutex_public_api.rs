// Copyright (c) 2025 R3BL LLC. Licensed under Apache License, Version 2.0.

//! Deadlock safe [`Mutex`]. See [`scoped_mutex!`] and [`ScopedMutex`] for details.
//!
//! [`Mutex`]: std::sync::Mutex
//! [`scoped_mutex!`]: macro@crate::scoped_mutex
//! [`ScopedMutex`]: super::ScopedMutex

use super::deadlock_prevention::{DeadlockPreventionGuard, DeadlockPreventionPolicy};
use std::sync::{LockResult, Mutex, MutexGuard};

// XMARK: Clever Rust use ADT Const Params instead of trait based strategy (boilerplate)

/// Restricts access to the underlying [`Mutex`] via the [**Scoped
/// Access**](#scoped-access-friction-as-a-feature) pattern. This is different from the
/// [**Chain of Custody**](#comparison-with-monitor-chain-of-custody) pattern implemented
/// by [`Monitor`].
///
/// # Parameters
///
/// | Parameter               | Generic "kind"                                 |
/// | :---------------------- | :--------------------------------------------- |
/// | State of type `S`       | generic over type                              |
/// | Policy variant `POLICY` | generic over value ([read more](#type-theory)) |
///
/// # Construction
///
/// Use the [`scoped_mutex!`] macro to create a new instance. Don't instantiate this
/// struct directly (the `state` field is private to the module).
///
/// ```rust
/// use r3bl_tui::{scoped_mutex, ScopedMutex,
///     DeadlockPreventionPolicy::PanicOnAnyLockNesting
/// };
///
/// let scoped_mutex = scoped_mutex!(any, 10);
/// ```
///
/// # Why
///
/// In Rust it is too easy to lock a [`Mutex`] and while holding the [`MutexGuard`]
/// attempt to lock the same [`Mutex`] again, despite best efforts at lock hygiene. This
/// is especially easy to do when dealing with `static` [`Mutex`]es. Instead of rewriting
/// a reentrant mutex from scratch (which is a very big task), this module focuses on all
/// the use cases involving [`Mutex`] used in this codebase (which are generally
/// applicable to other Rust codebases) and make sure that these are covered for safety at
/// various levels.
///
/// # Scoped Access (Friction-as-a-Feature)
///
/// This struct is designed to prevent deadlocks by making it physically impossible to
/// hold a [`MutexGuard`] longer than the execution of a single closure. It does this by
/// hiding the [`lock()`] method and only providing access via closures.
///
/// The closure's scope *is* the lock's scope. Once the closure returns, the lock is
/// guaranteed to be released. This "friction" ensures that locks are short-lived and
/// reduces the risk of accidental deadlocks caused by keeping a guard in a local variable
/// across a long-running or blocking operation.
///
/// # Anti-Patterns: Recursive Locking
///
/// While [`ScopedMutex`] prevents deadlocks caused by "leaking" guards into long-lived
/// scopes, it **cannot** prevent deadlocks caused by recursion if you choose to opt-out
/// of recursion detection. If the closure passed to [`read()`] or [`write()`] attempts to
/// call any method on the **same** [`ScopedMutex`] instance using the [`OptOut`] policy,
/// it will deadlock.
///
/// ```no_run
/// use std::sync::{LazyLock, Mutex};
/// use r3bl_tui::{ScopedMutex, MutexExt, DeadlockPreventionPolicy::OptOut};
/// static HOT_PATH: LazyLock<
///     ScopedMutex<i32, { OptOut }>
/// > =
///     LazyLock::new(|| Mutex::new(0).into_scoped_mutex());
///
/// HOT_PATH.write(|_| {
///     HOT_PATH.read(|_| {}); // ❌ DEADLOCK! (Recursive call to the same static)
/// });
/// ```
///
/// To mitigate this, [`ScopedMutex`] includes **Recursion Detection**, via the `POLICY`
/// const generic parameter (generic over value), which allows you choose which variant
/// you want (with various degrees of protection and latitude), or opt-out entirely via
/// [`OptOut`]. When enabled, if a recursive lock is detected on the same thread, it will
/// panic with a clear message instead of hanging indefinitely.
///
/// # Design Decision: Why Const Generics?
///
/// We chose to make `POLICY` a const generic parameter (using [ADT Const Params]), which
/// is a value (an enum variant) and not a type, using:
///
/// <!-- It is ok to use ignore here because it is just a fragment of the actual code -->
///
/// ```ignore
/// struct ScopedMutex<S, const POLICY: DeadlockPreventionPolicy>
/// ````
///
/// This provides the best balance of ergonomics and performance.
///
/// 1. **Type-Level Identity without Boilerplate**: Using a value (that is a `const` enum
///    variant) is much simpler than the alternative trait-based pattern. The latter would
///    require defining a trait (for the policy), so that we can use it as a trait
///    constraint (we can't use an enum or struct here), then creating separate structs
///    for each policy, and implementing the trait for each struct. Const generics allow
///    us to centralize all logic within a single enum while maintaining type-level
///    distinction. We get the benefit of the latter without the boilerplate. Different
///    policies result in different **types**. This allows the compiler to enforce safety
///    rules at the type level, ensuring that a [`ScopedMutex`] with a strict policy
///    cannot be accidentally treated as one with a relaxed policy.
///
///    <!-- It is ok to use ignore here because this is a counter-example that uses
///    placeholders and intentionally differs from the actual implementation. -->
///
///    ```ignore
///    // ❌ Alternatives we avoided (The "Trait Pattern" boilerplate)
///    trait PolicyTrait { fn check(); }
///    struct Strict; impl PolicyTrait for Strict { fn check() { /* ... */ } }
///    struct OptOut; impl PolicyTrait for OptOut { fn check() { /* ... */ } }
///    struct ScopedMutex<S, P: PolicyTrait> { /* ... */ }
///    ```
///
/// 2. **Zero Runtime Overhead**: We could have used a regular field of type
///    [`DeadlockPreventionPolicy`] instead. However, this would require a runtime `if`
///    statement check at every [`read()`] or [`write()`] access to determine the policy.
///    By using the `const` generic, because the policy is known at compile-time, the
///    compiler can perform **dead-code elimination** and branch pruning. For example,
///    when using [`OptOut`], the recursion detection logic is entirely removed from the
///    compiled binary, making it as fast as a raw [`Mutex`].
///
///    <!-- It is ok to use ignore here because this is a counter-example that uses
///    placeholders and intentionally differs from the actual implementation. -->
///
///    ```ignore
///    // ❌ Alternatives we avoided (The "Runtime Check" overhead)
///    struct ScopedMutex<S> {
///        policy_field: DeadlockPreventionPolicy,
///        /* ... */
///    }
///    impl<S> ScopedMutex<S> {
///        fn read(&self) {
///            match self.policy_field { // ❌ Branching happens at runtime
///                DeadlockPreventionPolicy::PanicOnAnyLockNesting => { /* ... */ }
///                DeadlockPreventionPolicy::OptOut => { /* ... */ }
///                _ => { /* ... */ }
///            }
///        }
///    }
///    ```
///
/// # Type Theory
///
/// ## Terminology: Declaration, Definition, and Usage
///
/// First, let's establish a mental model for the syntactic structure of Rust code. This
/// is important for understanding how const generics bridge the gap between values and
/// types. We will distinguish between:
/// 1. **Declaration**: Where a generic is declared. This is where you define the
///    placeholders (parameters), e.g., `T` in `Vec<T>` or `N` in `[T; N]`.
/// 2. **Usage**: Where a generic is used. This is where the placeholders are bound to
///    concrete type and value arguments, e.g., `Vec<i32>` or `[i32; 10]`.
///
/// | Concept     | Mapping   | Role                                                          |
/// | :---------- | :-------- | :------------------------------------------------------------ |
/// | Declaration | Header    | The "What": Identifier, Types, and Parameters (placeholders). |
/// | Definition  | Body      | The "How": Implementation, Logic, and Storage.                |
/// | Usage       | Call Site | The "Where": Arguments (values) and Execution.                |
///
/// ### Parameters vs. Arguments
///
/// - Parameters belong to the Header and Body. They are the generic slots (like `S` and
///   `POLICY`) defined by the library.
/// - Arguments belong to the Call Site. They are the concrete Types (like `i32`) or
///   Values (like `{ DeadlockPreventionPolicy::OptOut }`) provided by the user.
///
/// ### Perspective: Variable vs. Expression
///
/// A single line of code often represents multiple concepts simultaneously. Let's examine
/// this line:
/// ```
/// let a = String::new();
/// ```
/// - Variable `a`: This line is its Declaration (creating the variable `a`) and its
///   Definition (providing the value via the assigned expression).
/// - Expression `String::new()`: This line is a Usage (Call Site).
///
/// We can break this line into two expressions to make this more explicit:
/// ```
/// let a;
/// a = String::new();
/// ```
///
/// ## Primer on "Values as Types" and "Generic over Values"
///
/// You are probably familiar with standard Rust generics, e.g., `Vec<T>`, and are used to
/// _generics over types_. In this example, `Vec` is generic over type `T`. The compiler
/// uses the **Type** `T` as a "coordinate" to create a concrete implementation via
/// monomorphization.
///
/// [ADT Const Params] allow us to use _generics over values_. Instead of providing a
/// type, we provide a specific value of an **Algebraic Data Type (ADT)**, e.g.,
/// `ScopedMutex<i32, { DeadlockPreventionPolicy::PanicOnAnyLockNesting }>`. The compiler
/// uses the **Value** of `{DeadlockPreventionPolicy::PanicOnAnyLockNesting}` (enum
/// variant) as a "coordinate" to create a concrete implementation via monomorphization.
///
/// 1. **Sum Types at the Type Level**: An `enum`, e.g., `enum { A, B, C }`, is known in
///    type theory as a **Sum Type** because its total state space is the sum of its
///    variants (`A + B + C`). By using an enum as a const generic, we "lift" this sum
///    from the value level (runtime) to the type level (compile-time).
/// 2. **Type Families**: [`ScopedMutex`] is not just one type; it is a **Type Family**.
///    Each variant of the policy enum acts as a coordinate that identifies a unique,
///    disjoint member of that family. To the compiler, the following are distinct and
///    different types (just like `i32` and `String` are different types):
///    - [`ScopedMutex<S,
///      {DeadlockPreventionPolicy::PanicOnAnyLockNesting}>`][`ScopedMutex`]
///    - [`ScopedMutex<S,
///      {DeadlockPreventionPolicy::PanicOnSpecificLockNesting}>`][`ScopedMutex`]
///    - [`ScopedMutex<S, {DeadlockPreventionPolicy::OptOut}>`][`ScopedMutex`]
/// 3. **The Algebra of Branch Pruning**: Because the "choice" in our **Sum Type** is
///    fixed at compile-time for any given member of the type family, the compiler can
///    apply algebraic simplification to our code. When it sees a `match` on a `const`
///    value, it "multiplies the unreachable branches by zero," physically removing them
///    from the binary. This is how we achieve **Zero Runtime Overhead**.
///
/// # The Shared Ledger
///
/// Each thread uses its own private [`SharedLedger`] to track every lock it acquires from
/// a "participating" [`ScopedMutex`] instance (unless it has opted out via
/// [`DeadlockPreventionPolicy::OptOut`]). This allows the system to enforce safety
/// policies across all participating [`ScopedMutex`] instances.
///
/// When you create a [`ScopedMutex`] variable, you have to declare a policy. However, the
/// enforcement of this policy does NOT occur in that variable; instead, it is enforced on
/// a per-thread basis for each thread that acquires a lock from this [`ScopedMutex`].
///
/// 1. Policy declaration: The [`DeadlockPreventionPolicy`] variant is provided as a
///    `const` argument on the [`ScopedMutex`] type family when you declare your variable.
/// 2. Per-thread enforcement: Each thread uses its own private [`SharedLedger`] to track
///    every lock it acquires from a [`ScopedMutex`] instance.
/// 3. Isolation: This ledger is **NOT** shared between different threads.
/// 4. Cross [`ScopedMutex`] instance sharing: Because the same ledger is used for all
///    locks acquired by a thread, the state machine can track nesting across multiple
///    different [`ScopedMutex`] instances. This is how deadlocks (recursive locking) are
///    detected and prevented.
///
/// See [`SharedLedger`] for details on how this ledger is implemented.
///
/// # Performance vs. Safety
///
/// The recursion detection uses a [`thread_local!`] check which has a negligible but
/// non-zero cost.
///
/// ## Safety-First: [`PanicOnAnyLockNesting`]
///
/// [`PanicOnAnyLockNesting`] is the most restrictive and safest setting. It ensures that
/// a thread can hold at most **one** lock at a time across **all** [`ScopedMutex`]
/// instances. This is enforced by the private [`SharedLedger`] each thread uses to track
/// every lock it acquires from a [`ScopedMutex`] instance.
///
/// Standard mutual exclusion rules still apply: Thread B can acquire [`ScopedMutex`] X
/// once Thread A releases it.
///
/// - Safe scenario: A thread can hold at most **one** lock at a time across all
///   participating [`ScopedMutex`] instances it acquires (including those using the
///   [`PanicOnSpecificLockNesting`] policy, with the exception of [`OptOut`] policy).
/// - Panic scenario: If Thread A is inside a closure for [`ScopedMutex`] X (holding its
///   lock) and tries to call [`read()`] or [`write()`] on [`ScopedMutex`] Y, the thread
///   will panic. Even though X and Y are different instances, and even if Y uses a
///   different safety policy (that is not [`OptOut`]), the private ledger ensures the
///   thread never holds more than one lock. This physically eliminates the risk of
///   circular wait deadlocks because a thread cannot hold one resource while waiting for
///   another. Since we don't need to maintain a complex graph of held locks, this is very
///   efficient, but not very discerning at all. It is a very blunt instrument, giving you
///   an all or nothing approach.
///
/// ```no_run
/// use std::sync::{LazyLock, Mutex};
/// use r3bl_tui::{ScopedMutex, MutexExt,
///     DeadlockPreventionPolicy::PanicOnAnyLockNesting
/// };
/// static SAFE_STAT: LazyLock<
///     ScopedMutex<i32, { PanicOnAnyLockNesting }>
/// > =
///     LazyLock::new(|| Mutex::new(0).into_scoped_mutex());
/// ```
///
/// Examples of [`PanicOnAnyLockNesting`] variant usage (typically global mutable
/// statics):
/// - [`SAVED_TERMIOS`].
/// - [`ROLLING_LOG_FILE_WRITER_GUARD`].
///
/// ## Flexible Safety: [`PanicOnSpecificLockNesting`]
///
/// [`PanicOnSpecificLockNesting`] allows a thread to nest different [`ScopedMutex`]
/// instances but panics if that thread tries to lock the **same** instance recursively.
///
/// This is enforced by the private [`SharedLedger`] each thread uses to track every lock
/// it acquires from a [`ScopedMutex`] instance. It works by tracking the memory addresses
/// of all currently held locks. These addresses are stable and the Rust borrow checker
/// enforces that they will not move and be invalidated for the scope of operations that
/// you can perform.
///
/// - Safe scenario: Thread A can safely hold a lock for [`ScopedMutex`] X and then
///   acquire a lock for [`ScopedMutex`] Y. The private ledger sees that address X and
///   address Y are different, so it allows the nesting. This allows composition of
///   different protected resources acquired by the same thread.
/// - Panic scenario: Thread A can't hold a lock for [`ScopedMutex`] X and then try to
///   acquire another lock for [`ScopedMutex`] X. The private ledger sees that address X
///   is already in its list of held locks and panics.
/// - Deadlock scenario: This policy does **not** prevent circular wait deadlocks between
///   **different** threads and **different** [`ScopedMutex`] instances. For example, if
///   Thread A holds [`ScopedMutex`] X and tries to acquire [`ScopedMutex`] Y, while
///   Thread B holds [`ScopedMutex`] Y and tries to acquire [`ScopedMutex`] X, a deadlock
///   will occur. This is why [`PanicOnAnyLockNesting`] is preferred for global singletons
///   where nesting is never required.
///
/// ```no_run
/// use std::sync::{LazyLock, Mutex};
/// use r3bl_tui::{ScopedMutex, MutexExt,
///     DeadlockPreventionPolicy::PanicOnSpecificLockNesting
/// };
/// static SAFE_STAT: LazyLock<
///     ScopedMutex<i32, { PanicOnSpecificLockNesting }>
/// > =
///     LazyLock::new(|| Mutex::new(0).into_scoped_mutex());
/// ```
///
/// Example of [`PanicOnSpecificLockNesting`] variant usage (typically struct members):
/// - [`OutputDevice`].
/// - [`Readline`].
///
/// ## Performance-Critical: [`DeadlockPreventionPolicy::OptOut`] (Opt-out)
///
/// For performance-critical hot paths (like a render-loop cache), you can opt-out of this
/// check at compile-time by setting [`POLICY`] to [`DeadlockPreventionPolicy::OptOut`].
/// This entirely removes the [`thread_local!`] check from the generated code. This policy
/// is "invisible" to the shared thread-local ledger ([`SharedLedger`]); it neither checks
/// nor updates it, its "off the books".
///
/// - Safe scenario: A thread can acquire a [`DeadlockPreventionPolicy::OptOut`] lock
///   while holding a [`PanicOnAnyLockNesting`] or [`PanicOnSpecificLockNesting`] lock
///   (and vice-versa). Since the shared ledger ([`SharedLedger`]) is neither checked nor
///   updated, the thread can hold multiple locks simultaneously without triggering a
///   panic.
/// - Deadlock scenario 1: If a thread tries to lock the same
///   [`DeadlockPreventionPolicy::OptOut`] instance recursively, it will hang forever
///   (deadlock) because the [`SharedLedger`] is not used to detect the recursion and
///   trigger a panic.
/// - Deadlock scenario 2: This policy provides zero protection against circular wait
///   deadlocks between different threads.
///
/// ```no_run
/// use std::sync::{LazyLock, Mutex};
/// use r3bl_tui::{ScopedMutex, DeadlockPreventionPolicy::OptOut, MutexExt};
/// // Opt-out of recursion detection for maximum performance.
/// static HOT_PATH: LazyLock<
///     ScopedMutex<i32, { OptOut }>
/// > =
///     LazyLock::new(|| Mutex::new(0).into_scoped_mutex());
/// ```
///
/// Example of [`DeadlockPreventionPolicy::OptOut`] variant usage:
/// - [`DYNAMIC_CACHE`].
///
/// # Safety Policies & Composition (Mixed Policies)
///
/// When mixing different safety policies on the same thread, [`ScopedMutex`] enforces the
/// **strongest active constraint**. This is handled by the unified [`SharedLedger`] state
/// machine.
///
/// ## 1. [`PanicOnAnyLockNesting`] Vetoes All Others
///
/// If a thread already holds a lock with [`PanicOnAnyLockNesting`], attempting to acquire
/// *any* other lock (even one with a more relaxed policy like
/// [`PanicOnSpecificLockNesting`]) will panic. This ensures the "zero nesting" promise of
/// the first lock is never violated.
///
/// ## 2. [`PanicOnAnyLockNesting`] Cannot Be Nested
///
/// If a thread already holds one or more locks with [`PanicOnSpecificLockNesting`],
/// attempting to acquire a lock with [`PanicOnAnyLockNesting`] will panic. This prevents
/// "sneaking" a nested lock into a strict block.
///
/// ## 3. Deadlock Prevention (Circular Wait)
///
/// A critical benefit of this composition is that it prevents **Circular Wait deadlocks**
/// among all locks using the [`PanicOnAnyLockNesting`] policy. Since a thread can hold at
/// most one such lock, it can never "hold A while waiting for B" if both use this policy.
///
/// # Comparison with [`Monitor`] (Chain of Custody)
///
/// | Feature             | [`ScopedMutex`] (Scoped Access) | [`Monitor`] (Chain of Custody)    |
/// | :------------------ | :------------------------------ | :-------------------------------- |
/// | **Primary Goal**    | Simple shared state             | Complex state machines            |
/// | **Synchronization** | [`Mutex`] only                  | [`Mutex`] + [`Condvar`]           |
/// | **Access Pattern**  | Closure-based                   | Guard-based (move-by-value)       |
/// | **Deadlock Safety** | Structural (via closures)       | Protocol-based (chain of custody) |
/// | **Use Case**        | Global settings, single stats   | RRT engine, thread coordination   |
///
/// - **Use [`ScopedMutex`]**: When you just need to safely read or write a shared value
///   and want to ensure the lock is never held longer than necessary.
/// - **Use [`Monitor`]**: When you need to coordinate between threads (using [`wait()`]
///   or [`notify_all()`]). See the [Chain of Custody] section in [`Monitor`] for details.
///
/// # Poison Safety
///
/// This struct follows the crate's **Resilience over Integrity** philosophy.
/// - [`Self::read()`] and [`Self::write()`]: Fail-fast (panic on poisoning). Use these
///   for normal application logic.
/// - [`Self::lock_raw()`]: Poison-safe (returns raw [`LockResult`]). Use this for
///   **cleanup paths** (like [`Drop`] or terminal restoration) where you must attempt to
///   proceed even if the state is dirty.
///
/// See the [Terminal Restoration: Panic, Drop, and Mutex Poison-Safety] section in the
/// crate root documentation for details.
///
/// [`Condvar::wait()`]: std::sync::Condvar::wait
/// [`Condvar`]: std::sync::Condvar
/// [`DYNAMIC_CACHE`]: crate::core::common::string_repeat_cache::DYNAMIC_CACHE
/// [`lock()`]: std::sync::Mutex::lock
/// [`lock_raw()`]: Self::lock_raw()
/// [`Monitor`]: crate::core::common::Monitor
/// [`Mutex`]: std::sync::Mutex
/// [`MutexExt`]: super::MutexExt
/// [`MutexGuard`]: std::sync::MutexGuard
/// [`notify_all()`]: crate::core::common::Monitor::notify_all
/// [`OptOut`]: DeadlockPreventionPolicy::OptOut
/// [`OutputDevice`]: crate::OutputDevice
/// [`PanicOnAnyLockNesting`]: DeadlockPreventionPolicy::PanicOnAnyLockNesting
/// [`PanicOnSpecificLockNesting`]: DeadlockPreventionPolicy::PanicOnSpecificLockNesting
/// [`POLICY: DeadlockPreventionPolicy`]: DeadlockPreventionPolicy
/// [`POLICY`]: DeadlockPreventionPolicy
/// [`read()`]: Self::read()
/// [`Readline`]: crate::Readline
/// [`ROLLING_LOG_FILE_WRITER_GUARD`]:
///     crate::core::log::rolling_file_appender_impl::ROLLING_LOG_FILE_WRITER_GUARD
/// [`SAVED_TERMIOS`]: crate::core::ansi::terminal_raw_mode::raw_mode_unix::SAVED_TERMIOS
/// [`scoped_mutex!`]: macro@crate::scoped_mutex
/// [`SharedLedger`]: crate::SharedLedger
/// [`wait()`]: crate::core::common::Monitor::wait
/// [`write()`]: Self::write()
/// [ADT Const Params]:
///     https://doc.rust-lang.org/nightly/unstable-book/language-features/adt-const-params.html
/// [Chain of Custody]:
///     crate::core::common::Monitor#chain-of-custody-friction-as-a-feature
/// [Scoped Access]: #scoped-access-friction-as-a-feature
/// [Terminal Restoration: Panic, Drop, and Mutex Poison-Safety]:
///     crate#terminal-restoration-panic-drop-and-mutex-poison-safety
#[derive(Debug)]
pub struct ScopedMutex<S, const POLICY: DeadlockPreventionPolicy> {
    /// Underlying [`Mutex`] protecting the state `S`.
    ///
    /// This field is hidden from code outside of this module, in order to discourage
    /// direct instantiation of this struct - instead use [`scoped_mutex!`] macro.
    ///
    /// [`scoped_mutex!`]: macro@crate::scoped_mutex
    pub(super) state: Mutex<S>,
}

/// Methods for accessing the protected state according to the chosen `POLICY`, which is a
/// `const` generic parameter holding a value (that is a variant of the
/// [`DeadlockPreventionPolicy`] enum).
impl<S, const POLICY: DeadlockPreventionPolicy> ScopedMutex<S, POLICY> {
    /// Provides read-only access to the protected state via a closure.
    ///
    /// The lock is acquired before the closure is called and released immediately after
    /// it returns.
    ///
    /// # Panics
    ///
    /// - Panics if the internal mutex is poisoned.
    /// - Panics if a recursive lock is detected (if `POLICY` is not [`OptOut`]).
    ///
    /// [`OptOut`]: DeadlockPreventionPolicy::OptOut
    pub fn read<F, R>(&self, fun: F) -> R
    where
        F: FnOnce(&S) -> R,
    {
        let _recursion_guard = DeadlockPreventionGuard::new(self);
        let state_guard = self.state.lock().unwrap();
        fun(&*state_guard)
    }

    /// Provides read-write access to the protected state via a closure.
    ///
    /// The lock is acquired before the closure is called and released immediately after
    /// it returns.
    ///
    /// # Panics
    ///
    /// - Panics if the internal mutex is poisoned.
    /// - Panics if a recursive lock is detected (if `POLICY` is not [`OptOut`]).
    ///
    /// [`OptOut`]: DeadlockPreventionPolicy::OptOut
    pub fn write<F, R>(&self, fun: F) -> R
    where
        F: FnOnce(&mut S) -> R,
    {
        let _recursion_guard = DeadlockPreventionGuard::new(self);
        let mut state_guard = self.state.lock().unwrap();
        fun(&mut *state_guard)
    }

    /// Provides raw access to the internal mutex, returning the [`LockResult`].
    ///
    /// This is a **poison-safe** alternative to [`Self::read()`] and [`Self::write()`]
    /// specifically designed for **cleanup paths**. The closure receives a
    /// [`LockResult`], which allows retrieving the protected data even if the mutex is
    /// poisoned, via [`into_inner()`].
    ///
    /// This method **bypasses** recursion detection to ensure that cleanup or terminal
    /// restoration can attempt to proceed even in complex failure states.
    ///
    /// [`into_inner()`]: std::sync::PoisonError::into_inner
    pub fn lock_raw<'this, F, R>(&'this self, fun: F) -> R
    where
        F: FnOnce(LockResult<MutexGuard<'this, S>>) -> R,
    {
        fun(self.state.lock())
    }

    /// Returns the memory address of this [`ScopedMutex`] instance.
    ///
    /// This address is used by the [`SharedLedger`] to track which specific locks are
    /// held by a thread, enabling detection of recursive locking on the same instance.
    ///
    /// [`SharedLedger`]: crate::SharedLedger
    #[must_use]
    pub fn get_address(&self) -> usize { std::ptr::from_ref(self) as usize }
}

// 1. Any policy tests.
#[cfg(test)]
mod tests_any {
    use super::test_fixtures::assert_child_process_success;
    use crate::{extract_panic_message, generate_isolated_process_test, scoped_mutex};
    use std::{process::Stdio, sync::Arc};

    generate_isolated_process_test!(
        test_any_policy_in_isolated_process,
        assert_child_process_success,
        controlled_fn,
        /* stdin */ Stdio::null(),
        /* stdout */ Stdio::piped(),
        /* stderr */ Stdio::piped()
    );

    fn controlled_fn() {
        test_scoped_mutex_new();
        test_scoped_mutex_read_write();
        test_scoped_mutex_poisoning_recovery();
        test_scoped_mutex_scoped_access_prevents_deadlock();
        test_scoped_mutex_any_recursion_detection_panics();
        test_scoped_mutex_lock_raw_bypasses_recursion_detection();
        test_scoped_mutex_lock_raw_on_poisoned_mutex_succeeds();
    }

    fn test_scoped_mutex_new() {
        let sm = scoped_mutex!(any, 42);

        // Read.
        let val = sm.read(|&it| it);
        assert_eq!(val, 42);
    }

    fn test_scoped_mutex_read_write() {
        let sm = scoped_mutex!(any, 10);

        // Write.
        sm.write(|it| *it += 5);

        // Read.
        let val = sm.read(|&it| it);
        assert_eq!(val, 15);
    }

    fn test_scoped_mutex_poisoning_recovery() {
        let sm = Arc::new(scoped_mutex!(any, 0));

        // 1. Poison the mutex.
        let result_poisoned = std::thread::spawn({
            let sm_clone = Arc::clone(&sm);
            move || {
                sm_clone.write(|it| {
                    *it = 42;
                    panic!("Intentional panic to poison mutex");
                });
            }
        })
        .join();
        assert_eq!(
            extract_panic_message(result_poisoned),
            "Intentional panic to poison mutex"
        );

        // 2. Normal read/write should now panic (fail-fast).
        let result = std::panic::catch_unwind(|| {
            sm.read(|&it| it);
        });
        assert!(extract_panic_message(result).contains("PoisonError"));

        // 3. Recovery using lock_raw.
        sm.lock_raw(|result| match result {
            Ok(_) => unreachable!("The lock should be poisoned"),
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                assert_eq!(*guard, 42);
                *guard = 100;
            }
        });

        // 4. Verify new value in still-poisoned mutex.
        sm.lock_raw(|result| {
            let guard = result.unwrap_err().into_inner();
            assert_eq!(*guard, 100);
        });
    }

    fn test_scoped_mutex_scoped_access_prevents_deadlock() {
        let scoped_mutex = Arc::new(scoped_mutex!(any, 0));

        // This would deadlock if we held the lock across the inner block, but ScopedMutex
        // ensures the lock is released as soon as the closure returns.
        scoped_mutex.write(|s| *s = 1);

        // We can immediately access it again.
        scoped_mutex.write(|s| *s = 2);

        assert_eq!(scoped_mutex.read(|&s| s), 2);
    }

    fn test_scoped_mutex_any_recursion_detection_panics() {
        let scoped_mutex_1 = scoped_mutex!(any, 0);
        let scoped_mutex_2 = scoped_mutex!(any, 0);

        // 1. Recursive write on SAME instance should panic.
        let result = std::panic::catch_unwind(|| {
            scoped_mutex_1.write(|_| {
                scoped_mutex_1.read(|_| {});
            });
        });
        assert!(result.is_err());

        // 2. Nested write on DIFFERENT instance should ALSO panic.
        let result = std::panic::catch_unwind(|| {
            scoped_mutex_1.write(|_| {
                scoped_mutex_2.read(|_| {});
            });
        });
        assert!(result.is_err());
    }

    fn test_scoped_mutex_lock_raw_bypasses_recursion_detection() {
        let scoped_mutex_1 = scoped_mutex!(any, 0);
        let scoped_mutex_2 = scoped_mutex!(any, 0);

        scoped_mutex_1.write(|_| {
            scoped_mutex_2.lock_raw(|result| {
                assert!(result.is_ok());
            });
        });
    }

    fn test_scoped_mutex_lock_raw_on_poisoned_mutex_succeeds() {
        let scoped_mutex = Arc::new(scoped_mutex!(any, 0));
        let scoped_mutex_clone = Arc::clone(&scoped_mutex);

        // 1. Poison the mutex.
        let _unused = std::thread::spawn(move || {
            scoped_mutex_clone.write(|s| {
                *s = 42;
                panic!("Intentional panic to poison mutex");
            });
        })
        .join();

        // 2. lock_raw should succeed and allow access to the data.
        scoped_mutex.lock_raw(|result| {
            assert!(result.is_err());
            let guard = result.unwrap_err().into_inner();
            assert_eq!(*guard, 42);
        });
    }
}

// 2. Specific policy tests.
#[cfg(test)]
mod tests_specific {
    use super::test_fixtures::assert_child_process_success;
    use crate::{extract_panic_message, generate_isolated_process_test, scoped_mutex};
    use std::process::Stdio;

    generate_isolated_process_test!(
        test_specific_policy_in_isolated_process,
        assert_child_process_success,
        controlled_fn,
        /* stdin */ Stdio::null(),
        /* stdout */ Stdio::piped(),
        /* stderr */ Stdio::piped()
    );

    fn controlled_fn() {
        test_scoped_mutex_specific_recursion_detection();
        test_scoped_mutex_recursion_detection_opt_out_compiles();
    }

    fn test_scoped_mutex_specific_recursion_detection() {
        let scoped_mutex_1 = scoped_mutex!(specific, 0);
        let scoped_mutex_2 = scoped_mutex!(specific, 0);

        // 1. Recursive write on SAME instance should panic.
        let result = std::panic::catch_unwind(|| {
            scoped_mutex_1.write(|_| {
                scoped_mutex_1.read(|_| {});
            });
        });
        assert!(result.is_err());

        // 2. Nested write on DIFFERENT instance should SUCCEED.
        let result = std::panic::catch_unwind(|| {
            scoped_mutex_1.lock_raw(|result_1| {
                let mut guard_1 =
                    result_1.unwrap_or_else(std::sync::PoisonError::into_inner);
                scoped_mutex_2.read(|_| {
                    scoped_mutex_2.read(|_| {}); // This should panic (recursion on 2)
                });
                *guard_1 = 1;
            });
        });
        assert!(
            extract_panic_message(result)
                .contains("Recursive lock detected on ScopedMutex")
        );

        // 3. Nested write on DIFFERENT instance should SUCCEED. We need to use lock_raw
        // here because the mutexes were poisoned by the panics above.
        scoped_mutex_1.lock_raw(|result_1| {
            assert!(result_1.is_err());
            scoped_mutex_2.lock_raw(|result_2| {
                assert!(result_2.is_err());
            });
        });
    }

    fn test_scoped_mutex_recursion_detection_opt_out_compiles() {
        // Opt-out of recursion detection.
        let hot_mutex = scoped_mutex!(opt_out, 42);

        // Verify normal functionality works.
        let val = hot_mutex.read(|&s| s);
        assert_eq!(val, 42);

        hot_mutex.write(|s| *s = 100);
        assert_eq!(hot_mutex.read(|&s| s), 100);
    }
}

// 3. Mixed policy tests.
#[cfg(test)]
mod tests_mixed {
    use super::test_fixtures::assert_child_process_success;
    use crate::{extract_panic_message, generate_isolated_process_test, scoped_mutex};
    use std::process::Stdio;

    generate_isolated_process_test!(
        test_mixed_policy_in_isolated_process,
        assert_child_process_success,
        controlled_fn,
        /* stdin */ Stdio::null(),
        /* stdout */ Stdio::piped(),
        /* stderr */ Stdio::piped()
    );

    fn controlled_fn() { test_scoped_mutex_mixed_recursion_cross_check(); }

    fn test_scoped_mutex_mixed_recursion_cross_check() {
        // Scenario 1: Holding Specific then trying to hold Any.
        {
            let mutex_spec = scoped_mutex!(specific, 1);
            let mutex_any = scoped_mutex!(any, 42);

            let result = std::panic::catch_unwind(|| {
                mutex_spec.read(|_| {
                    mutex_any.read(|_| {});
                });
            });
            assert!(
                extract_panic_message(result)
                    .contains("PanicOnAnyLockNesting forbids ANY nesting")
            );
        }

        // Scenario 2: Holding Any then trying to hold Specific.
        {
            let mutex_any = scoped_mutex!(any, 42);
            let mutex_spec = scoped_mutex!(specific, 2);

            let result = std::panic::catch_unwind(|| {
                mutex_any.read(|_| {
                    mutex_spec.read(|_| {});
                });
            });
            assert!(
                extract_panic_message(result)
                    .contains("Cannot acquire a Specific lock while an Any lock is held")
            );
        }
    }
}

// Test fixtures.
#[cfg(test)]
mod test_fixtures {
    use std::process::Output;

    pub fn assert_child_process_success(child_process_output: Output) {
        if !child_process_output.status.success() {
            let stderr = String::from_utf8_lossy(&child_process_output.stderr);
            let stdout = String::from_utf8_lossy(&child_process_output.stdout);
            eprintln!("Isolated test failed!");
            eprintln!("Exit status: {:?}", child_process_output.status);
            eprintln!("Stdout: {stdout}");
            eprintln!("Stderr: {stderr}");
            panic!("Isolated test failed");
        }
    }
}
