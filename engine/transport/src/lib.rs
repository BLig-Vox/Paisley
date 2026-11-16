//! Bounds for Paisley's Servo process boundary.
use std::io;
pub mod mailbox;
pub mod residency;

pub const MAX_PACKET_BYTES: usize = 64 * 1024 * 1024;
pub const EVENT_CAPACITY: usize = 2;
pub const ACTION_CAPACITY: usize = 512;
pub const MAX_ACTION_BYTES: usize = 256 * 1024;

pub fn packet_length(header: [u8; 4]) -> io::Result<usize> {
    let len = u32::from_le_bytes(header) as usize;
    if len == 0 || len > MAX_PACKET_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Servo packet exceeds memory budget",
        ));
    }
    Ok(len)
}

pub fn event_channel<T>() -> (async_channel::Sender<T>, async_channel::Receiver<T>) {
    async_channel::bounded(EVENT_CAPACITY)
}

pub fn action_channel<T>() -> (async_channel::Sender<T>, async_channel::Receiver<T>) {
    async_channel::bounded(ACTION_CAPACITY)
}

pub fn valid_rgba(width: u32, height: u32, bytes: usize) -> bool {
    width > 0
        && height > 0
        && (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .is_some_and(|n| n == bytes && n <= MAX_PACKET_BYTES)
}

pub const TRACKER_DOMAINS: &[&str] = &[
    "doubleclick.net",
    "googlesyndication.com",
    "googleadservices.com",
    "google-analytics.com",
    "googletagmanager.com",
    "adnxs.com",
    "adsrvr.org",
    "scorecardresearch.com",
    "quantserve.com",
    "criteo.com",
    "taboola.com",
    "outbrain.com",
];

pub fn tracker_blocked(target: &str, top: &str) -> bool {
    fn matches(host: &str, domain: &str) -> bool {
        host == domain
            || host
                .strip_suffix(domain)
                .is_some_and(|prefix| prefix.ends_with('.'))
    }
    let target = target.trim_end_matches('.').to_ascii_lowercase();
    let top = top.trim_end_matches('.').to_ascii_lowercase();
    TRACKER_DOMAINS
        .iter()
        .any(|domain| matches(&target, domain) && !matches(&top, domain))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_packets_are_rejected_before_allocating() {
        assert!(packet_length(u32::MAX.to_le_bytes()).is_err());
        assert!(packet_length(0u32.to_le_bytes()).is_err());
        assert_eq!(
            packet_length((MAX_PACKET_BYTES as u32).to_le_bytes()).unwrap(),
            MAX_PACKET_BYTES
        );
    }
    #[test]
    fn busy_ui_applies_backpressure_instead_of_accumulating_frames() {
        let (tx, rx) = event_channel();
        tx.try_send(1).unwrap();
        tx.try_send(2).unwrap();
        assert!(matches!(
            tx.try_send(3),
            Err(async_channel::TrySendError::Full(3))
        ));
        assert_eq!(rx.try_recv().unwrap(), 1);
        assert_eq!(rx.try_recv().unwrap(), 2);
    }
    #[test]
    fn input_queue_is_bounded_without_spawning_waiting_tasks() {
        let (tx, rx) = action_channel();
        for n in 0..ACTION_CAPACITY {
            tx.try_send(n).unwrap();
        }
        assert!(tx.try_send(ACTION_CAPACITY).is_err());
        drop(rx);
        assert!(tx.is_closed());
    }
    #[test]
    fn malformed_pixel_lengths_and_dimensions_are_rejected() {
        assert!(!valid_rgba(0, 4, 0));
        assert!(!valid_rgba(u32::MAX, u32::MAX, 1));
        assert!(!valid_rgba(2, 2, 15));
        assert!(valid_rgba(2, 2, 16));
        assert!(valid_rgba(3840, 2160, 3840 * 2160 * 4));
    }
    #[test]
    fn tracker_filter_respects_domain_boundaries_and_first_party() {
        for domain in TRACKER_DOMAINS {
            assert!(tracker_blocked(domain, "example.org"));
            assert!(tracker_blocked(&format!("cdn.{domain}"), "example.org"));
            assert!(!tracker_blocked(domain, domain));
            assert!(!tracker_blocked(domain, &format!("www.{domain}")));
            assert!(!tracker_blocked(
                &format!("{domain}.example.org"),
                "example.net"
            ));
            assert!(!tracker_blocked(&format!("not{domain}"), "example.net"));
        }
    }
}
