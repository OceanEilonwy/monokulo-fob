//! The coins bar on Home and Coins: one segment per coin, each as wide as
//! its amount. Ready coins come first, in ink, largest first; then the
//! coins not ready yet, in green, soonest ready first, each filling from
//! faint to solid as its own confirmations come, so the next to turn ink
//! sits beside the ink. Past [`MAX_SEGMENTS`] the smallest ready coins
//! share one segment at the end of the ink, and no segment is drawn
//! narrower than the minimum the screen asks for, so dust still shows.

use crate::not_ready::PieceView;

/// The most segments the bar draws.
pub const MAX_SEGMENTS: usize = 24;

/// One segment of the bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    /// In atomic units: how wide it is beside the others.
    pub atomic: u64,
    /// For a coin not ready yet, how far along it is (0 to 1); `None` for
    /// money ready to spend.
    pub progress: Option<f32>,
    /// How many coins it stands for: 1, or more for the smallest ready ones
    /// put together.
    pub coins: u32,
}

/// The bar's segments for the `ready` coins (any order) and the `waiting`
/// pieces (soonest ready first, as [`crate::not_ready::summarise`] lists
/// them).
pub fn segments(ready: &[u64], waiting: &[PieceView]) -> Vec<Segment> {
    let mut ready: Vec<u64> = ready.iter().copied().filter(|a| *a > 0).collect();
    ready.sort_unstable_by(|a, b| b.cmp(a));
    let mut out: Vec<Segment> = ready.iter().map(|&atomic| Segment { atomic, progress: None, coins: 1 }).collect();
    let room = MAX_SEGMENTS.saturating_sub(waiting.len()).max(1);
    if out.len() > room {
        let rest = out.split_off(room - 1);
        out.push(Segment { atomic: fob_monero::amount::total(rest.iter().map(|s| s.atomic)), progress: None, coins: rest.len() as u32 });
    }
    // Waiting pieces past the cap (only with more than 23 not ready at
    // once) share the last segment likewise, at their weighted progress.
    let keep = MAX_SEGMENTS.saturating_sub(out.len()).max(1);
    let (shown, rest) = if waiting.len() <= keep { (waiting, &[][..]) } else { waiting.split_at(keep - 1) };
    out.extend(shown.iter().map(|p| Segment { atomic: p.atomic, progress: Some(p.progress.clamp(0.0, 1.0)), coins: 1 }));
    if !rest.is_empty() {
        let atomic = fob_monero::amount::total(rest.iter().map(|p| p.atomic));
        let done: f64 = rest.iter().map(|p| p.atomic as f64 * f64::from(p.progress.clamp(0.0, 1.0))).sum();
        let progress = if atomic == 0 { 0.0 } else { (done / atomic as f64) as f32 };
        out.push(Segment { atomic, progress: Some(progress), coins: rest.len() as u32 });
    }
    out
}

/// Each segment's width across `width`, `gap` apart, none narrower than
/// `min` (all in the screen's own units): those that would be narrower take
/// `min`, and the others share what's left by amount. Squeezed below
/// `min` each only when even that doesn't fit.
pub fn widths(atomics: &[u64], width: f32, gap: f32, min: f32) -> Vec<f32> {
    let n = atomics.len();
    if n == 0 {
        return vec![];
    }
    let free = (width - gap * (n - 1) as f32).max(0.0);
    if min * n as f32 >= free {
        return vec![free / n as f32; n];
    }
    let mut floored = vec![false; n];
    loop {
        let share: f64 = atomics.iter().zip(&floored).filter(|(_, f)| !**f).map(|(a, _)| *a as f64).sum();
        let left = f64::from(free) - f64::from(min) * floored.iter().filter(|f| **f).count() as f64;
        let widths: Vec<f32> = atomics
            .iter()
            .zip(&floored)
            .map(|(a, f)| if *f { min } else if share > 0.0 { (*a as f64 / share * left) as f32 } else { (left / floored.iter().filter(|f| !**f).count() as f64) as f32 })
            .collect();
        let mut changed = false;
        for (i, w) in widths.iter().enumerate() {
            if !floored[i] && *w < min {
                floored[i] = true;
                changed = true;
            }
        }
        if !changed {
            return widths;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::not_ready::Source;

    fn piece(atomic: u64, progress: f32) -> PieceView {
        PieceView {
            atomic,
            amount: String::new(),
            fiat: None,
            confirmations: 0,
            needed: 10,
            progress,
            source: Source::Received,
            came: String::new(),
            ready: String::new(),
            seconds_left: 0,
            ready_at: 0,
        }
    }

    #[test]
    fn ready_coins_go_first_largest_first_then_the_waiting_in_order() {
        let s = segments(&[5, 0, 30, 10], &[piece(20, 0.6), piece(7, 0.2)]);
        let shape: Vec<(u64, Option<f32>)> = s.iter().map(|s| (s.atomic, s.progress)).collect();
        assert_eq!(shape, [(30, None), (10, None), (5, None), (20, Some(0.6)), (7, Some(0.2))]);
        assert!(s.iter().all(|s| s.coins == 1));
    }

    #[test]
    fn past_the_cap_the_smallest_ready_coins_share_one_segment() {
        let ready: Vec<u64> = (1..=40).collect();
        let s = segments(&ready, &[piece(100, 0.5)]);
        assert_eq!(s.len(), MAX_SEGMENTS);
        let merged = &s[MAX_SEGMENTS - 2];
        assert_eq!((merged.coins, merged.progress), (40 - (MAX_SEGMENTS as u32 - 2), None));
        assert_eq!(s.iter().map(|s| s.atomic).sum::<u64>(), ready.iter().sum::<u64>() + 100);
        assert_eq!(s.last().unwrap().progress, Some(0.5));
    }

    #[test]
    fn too_many_waiting_share_the_last_segment_at_their_weighted_progress() {
        let waiting: Vec<PieceView> = (0..30).map(|i| piece(10, if i < 15 { 1.0 } else { 0.0 })).collect();
        let s = segments(&[50], &waiting);
        assert_eq!(s.len(), MAX_SEGMENTS);
        assert_eq!(s.iter().map(|s| s.coins).sum::<u32>(), 31);
        assert_eq!(s.iter().map(|s| s.atomic).sum::<u64>(), 350);
        let last = s.last().unwrap();
        assert_eq!(last.coins, 30 - (MAX_SEGMENTS as u32 - 2));
    }

    #[test]
    fn widths_fill_the_bar_with_none_below_the_minimum() {
        let w = widths(&[2_600, 1_200, 40, 20, 10, 6, 15, 8], 318.0, 2.0, 4.0);
        let total: f32 = w.iter().sum::<f32>() + 2.0 * 7.0;
        assert!((total - 318.0).abs() < 0.01, "{total}");
        assert!(w.iter().all(|w| *w >= 4.0 - 1e-4), "{w:?}");
        assert!(w[0] > w[1] && w[1] > w[2]);
        assert_eq!(w[2], 4.0);
    }

    #[test]
    fn widths_share_evenly_when_even_the_minimum_does_not_fit() {
        assert_eq!(widths(&[1, 2, 3], 10.0, 2.0, 4.0), vec![2.0; 3]);
        assert_eq!(widths(&[], 100.0, 2.0, 4.0), Vec::<f32>::new());
        assert_eq!(widths(&[0, 0], 102.0, 2.0, 4.0), vec![50.0, 50.0]);
    }
}
