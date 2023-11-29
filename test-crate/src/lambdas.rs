use std::marker::Destruct;

pub fn lambda_called(a: usize) -> usize {
    let l = |x| {
        return x * x;
    };

    l(a)
}

pub fn lambda_uncalled(a: usize) -> usize {
    let _l = |x: usize| -> usize {
        return x * x;
    };
    a
}

// This works fine even with FnOnce or FnMut.
pub fn execute<F: FnOnce(usize) -> usize>(x: usize, l: F) -> usize {
    l(x)
}

// This example *might* drop types, but it doesn't happen in this case.
#[inline]
#[must_use]
pub const fn execute_destruct<T, F: ~ const FnOnce(&T) -> bool>(x: T, l: F) -> bool
    where
        T: ~ const Destruct,
        F: ~ const Destruct,
{
    l(&x)
}

// This is an example of dynamic dispatch, which does not let compiler determine the type of l.
pub fn execute_dyn(x: usize, l: &dyn Fn(usize) -> usize) -> usize {
    l(x)
}

pub fn closure_test(a: usize) {
    let lambda = |x: usize| -> usize {
        return x * x;
    };

    let lambda_ref = |x: &usize| -> bool {
        return *x > 0;
    };

    let y = 42;
    let closure_capture = |x: usize| -> usize  {
        return x * y;
    };

    let y = 42;
    let closure_capture_move = move |x: usize| -> usize  {
        return x * y;
    };

    let y = 42;
    let ambiguous_lambda = if y > 5 {
        |x: usize| -> usize  {
            return x;
        }
    } else {
        |x: usize| -> usize  {
            return x * x;
        }
    };

    // execute(a, lambda);
    // execute_destruct(a, lambda_ref);
    execute_dyn(a, &lambda);
    // execute(a, closure_capture);
    // execute(a, closure_capture_move);
    // execute(a, ambiguous_lambda);
}
