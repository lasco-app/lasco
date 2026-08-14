pub(super) fn inclusive_slice<T>(items: &[T], start: usize, end: usize) -> Option<&[T]> {
    if start > end || start >= items.len() {
        return None;
    }
    items.get(start..=end.min(items.len() - 1))
}
