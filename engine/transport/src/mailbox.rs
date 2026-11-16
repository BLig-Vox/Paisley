//! A UI-thread mailbox: control events retain order, frames replace older frames.
use std::{cell::RefCell, collections::VecDeque};

struct State<T> {
    controls: VecDeque<(T, usize)>,
    bytes: usize,
    frame: Option<T>,
    auxiliary: Option<T>,
    next_auxiliary: bool,
    closed: bool,
}
pub struct Mailbox<T> {
    state: RefCell<State<T>>,
    wake: async_channel::Sender<()>,
    ready: async_channel::Receiver<()>,
}

impl<T> Default for Mailbox<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> Mailbox<T> {
    pub fn new() -> Self {
        let (wake, ready) = async_channel::bounded(1);
        Self {
            state: RefCell::new(State {
                controls: VecDeque::new(),
                bytes: 0,
                frame: None,
                auxiliary: None,
                next_auxiliary: false,
                closed: false,
            }),
            wake,
            ready,
        }
    }
    pub fn push_control(&self, event: T, bytes: usize) -> Result<(), T> {
        let mut state = self.state.borrow_mut();
        if state.closed
            || state.controls.len() >= crate::ACTION_CAPACITY
            || bytes > (2 * 1024 * 1024usize).saturating_sub(state.bytes)
        {
            return Err(event);
        }
        state.controls.push_back((event, bytes));
        state.bytes += bytes;
        drop(state);
        let _ = self.wake.try_send(());
        Ok(())
    }
    pub fn push_frame(&self, frame: T) {
        let mut state = self.state.borrow_mut();
        if state.closed {
            return;
        }
        state.frame = Some(frame);
        drop(state);
        let _ = self.wake.try_send(());
    }
    pub fn push_aux_frame(&self, frame: T) {
        let mut state = self.state.borrow_mut();
        if state.closed {
            return;
        }
        state.auxiliary = Some(frame);
        drop(state);
        let _ = self.wake.try_send(());
    }
    pub fn try_recv(&self) -> Option<T> {
        let mut state = self.state.borrow_mut();
        if let Some((event, bytes)) = state.controls.pop_front() {
            state.bytes -= bytes;
            Some(event)
        } else {
            let frame = if state.next_auxiliary {
                state.auxiliary.take().or_else(|| state.frame.take())
            } else {
                state.frame.take().or_else(|| state.auxiliary.take())
            };
            if frame.is_some() {
                state.next_auxiliary = !state.next_auxiliary;
            }
            frame
        }
    }
    pub async fn recv(&self) -> Result<T, ()> {
        loop {
            if let Some(event) = self.try_recv() {
                return Ok(event);
            }
            if self.state.borrow().closed {
                return Err(());
            }
            self.ready.recv().await.map_err(|_| ())?;
        }
    }
    pub fn close(&self) {
        let mut state = self.state.borrow_mut();
        state.closed = true;
        state.controls.clear();
        state.frame = None;
        state.auxiliary = None;
        state.bytes = 0;
        self.wake.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_replace_without_losing_ordered_control_events() {
        let q = Mailbox::new();
        for n in 0..10_000 {
            q.push_frame(n);
        }
        q.push_control(20_001, 8).unwrap();
        q.push_control(20_002, 8).unwrap();
        assert_eq!(q.try_recv(), Some(20_001));
        assert_eq!(q.try_recv(), Some(20_002));
        assert_eq!(q.try_recv(), Some(9_999));
        assert_eq!(q.try_recv(), None);
    }
    #[test]
    fn main_and_popup_keep_independent_latest_frames() {
        let q = Mailbox::new();
        for n in 0..10000 {
            q.push_frame((0, n));
            q.push_aux_frame((1, n));
        }
        assert_eq!(q.try_recv(), Some((0, 9999)));
        assert_eq!(q.try_recv(), Some((1, 9999)));
        assert_eq!(q.try_recv(), None);
        q.push_aux_frame((1, 10000));
        q.close();
        assert_eq!(q.try_recv(), None);
    }
    #[test]
    fn continuous_page_paint_cannot_starve_a_popup() {
        let q = Mailbox::new();
        q.push_frame(1);
        q.push_aux_frame(2);
        assert_eq!(q.try_recv(), Some(1));
        q.push_frame(3);
        assert_eq!(q.try_recv(), Some(2));
        assert_eq!(q.try_recv(), Some(3));
    }
    #[test]
    fn controls_have_a_total_byte_budget_and_close_releases_everything() {
        let q = Mailbox::new();
        q.push_control(1, 2 * 1024 * 1024).unwrap();
        assert_eq!(q.push_control(2, 1), Err(2));
        q.push_frame(3);
        q.close();
        assert_eq!(q.try_recv(), None);
        assert_eq!(q.push_control(4, 1), Err(4));
        assert!(q.ready.is_closed());
    }
}
