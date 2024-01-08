trait Dynamic {
    fn inc(&self, a: usize) -> usize;
}

struct Foo;

struct Bar;

impl Dynamic for Foo {
    fn inc(&self, a: usize) -> usize {
        a + 1
    }
}

impl Dynamic for Bar {
    fn inc(&self, a: usize) -> usize {
        a + 2
    }
}

#[doc = "pure"]
fn eraser_outer(a: usize) -> usize {
    let dynamic: &dyn Dynamic = if a == 0 { &Foo {} } else { &Bar {} };
    eraser_inner(a, dynamic)
}

#[doc = "impure"]
fn eraser_inner(a: usize, dynamic: &dyn Dynamic) -> usize {
    dynamic.inc(a)
}
