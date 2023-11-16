pub fn privacy_critical(sensitive_arg: i32) {
    let mut variable = 1;

    // Implicit flow.
    // if sensitive_arg > 0 {
    //     variable = 2;
    // }

    leak(variable);
}

pub fn leak(sensitive_arg: i32) {
    if sensitive_arg == 0 {
        println!("foo");
    }
}
