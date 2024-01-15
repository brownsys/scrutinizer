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

// Type eraser for arbitrary objects.

#[doc = "pure"]
fn eraser_outer(a: usize) -> usize {
    let dynamic: &dyn Dynamic = if a == 0 { &Foo {} } else { &Bar {} };
    eraser_inner(a, dynamic)
}

#[doc = "impure"]
fn eraser_inner(a: usize, dynamic: &dyn Dynamic) -> usize {
    dynamic.inc(a)
}

// Type eraser in return position.

#[doc = "pure"]
fn eraser_ret_outer(a: usize) -> usize {
    let cl = eraser_ret_hof(a);
    eraser_ret_executor(a, &cl)
}

#[doc = "pure"]
fn eraser_ret_executor(a: usize, cl: &dyn Fn(usize) -> usize) -> usize {
    cl(a)
}

#[doc = "pure"]
fn eraser_ret_hof(a: usize) -> impl Fn(usize) -> usize {
    move |x| x + a
}

// Type eraser with upvars.

#[doc = "pure"]
fn eraser_upvar_outer(a: usize) -> usize {
    let lam = |x| x + 1;
    let cl = eraser_upvar_hof(a, &lam);
    eraser_upvar_executor(a, &cl)
}

#[doc = "pure"]
fn eraser_upvar_executor(a: usize, cl: &dyn Fn(usize) -> usize) -> usize {
    cl(a)
}

#[doc = "pure"]
fn eraser_upvar_hof(a: usize, cl: &dyn Fn(usize) -> usize) -> impl Fn(usize) -> usize + '_ {
    move |x| cl(x + a)
}
