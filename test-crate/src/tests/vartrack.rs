mod leaky_flows {
    #[doc = "impure"]
    pub fn implicit_leak(sensitive_arg: i32) {
        let mut variable = 1;
        // Implicit flow.
        if sensitive_arg > 0 {
            variable = 2;
        }
        println!("{}", variable);
    }

    #[doc = "impure"]
    pub fn reassignment_leak(sensitive_arg: i32) {
        let mut variable = sensitive_arg;
        // Implicit flow.
        if variable > 0 {
            variable = 2;
        }
        println!("{}", variable);
    }
}

mod arc_leak {
    use std::sync::{Arc, Mutex};

    #[doc = "impure"]
    pub fn arc_leak(sensitive_arg: i32) {
        let sensitive_arc = Arc::new(Mutex::new(sensitive_arg));
        let sensitive_arc_copy = sensitive_arc.clone();
        let unwrapped = *sensitive_arc_copy.lock().unwrap();
        println!("{}", unwrapped);
    }
}

mod tricky_flows {
    #[doc = "impure"]
    pub fn implicit_leak(sensitive_arg: i32) {
        let mut variable = 1;
        // Implicit flow.
        if variable > 0 {
            variable = 2;
        }
        println!("{}", variable);
        if sensitive_arg > 0 {
            variable = 2;
        }
        // This call needs to be revisited.
        println!("{}", variable);
    }
}

mod non_leaky_flows {
    #[doc = "pure"]
    pub fn foo(sensitive_arg: i32) {
        let mut variable = 1;
        // Implicit flow.
        if variable > 0 {
            variable = 2;
        }
        println!("{}", variable);
    }
}
