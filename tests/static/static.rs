#![allow(dead_code)]

mod mutable_static {
    static mut GLOBAL_VEC: Vec<u32> = vec![];

    #[doc = "impure"]
    fn leak_into_static(a: u32) {
        unsafe {
            GLOBAL_VEC.push(a);
        }
    }
}

mod mutation_from_static {
    struct PureIncrementer;

    impl PureIncrementer {
        fn inc(&self, a: usize) -> usize {
            a + 1
        }
    }

    struct ImpureIncrementer;

    impl ImpureIncrementer {
        fn inc(&self, a: usize) -> usize {
            println!("{}", a);
            a + 1
        }
    }

    static PURE_INCREMENTER: PureIncrementer = PureIncrementer {};
    static IMPURE_INCREMENTER: ImpureIncrementer = ImpureIncrementer {};

    #[doc = "pure"]
    fn pure_call_from_static(a: usize) -> usize {
        PURE_INCREMENTER.inc(a)
    }

    #[doc = "impure"]
    fn impure_call_from_static(a: usize) -> usize {
        IMPURE_INCREMENTER.inc(a)
    }
}

fn main() {}
