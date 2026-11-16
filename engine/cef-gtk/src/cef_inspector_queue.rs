use std::collections::VecDeque;
// DevTools startup and DOM/console bursts exceed 64 small protocol messages.
// Bound retained bytes as before; also cap zero-size/per-message overhead.
pub(crate) const BUDGET: usize = 4 * 1024 * 1024;
#[derive(Default)]
pub(crate) struct Queue {
    items: VecDeque<Vec<u8>>,
    bytes: usize,
}
impl Queue {
    pub(crate) fn push(&mut self, value: &[u8]) -> bool {
        if self.items.len() >= 4096 || value.len() > BUDGET.saturating_sub(self.bytes) {
            return false;
        }
        self.bytes += value.len();
        self.items.push_back(value.to_vec());
        true
    }
    pub(crate) fn pop(&mut self) -> Option<Vec<u8>> {
        let value = self.items.pop_front()?;
        self.bytes -= value.len();
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn devtools_startup_and_console_bursts_fit_without_disconnect() {
        let mut q = Queue::default();
        for i in 0u32..2000 {
            assert!(q.push(&i.to_le_bytes()), "normal burst rejected at {i}");
        }
        for i in 0u32..2000 {
            assert_eq!(q.pop().unwrap(), i.to_le_bytes());
        }
        assert_eq!(q.bytes, 0);
    }
    #[test]
    fn byte_budget_remains_bounded_and_reclaimed() {
        let mut q = Queue::default();
        assert!(q.push(&vec![0; BUDGET]));
        assert!(!q.push(&[1]));
        assert_eq!(q.bytes, BUDGET);
        q.pop();
        assert!(q.push(&[1]));
        assert_eq!(q.bytes, 1);
    }
    #[test]
    fn empty_message_flood_is_bounded() {
        let mut q = Queue::default();
        for _ in 0..4096 {
            assert!(q.push(&[]));
        }
        assert!(!q.push(&[]));
    }
}
