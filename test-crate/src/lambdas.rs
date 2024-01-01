#[doc = "pure"]
pub fn lambda_called(a: usize) -> usize {
    let l = |x| {
        return x * x;
    };

    l(a)
}

#[doc = "pure"]
pub fn lambda_uncalled(a: usize) -> usize {
    let _l = |x: usize| -> usize {
        return x * x;
    };
    a
}

// This works fine even with FnOnce or FnMut.
#[doc = "impure"]
pub fn execute<F: FnOnce(usize) -> usize>(x: usize, l: F) -> usize {
    l(x)
}

// This is an example of dynamic dispatch, which does not let compiler determine the type of l.
#[doc = "impure"]
pub fn execute_dyn(x: usize, l: &dyn Fn(usize) -> usize) -> usize {
    l(x)
}

#[doc = "pure"]
pub fn closure_test(a: usize) {
    let lambda = |x: usize| -> usize {
        return x * x;
    };

    let lambda_ref = |x: &usize| -> bool {
        return *x > 0;
    };

    let y = 42;
    let closure_capture = |x: usize| -> usize {
        return x * y;
    };

    let y = 42;
    let closure_capture_move = move |x: usize| -> usize {
        return x * y;
    };

    let y = 42;
    let ambiguous_lambda = if y > 5 {
        |x: usize| -> usize {
            return x;
        }
    } else {
        |x: usize| -> usize {
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

#[doc = "impure"]
pub fn partially_opaque(sensitive_attr: usize, flag: bool, l1: &dyn Fn(usize) -> usize) -> usize {
    let l2 = |x: usize| -> usize {
        return x * x;
    };

    let lambda = if flag {
        l1
    } else {
        &l2
    };

    lambda(sensitive_attr)
}

#[doc = "pure"]
pub fn resolved_partially_opaque(sensitive_attr: usize, flag: bool) -> usize {
    let lambda = |x: usize| -> usize {
        return x * x;
    };

    partially_opaque(sensitive_attr, flag, &lambda)
}
