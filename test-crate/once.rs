fn std::ops::FnOnce::call_once(_1: Self, _2: Args) -> <Self as std::ops::FnOnce<Args>>::Output {
    let mut _0: <Self as std::ops::FnOnce<Args>>::Output; // return place in scope 0 at /Users/artemagvanian/.rustup/toolchains/nightly-2023-04-12-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/ops/function.rs:250:5: 250:71
    let _3: &mut Self;                   // in scope 0 at /Users/artemagvanian/.rustup/toolchains/nightly-2023-04-12-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/ops/function.rs:250:5: 250:71

    bb0: {
        _3 = &mut _1;                    // scope 0 at /Users/artemagvanian/.rustup/toolchains/nightly-2023-04-12-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/ops/function.rs:250:5: 250:71
        _0 = <Self as std::ops::FnMut<Args>>::call_mut(move _3, move _2) -> [return: bb1, unwind: bb3]; // scope 0 at /Users/artemagvanian/.rustup/toolchains/nightly-2023-04-12-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/ops/function.rs:250:5: 250:71
                                         // mir::Constant
                                         // + span: /Users/artemagvanian/.rustup/toolchains/nightly-2023-04-12-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/ops/function.rs:250:5: 250:71
                                         // + literal: Const { ty: for<'a> extern "rust-call" fn(&'a mut Self, Args) -> <Self as std::ops::FnOnce<Args>>::Output {<Self as std::ops::FnMut<Args>>::call_mut}, val: Value(<ZST>) }
    }

    bb1: {
        drop(_1) -> bb2;                 // scope 0 at /Users/artemagvanian/.rustup/toolchains/nightly-2023-04-12-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/ops/function.rs:250:5: 250:71
    }

    bb2: {
        return;                          // scope 0 at /Users/artemagvanian/.rustup/toolchains/nightly-2023-04-12-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/ops/function.rs:250:5: 250:71
    }

    bb3 (cleanup): {
        drop(_1) -> [return: bb4, unwind terminate]; // scope 0 at /Users/artemagvanian/.rustup/toolchains/nightly-2023-04-12-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/ops/function.rs:250:5: 250:71
    }

    bb4 (cleanup): {
        resume;                          // scope 0 at /Users/artemagvanian/.rustup/toolchains/nightly-2023-04-12-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/ops/function.rs:250:5: 250:71
    }
}
