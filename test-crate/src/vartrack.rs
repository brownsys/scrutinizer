use std::sync::{Arc, Mutex};

pub fn privacy_critical(sensitive_arg: i32) {
    let mut variable = 1;

    // Implicit flow.
    if sensitive_arg > 0 {
        variable = 2;
    }

    leak(variable);
}

pub fn sneaky_arc(sensitive_arg: i32) {
    let sensitive_arc = Arc::new(Mutex::new(sensitive_arg));
    let sensitive_arc_copy = sensitive_arc.clone();
    let unwrapped = *sensitive_arc_copy.lock().unwrap();
    leak(unwrapped);
}

pub fn leak(sensitive_arg: i32) {
    if sensitive_arg == 0 {
        println!("foo");
    }
}
