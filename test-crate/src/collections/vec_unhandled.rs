// None of those functions are monomorphic, so they would be rejected right away.

pub fn splice<R, I>(
  vec: &mut Vec<usize>,
  range: R,
  replace_with: I,
) -> Splice<'_, <I as IntoIterator>::IntoIter, Global>
where
  R: RangeBounds<usize>,
  I: IntoIterator<Item = usize>;

pub fn concat<Item>(&self) -> <[T] as Concat<Item>>::Output 
where
    [T]: Concat<Item>, Item: ?Sized;

pub fn join<Separator>(
    &self,
    sep: Separator
) -> <[T] as Join<Separator>>::Output 
where
    [T]: Join<Separator>;

pub fn rsplit<F>(&self, pred: F) -> RSplit<'_, T, F> 
where
    F: FnMut(&T) -> bool;

pub fn rsplit_mut<F>(&mut self, pred: F) -> RSplitMut<'_, T, F> 
where
    F: FnMut(&T) -> bool;

pub fn rsplitn<F>(&self, n: usize, pred: F) -> RSplitN<'_, T, F> 
where
    F: FnMut(&T) -> bool;

pub fn rsplitn_mut<F>(&mut self, n: usize, pred: F) -> RSplitNMut<'_, T, F> 
where
    F: FnMut(&T) -> bool;

pub fn split<F>(&self, pred: F) -> Split<'_, T, F> 
where
    F: FnMut(&T) -> bool;

pub fn split_inclusive<F>(&self, pred: F) -> SplitInclusive<'_, T, F> 
where
    F: FnMut(&T) -> bool;

pub fn split_inclusive_mut<F>(&mut self, pred: F) -> SplitInclusiveMut<'_, T, F> 
where
    F: FnMut(&T) -> bool;

pub fn split_mut<F>(&mut self, pred: F) -> SplitMut<'_, T, F> 
where
    F: FnMut(&T) -> bool;

pub fn splitn<F>(&self, n: usize, pred: F) -> SplitN<'_, T, F> 
where
    F: FnMut(&T) -> bool;

pub fn splitn_mut<F>(&mut self, n: usize, pred: F) -> SplitNMut<'_, T, F> 
where
    F: FnMut(&T) -> bool;

pub fn strip_prefix<P>(&self, prefix: &P) -> Option<&[T]>
where
    P: SlicePattern<Item = T> + ?Sized,
    T: PartialEq<T>;

pub fn strip_suffix<P>(&self, suffix: &P) -> Option<&[T]>
where
    P: SlicePattern<Item = T> + ?Sized,
    T: PartialEq<T>;