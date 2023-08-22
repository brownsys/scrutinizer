use uuid::Uuid;

pub fn foreign_crate(left: usize, right: usize) -> usize {
    let _id = Uuid::new_v4();
    left + right
}

pub fn println_side_effect(left: usize, right: usize) -> usize {
    println!("{} {}", left, right);
    left + right
}

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

pub fn add_mut(left: &mut usize, right: &mut usize) -> usize {
    *left + *right
}

pub fn add_mut_wrapper(left: &mut usize, right: &mut usize) -> usize {
    add_mut(left, right)
}

pub fn contains(haystack: &Vec<usize>, needle: &usize) -> bool {
    haystack.contains(needle)
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