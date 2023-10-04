use std::cell::RefCell;
use std::io;
use std::net::UdpSocket;
use uuid::Uuid;

// Calling a function from a foreign crate.
pub fn foreign_crate(left: usize, right: usize) -> usize {
    let _id = Uuid::new_v4();
    left + right
}

// Function with a side effect.
pub fn println_side_effect(left: usize, right: usize) -> usize {
    println!("{} {}", left, right);
    left + right
}

// Pure arithmetic function.
pub fn add(left: usize, right: usize) -> usize {
    left + right
}

// Function with pure body but mutable arguments.
pub fn add_mut(left: &mut usize, right: &mut usize) -> usize {
    *left + *right
}

// Function that calls a function that accepts arguments by mutable reference.
pub fn add_mut_wrapper(left: &mut usize, right: &mut usize) -> usize {
    add_mut(left, right)
}

pub fn udp_socket_send(socket: &UdpSocket, buf: &[u8]) -> io::Result<usize> {
    socket.send(buf)
}

pub fn ref_cell_mut(refcell: &RefCell<usize>) {
    *refcell.borrow_mut() = 10;
}
