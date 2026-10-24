/// Cumulative thresholds supplied by the game's five-heart progression table.
pub const HEART_THRESHOLDS: [u64; 5] = [2_000, 8_000, 18_000, 38_000, 88_000];
pub const MAX_BOND: u64 = HEART_THRESHOLDS[4];

#[derive(Clone, Copy, Debug)]
pub struct BondProgress {
    pub total: u64,
    pub hearts: usize,
    pub stage_current: u64,
    pub stage_required: u64,
}
impl BondProgress {
    pub fn new(total: u64) -> Self {
        let total = total.min(MAX_BOND);
        let hearts = HEART_THRESHOLDS
            .iter()
            .take_while(|&&limit| total >= limit)
            .count();
        let stage = hearts.min(4);
        let start = if stage == 0 {
            0
        } else {
            HEART_THRESHOLDS[stage - 1]
        };
        Self {
            total,
            hearts,
            stage_current: total - start,
            stage_required: HEART_THRESHOLDS[stage] - start,
        }
    }
    pub fn percent(self) -> f32 {
        self.total as f32 / MAX_BOND as f32 * 100.
    }
    pub fn remaining(self) -> u64 {
        self.stage_required - self.stage_current
    }
    pub fn heart_fill(self, index: usize) -> f32 {
        if index < self.hearts {
            1.
        } else if index == self.hearts && self.hearts < 5 {
            self.stage_current as f32 / self.stage_required as f32
        } else {
            0.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn heart_levels_and_stage_progress_change_at_exact_thresholds() {
        for (level, threshold) in HEART_THRESHOLDS.into_iter().enumerate() {
            let before = BondProgress::new(threshold - 1);
            assert_eq!(before.hearts, level);
            assert_eq!(before.remaining(), 1);
            let at = BondProgress::new(threshold);
            assert_eq!(at.hearts, level + 1);
            if level < 4 {
                assert_eq!(at.stage_current, 0);
            }
        }
        let middle = BondProgress::new(12_000);
        assert_eq!(
            (middle.hearts, middle.stage_current, middle.stage_required),
            (2, 4_000, 10_000)
        );
        let full = BondProgress::new(u64::MAX);
        assert_eq!((full.total, full.hearts, full.remaining()), (88_000, 5, 0));
        assert_eq!(full.percent(), 100.);
        assert_eq!(full.heart_fill(5), 0.);
        assert_eq!(BondProgress::new(0).percent(), 0.);
        let partial = BondProgress::new(13_000);
        assert_eq!(
            (
                partial.heart_fill(0),
                partial.heart_fill(2),
                partial.heart_fill(3)
            ),
            (1., 0.5, 0.)
        );
    }
}
