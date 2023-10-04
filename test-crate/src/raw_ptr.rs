// Raw mut pointer dereference.
pub unsafe fn raw_mut_ptr_deref() {
    let mut x = 42;
    let raw = &mut x as *mut i32;
    *raw = 5;
}

// Raw mut pointer aliasing.
pub unsafe fn raw_mut_ptr_mut_ref() {
    let mut x = 42;
    let raw = &mut x as *mut i32;
    let mut_ref = &mut *raw;
}

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
pub unsafe fn raw_mut_ptr_deref_into_call() {
    let mut x = Foo { x: 0 };
    let raw = &mut x as *mut Foo;
    (*raw).amend();
}

pub unsafe fn raw_mut_ptr_deref_outer() {
    raw_mut_ptr_deref();
    raw_mut_ptr_mut_ref();
    raw_mut_ptr_deref_into_call();
}

// Safe raw mut pointer dereference examples.
pub unsafe fn safe_raw_mut_ptr_deref<'a>() -> &'a i32 {
    let mut x = 42;
    let raw = &mut x as *mut i32;
    let immutable = *raw;
    &*raw
}

pub unsafe fn safe_raw_mut_ptr_deref_outer() {
    safe_raw_mut_ptr_deref();
}

// Raw const pointer dereference.
pub unsafe fn raw_const_ptr_deref() {
    let x = 42;
    let raw = &x as *const i32;
    let _points_at = *raw;
}

pub unsafe fn raw_const_ptr_deref_outer() {
    raw_const_ptr_deref();
}
