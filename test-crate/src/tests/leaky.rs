mod print {
    #[doc = "impure"]
    pub fn println_side_effect(left: usize, right: usize) -> usize {
        println!("{} {}", left, right);
        left + right
    }
}

mod network {
    use std::io;
    use std::net::UdpSocket;

    #[doc = "impure"]
    pub fn udp_socket_send(socket: &UdpSocket, buf: &[u8]) -> io::Result<usize> {
        socket.send(buf)
    }
}

mod interior {
    use std::cell::RefCell;

    #[doc = "impure"]
    pub fn ref_cell_mut(refcell: &RefCell<usize>) {
        *refcell.borrow_mut() = 10;
    }
}

mod implicit {
    struct CustomSmartPointer {
        data: usize,
    }

    impl Drop for CustomSmartPointer {
        fn drop(&mut self) {
            println!("Dropping CustomSmartPointer with data `{}`!", self.data);
        }
    }

    #[doc = "impure"]
    pub fn sneaky_drop(data: usize) {
        let sp = CustomSmartPointer { data };
    }
}
