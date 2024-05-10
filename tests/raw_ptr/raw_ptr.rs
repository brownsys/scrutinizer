#![allow(dead_code)]

mod raw_mut_ptr {
    // Raw mut pointer dereference.
    #[doc = "impure"]
    pub unsafe fn raw_mut_ptr_deref(a: usize) {
        let mut x = 42;
        let raw = &mut x as *mut i32;
        *raw = 5;
    }

    // Raw mut pointer aliasing.
    #[doc = "impure"]
    pub unsafe fn raw_mut_ptr_mut_ref(a: usize) {
        let mut x = 42;
        let raw = &mut x as *mut i32;
        let mut_ref = &mut *raw;
    }
}

mod raw_mut_ptr_call {
    #[derive(Debug)]
    struct Foo {
        x: i32
    }

    impl Foo {
        fn amend(&mut self) {
            self.x = 42;
        }
    }
    // Raw mut pointer dereference into call.
    #[doc = "impure"]
    pub unsafe fn raw_mut_ptr_deref_into_call(a: usize) {
        let mut x = Foo { x: 0 };
        let raw = &mut x as *mut Foo;
        (*raw).amend();
    }
}

mod safe_raw_mut_ptr {
    // Safe raw mut pointer dereference examples.
    #[doc = "pure"]
    pub unsafe fn safe_raw_mut_ptr_deref<'a>(a: usize) -> &'a i32 {
        let mut x = 42;
        let raw = &mut x as *mut i32;
        let immutable = *raw;
        &*raw
    }
}

mod raw_const_ptr {
    // Raw const pointer dereference.
    #[doc = "pure"]
    pub unsafe fn raw_const_ptr_deref(a: usize) {
        let x = 42;
        let raw = &x as *const i32;
        let _points_at = *raw;
    }
}

fn main() {}
