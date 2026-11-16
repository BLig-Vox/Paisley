//! Engine-independent eligibility policy. Unknown or stale activity is protected.
#[derive(Clone, Debug)]
pub struct Tab {
    pub id: u64,
    pub active: bool,
    pub last_used: u64,
    pub activity: Option<(u64, bool)>,
    pub reloadable: bool,
    pub busy: bool,
}
pub fn victims(tabs: &[Tab], now: u64, available_mib: u64, browser_mib: u64) -> Vec<u64> {
    let pressure = available_mib <= 2048 || browser_mib >= 3072;
    let target = if pressure { 1 } else { 3 };
    let grace = if pressure { 30 } else { 300 };
    let mut eligible: Vec<_> = tabs
        .iter()
        .filter(|tab| {
            !tab.active
                && !tab.busy
                && tab.reloadable
                && now.saturating_sub(tab.last_used) >= grace
                && tab
                    .activity
                    .is_some_and(|(at, protected)| !protected && at <= now && now - at <= 15)
        })
        .collect();
    eligible.sort_by_key(|tab| (tab.last_used, tab.id));
    eligible
        .into_iter()
        .take(tabs.len().saturating_sub(target))
        .map(|tab| tab.id)
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tab(id: u64, last_used: u64) -> Tab {
        Tab {
            id,
            active: false,
            last_used,
            activity: Some((1000, false)),
            reloadable: true,
            busy: false,
        }
    }
    #[test]
    fn trims_oldest_idle_pages_to_three() {
        let tabs = vec![
            tab(1, 100),
            tab(2, 200),
            tab(3, 300),
            tab(4, 400),
            tab(5, 500),
        ];
        assert_eq!(victims(&tabs, 1000, 8000, 1500), vec![1, 2]);
    }
    #[test]
    fn pressure_reclaims_eligible_pages_after_thirty_seconds() {
        let tabs = vec![tab(1, 950), tab(2, 960), tab(3, 990)];
        assert_eq!(victims(&tabs, 1000, 1500, 1000), vec![1, 2]);
        assert_eq!(victims(&tabs, 1000, 8000, 4000), vec![1, 2]);
    }
    #[test]
    fn protects_active_unknown_dirty_stale_busy_and_non_reloadable() {
        let mut tabs = vec![
            tab(1, 0),
            tab(2, 0),
            tab(3, 0),
            tab(4, 0),
            tab(5, 0),
            tab(6, 0),
            tab(7, 0),
        ];
        tabs[0].active = true;
        tabs[1].activity = None;
        tabs[2].activity = Some((1000, true));
        tabs[3].activity = Some((980, false));
        tabs[4].busy = true;
        tabs[5].reloadable = false;
        assert_eq!(victims(&tabs, 1000, 1000, 4000), vec![7]);
    }
    #[test]
    fn no_discard_below_target_or_before_idle_grace() {
        let tabs = vec![tab(1, 999), tab(2, 998), tab(3, 997), tab(4, 996)];
        assert!(victims(&tabs, 1000, 8000, 1000).is_empty());
        assert!(victims(&tabs[..3], 1000, 8000, 1000).is_empty());
        assert!(victims(&tabs, 1000, 1000, 4000).is_empty());
    }
    #[test]
    fn ties_are_stable_and_future_activity_is_not_trusted() {
        let mut tabs = vec![tab(4, 0), tab(3, 0), tab(2, 0), tab(1, 0)];
        assert_eq!(victims(&tabs, 1000, 8000, 1000), vec![1]);
        for t in &mut tabs {
            t.activity = Some((1001, false));
        }
        assert!(victims(&tabs, 1000, 1000, 4000).is_empty());
    }
}
