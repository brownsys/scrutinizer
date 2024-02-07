use itertools::Itertools;

pub fn transpose<T>(v: Vec<Vec<T>>) -> Vec<Vec<T>> {
    let len = v[0].len();
    let mut iters: Vec<_> = v.into_iter().map(|n| n.into_iter()).collect_vec();
    (0..len)
        .map(|_| iters.iter_mut().map(|n| n.next().unwrap()).collect_vec())
        .collect_vec()
}