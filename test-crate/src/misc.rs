use chrono::naive::NaiveDateTime;
use std::cell::RefCell;
use std::io;
use std::net::UdpSocket;
use uuid::Uuid;

// Calling a function from a foreign crate.
#[doc = "pure"]
pub fn foreign_crate(left: usize, right: usize) -> usize {
    let _id = Uuid::new_v4();
    left + right
}

// Function with a side effect.
// #[doc = "impure"]
// pub fn println_side_effect(left: usize, right: usize) -> usize {
//     println!("{} {}", left, right);
//     left + right
// }

// Pure arithmetic function.
#[doc = "pure"]
pub fn add(left: usize, right: usize) -> usize {
    left + right
}

// Function with pure body but mutable arguments.
#[doc = "impure"]
pub fn add_mut(left: &mut usize, right: &mut usize) -> usize {
    *left + *right
}

// Function that calls a function that accepts arguments by mutable reference.
#[doc = "impure"]
pub fn add_mut_wrapper(left: &mut usize, right: &mut usize) -> usize {
    add_mut(left, right)
}

#[doc = "impure"]
pub fn udp_socket_send(socket: &UdpSocket, buf: &[u8]) -> io::Result<usize> {
    socket.send(buf)
}

#[doc = "impure"]
pub fn ref_cell_mut(refcell: &RefCell<usize>) {
    *refcell.borrow_mut() = 10;
}

#[doc = "pure"]
pub fn date_format(v: NaiveDateTime) -> String {
    v.format("%Y-%m-%d %H:%M:%S").to_string()
}

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
pub fn simple(a: usize) -> usize {
    let dynamic: &dyn Dynamic = if a == 0 { &Foo {} } else { &Bar {} };
    dynamic.inc(a)
}
