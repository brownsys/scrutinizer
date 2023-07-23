use uuid::Uuid;

pub fn foo(left: &mut usize, right: &mut usize) -> usize {
    let id = Uuid::new_v4();
    println!("{}", id);
    *left + *right
}

pub fn add(left: usize, right: usize) -> usize {
    let mut left = left;
    let mut right = right;
    foo(&mut left, &mut right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}