//! Weighted local search for coverings with `t = 2` or `t = 3`.
//!
//! The search holds some blocks and the list of t-sets that no block contains. One step picks an
//! open t-set at random, looks at the blocks that already hold most of its points, and makes the
//! swap (one point out, one point of the set in) that lowers the weighted number of open sets the
//! most. When no swap helps, every open set grows heavier, which reshapes the landscape around the
//! place the search is stuck in. Once nothing is open the blocks form a covering: the block that
//! covers the fewest sets alone is removed and the search repairs the damage, one level lower. A
//! level that resists for too long is tried again from the last covering, shaken.
//!
//! A run depends on its seed and settings alone. It reads no clock and keeps no global state, so
//! the same call gives the same blocks on any machine.

use crate::design::{Covering, MAX_POINTS};
use crate::rng::Rng;

/// The knobs of the search. The defaults are the values that found the most known records again
/// in a fixed time on a calibration set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Steps during which a swap may not be undone.
    pub tabu: i64,
    /// Chance, out of a thousand, that a step makes a random swap instead of the best one.
    pub noise: usize,
    /// Chance, out of a hundred, that a swap which changes nothing is still made.
    pub flat: usize,
    /// Chance, out of a hundred, that the best swap is made even though it gains nothing, instead of raising
    /// the weights. Without it the search can only leave a plateau by reshaping the weights.
    pub walk: usize,
    /// After this many weight raises, every weight above one drops by one.
    pub smooth: u64,
    /// At most this many blocks are scored in one step.
    pub cap: usize,
    /// Random swaps made on the last covering before a level is tried again.
    pub kick: usize,
    /// Steps a level may resist before it is tried again.
    pub patience: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tabu: 3,
            noise: 20,
            flat: 30,
            walk: 3,
            smooth: 1000,
            cap: 24,
            kick: 6,
            patience: 1_500_000,
        }
    }
}

impl Settings {
    /// The default settings with `patience` scaled to the block size: a step costs about
    /// `cap * t * k^t` lookups, so this gives every level a similar amount of work before it is
    /// tried again, whatever the parameters are.
    pub fn for_blocks(k: usize, t: usize) -> Self {
        let settings = Self::default();
        let work = (settings.cap * t) as u64 * (k as u64).saturating_pow(t as u32);
        Self {
            patience: (20_000_000_000 / work.max(1)).max(100_000),
            ..settings
        }
    }
}

/// Where a search stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    /// Steps made so far.
    pub steps: u64,
    /// Blocks in the smallest covering found, if any was found.
    pub best: Option<usize>,
    /// The lowest number of blocks tried without reaching a covering, and the fewest t-sets left
    /// open at that level. `None` until a level has resisted at least one step.
    pub lowest: Option<(usize, usize)>,
    /// How often a level was tried again from the last covering.
    pub retries: u64,
}

const NOT_OPEN: u32 = u32::MAX;
const LONG_AGO: i64 = -1_000_000_000;

/// A running search. See the module text for what it does.
pub struct Search {
    v: usize,
    k: usize,
    t: usize,
    settings: Settings,
    rng: Rng,
    /// Number of blocks held now.
    live: usize,
    /// `k` points per block, in no particular order.
    points: Vec<u8>,
    /// `v` flags per block: is the point in the block.
    member: Vec<bool>,
    /// Per block and point: the step at which the point last left, and last entered, the block.
    left: Vec<i64>,
    entered: Vec<i64>,
    /// The points of every t-set (the third is unused when `t = 2`).
    sets: Vec<[u8; 3]>,
    /// The number of a t-set, looked up by its points in any order.
    lookup: Vec<u32>,
    /// How many blocks contain each t-set.
    cover: Vec<u16>,
    weight: Vec<u32>,
    /// The t-sets that no block contains, and where each one sits in that list.
    open: Vec<u32>,
    place: Vec<u32>,
    candidates: Vec<usize>,
    steps: u64,
    raises: u64,
    level_started: u64,
    saved: Vec<u8>,
    saved_live: usize,
    best: Option<Covering>,
    lowest: Option<(usize, usize)>,
    /// The blocks as they were when `lowest` was last improved.
    closest: Vec<u8>,
    retries: u64,
}

impl Search {
    /// Starts a search for (v, k, t) coverings from `start` random blocks.
    pub fn new(
        v: usize,
        k: usize,
        t: usize,
        start: usize,
        seed: u64,
        settings: Settings,
    ) -> Result<Self, String> {
        if !(t == 2 || t == 3) {
            return Err(format!("the search handles t = 2 and t = 3, not t = {t}"));
        }
        if k <= t || k >= v || v > MAX_POINTS {
            return Err(format!(
                "need t < k < v <= {MAX_POINTS}, got v = {v}, k = {k}, t = {t}"
            ));
        }
        if start == 0 || start > usize::from(u16::MAX) {
            return Err(format!(
                "need between 1 and {} starting blocks, got {start}",
                u16::MAX
            ));
        }
        if settings.smooth == 0 || settings.cap == 0 {
            return Err("the settings `smooth` and `cap` must be at least 1".to_string());
        }
        let mut sets = Vec::new();
        let mut lookup = vec![0u32; if t == 2 { v * v } else { v * v * v }];
        for a in 0..v {
            for c in a + 1..v {
                if t == 2 {
                    lookup[a * v + c] = sets.len() as u32;
                    lookup[c * v + a] = sets.len() as u32;
                    sets.push([a as u8, c as u8, 0]);
                    continue;
                }
                for d in c + 1..v {
                    for (x, y, z) in [
                        (a, c, d),
                        (a, d, c),
                        (c, a, d),
                        (c, d, a),
                        (d, a, c),
                        (d, c, a),
                    ] {
                        lookup[(x * v + y) * v + z] = sets.len() as u32;
                    }
                    sets.push([a as u8, c as u8, d as u8]);
                }
            }
        }
        let count = sets.len();
        let mut search = Self {
            v,
            k,
            t,
            settings,
            rng: Rng::new(seed),
            live: start,
            points: vec![0; start * k],
            member: vec![false; start * v],
            left: vec![LONG_AGO; start * v],
            entered: vec![LONG_AGO; start * v],
            sets,
            lookup,
            cover: vec![0; count],
            weight: vec![1; count],
            open: (0..count as u32).collect(),
            place: (0..count as u32).collect(),
            candidates: Vec::with_capacity(start),
            steps: 0,
            raises: 0,
            level_started: 0,
            saved: Vec::new(),
            saved_live: 0,
            best: None,
            lowest: None,
            closest: Vec::new(),
            retries: 0,
        };
        let mut deck: Vec<u8> = (0..v as u8).collect();
        for block in 0..start {
            for i in 0..k {
                let other = i + search.rng.below(v - i);
                deck.swap(i, other);
                search.points[block * k + i] = deck[i];
                search.member[block * v + usize::from(deck[i])] = true;
            }
            search.count_block(block, true);
        }
        Ok(search)
    }

    /// Makes up to `budget` more steps. Returns true as soon as a covering with at most `stop_at`
    /// blocks is held; the search has then nothing left to do.
    ///
    /// Spending a budget in several calls gives exactly the run that one call would give.
    pub fn run(&mut self, budget: u64, stop_at: usize) -> bool {
        let end = self.steps.saturating_add(budget);
        loop {
            if self.open.is_empty() {
                if self
                    .best
                    .as_ref()
                    .is_none_or(|best| self.live < best.blocks.len())
                {
                    self.best = Some(self.snapshot());
                }
                if self.live <= stop_at.max(1) {
                    return true;
                }
                self.saved.clear();
                self.saved
                    .extend_from_slice(&self.points[..self.live * self.k]);
                self.saved_live = self.live;
                self.drop_block();
                self.level_started = self.steps;
                continue;
            }
            let retry_at = self
                .level_started
                .saturating_add(self.settings.patience)
                .saturating_add(1);
            if self.steps >= retry_at {
                if self.saved_live > 0 {
                    self.retry();
                    self.retries += 1;
                }
                self.level_started = self.steps;
                continue;
            }
            if self.steps >= end {
                return false;
            }
            let pause = end.min(retry_at);
            while self.steps < pause && !self.open.is_empty() {
                self.step();
                let here = (self.live, self.open.len());
                if here.1 > 0 && self.lowest.is_none_or(|low| here < low) {
                    self.lowest = Some(here);
                    self.closest.clear();
                    self.closest
                        .extend_from_slice(&self.points[..self.live * self.k]);
                }
            }
        }
    }

    /// The smallest covering found so far.
    pub fn best(&self) -> Option<&Covering> {
        self.best.as_ref()
    }

    /// The blocks of the nearest miss: the lowest level tried without a covering, at the moment
    /// the fewest t-sets were open there. It is not a covering; the checker says what it lacks.
    pub fn closest(&self) -> Option<Covering> {
        self.lowest.map(|_| Covering {
            v: self.v,
            k: self.k,
            t: self.t,
            blocks: self.closest.chunks(self.k).map(<[u8]>::to_vec).collect(),
        })
    }

    /// A snapshot of how far the search has got.
    pub fn progress(&self) -> Progress {
        Progress {
            steps: self.steps,
            best: self.best.as_ref().map(|best| best.blocks.len()),
            lowest: self.lowest,
            retries: self.retries,
        }
    }

    fn snapshot(&self) -> Covering {
        let blocks = self.points[..self.live * self.k]
            .chunks(self.k)
            .map(<[u8]>::to_vec)
            .collect();
        Covering {
            v: self.v,
            k: self.k,
            t: self.t,
            blocks,
        }
    }

    #[inline]
    fn set_of(&self, a: u8, c: u8, d: u8) -> usize {
        let (a, c) = (usize::from(a), usize::from(c));
        if self.t == 2 {
            self.lookup[a * self.v + c] as usize
        } else {
            self.lookup[(a * self.v + c) * self.v + usize::from(d)] as usize
        }
    }

    #[inline]
    fn cover_more(&mut self, set: usize) {
        if self.cover[set] == 0 {
            let at = self.place[set] as usize;
            let last = self
                .open
                .pop()
                .expect("a set with no cover is in the open list");
            if at < self.open.len() {
                self.open[at] = last;
                self.place[last as usize] = at as u32;
            }
            self.place[set] = NOT_OPEN;
        }
        self.cover[set] += 1;
    }

    #[inline]
    fn cover_less(&mut self, set: usize) {
        self.cover[set] -= 1;
        if self.cover[set] == 0 {
            self.place[set] = self.open.len() as u32;
            self.open.push(set as u32);
        }
    }

    /// Adds every t-set of a block to the cover counts, or takes them all away.
    fn count_block(&mut self, block: usize, add: bool) {
        let (k, base) = (self.k, block * self.k);
        for i in 0..k {
            for j in i + 1..k {
                let (a, c) = (self.points[base + i], self.points[base + j]);
                if self.t == 2 {
                    let set = self.set_of(a, c, 0);
                    if add {
                        self.cover_more(set);
                    } else {
                        self.cover_less(set);
                    }
                    continue;
                }
                for l in j + 1..k {
                    let set = self.set_of(a, c, self.points[base + l]);
                    if add {
                        self.cover_more(set);
                    } else {
                        self.cover_less(set);
                    }
                }
            }
        }
    }

    /// The weighted change in open sets if `block` trades the point at position `out` for `into`.
    #[inline]
    fn gain(&self, block: usize, out: usize, into: u8) -> i64 {
        let (k, v) = (self.k, self.v);
        let held = &self.points[block * k..(block + 1) * k];
        let (leaving, into) = (usize::from(held[out]), usize::from(into));
        let mut change = 0i64;
        if self.t == 2 {
            let lost_row = &self.lookup[leaving * v..(leaving + 1) * v];
            let won_row = &self.lookup[into * v..(into + 1) * v];
            for (i, &a) in held.iter().enumerate() {
                if i == out {
                    continue;
                }
                let (lost, won) = (
                    lost_row[usize::from(a)] as usize,
                    won_row[usize::from(a)] as usize,
                );
                if self.cover[lost] == 1 {
                    change += i64::from(self.weight[lost]);
                }
                if self.cover[won] == 0 {
                    change -= i64::from(self.weight[won]);
                }
            }
            return change;
        }
        for (i, &a) in held.iter().enumerate() {
            if i == out {
                continue;
            }
            let (lost_at, won_at) = (
                (leaving * v + usize::from(a)) * v,
                (into * v + usize::from(a)) * v,
            );
            let (lost_row, won_row) = (
                &self.lookup[lost_at..lost_at + v],
                &self.lookup[won_at..won_at + v],
            );
            for (j, &c) in held.iter().enumerate().skip(i + 1) {
                if j == out {
                    continue;
                }
                let (lost, won) = (
                    lost_row[usize::from(c)] as usize,
                    won_row[usize::from(c)] as usize,
                );
                if self.cover[lost] == 1 {
                    change += i64::from(self.weight[lost]);
                }
                if self.cover[won] == 0 {
                    change -= i64::from(self.weight[won]);
                }
            }
        }
        change
    }

    fn swap(&mut self, block: usize, out: usize, into: u8) {
        let (k, v, base) = (self.k, self.v, block * self.k);
        let leaving = self.points[base + out];
        for i in 0..k {
            if i == out {
                continue;
            }
            let a = self.points[base + i];
            if self.t == 2 {
                let (lost, won) = (self.set_of(leaving, a, 0), self.set_of(into, a, 0));
                self.cover_less(lost);
                self.cover_more(won);
                continue;
            }
            for j in i + 1..k {
                if j == out {
                    continue;
                }
                let c = self.points[base + j];
                let (lost, won) = (self.set_of(leaving, a, c), self.set_of(into, a, c));
                self.cover_less(lost);
                self.cover_more(won);
            }
        }
        self.points[base + out] = into;
        self.member[block * v + usize::from(leaving)] = false;
        self.member[block * v + usize::from(into)] = true;
        self.left[block * v + usize::from(leaving)] = self.steps as i64;
        self.entered[block * v + usize::from(into)] = self.steps as i64;
    }

    /// Makes every open set heavier; now and then lets all weights sink a little.
    fn raise(&mut self) {
        for &set in &self.open {
            self.weight[set as usize] += 1;
        }
        self.raises += 1;
        if self.raises % self.settings.smooth == 0 {
            for weight in &mut self.weight {
                if *weight > 1 {
                    *weight -= 1;
                }
            }
        }
    }

    fn step(&mut self) {
        let (v, k, t) = (self.v, self.k, self.t);
        let wanted = self.sets[self.open[self.rng.below(self.open.len())] as usize];
        let wanted = &wanted[..t];
        // The blocks that already hold the most points of the wanted set.
        let (w0, w1, w2) = (
            usize::from(wanted[0]),
            usize::from(wanted[1]),
            usize::from(wanted[t - 1]),
        );
        let mut most = 0;
        self.candidates.clear();
        for (block, row) in self.member.chunks_exact(v).take(self.live).enumerate() {
            let held = usize::from(row[w0]) + usize::from(row[w1]) + usize::from(t == 3 && row[w2]);
            if self.candidates.is_empty() || held > most {
                most = held;
                self.candidates.clear();
            }
            if held == most {
                self.candidates.push(block);
            }
        }
        let now = self.steps as i64;
        let (mut best, mut best_swap, mut ties) = (i64::MAX, None, 0);
        let (mut any_swap, mut seen) = (None, 0);
        let scored = self.candidates.len().min(self.settings.cap);
        for slot in 0..scored {
            if self.candidates.len() > self.settings.cap {
                let other = slot + self.rng.below(self.candidates.len() - slot);
                self.candidates.swap(slot, other);
            }
            let block = self.candidates[slot];
            for &into in wanted {
                if self.member[block * v + usize::from(into)] {
                    continue;
                }
                for out in 0..k {
                    let leaving = self.points[block * k + out];
                    if leaving == wanted[0] || leaving == wanted[1] || leaving == wanted[t - 1] {
                        continue;
                    }
                    seen += 1;
                    if self.rng.below(seen) == 0 {
                        any_swap = Some((block, out, into));
                    }
                    let recent = now - self.left[block * v + usize::from(into)]
                        < self.settings.tabu
                        || now - self.entered[block * v + usize::from(leaving)]
                            < self.settings.tabu;
                    if recent {
                        continue;
                    }
                    let change = self.gain(block, out, into);
                    if change < best {
                        (best, best_swap, ties) = (change, Some((block, out, into)), 1);
                    } else if change == best {
                        ties += 1;
                        if self.rng.below(ties) == 0 {
                            best_swap = Some((block, out, into));
                        }
                    }
                }
            }
        }
        self.steps += 1;
        let Some((block, out, into)) =
            best_swap.filter(|_| self.rng.below(1000) >= self.settings.noise)
        else {
            if let Some((block, out, into)) = any_swap {
                self.swap(block, out, into);
            }
            return;
        };
        if best < 0
            || most + 1 < t
            || (best == 0 && self.rng.below(100) < self.settings.flat)
            || self.rng.below(100) < self.settings.walk
        {
            self.swap(block, out, into);
        } else {
            self.raise();
        }
    }

    /// Removes the block that covers the fewest t-sets alone.
    fn drop_block(&mut self) {
        let (k, v) = (self.k, self.v);
        let (mut loser, mut fewest) = (0, usize::MAX);
        for block in 0..self.live {
            let base = block * k;
            let mut alone = 0;
            for i in 0..k {
                for j in i + 1..k {
                    let (a, c) = (self.points[base + i], self.points[base + j]);
                    if self.t == 2 {
                        alone += usize::from(self.cover[self.set_of(a, c, 0)] == 1);
                        continue;
                    }
                    for l in j + 1..k {
                        alone +=
                            usize::from(self.cover[self.set_of(a, c, self.points[base + l])] == 1);
                    }
                }
            }
            if alone < fewest || (alone == fewest && self.rng.below(2) == 1) {
                (loser, fewest) = (block, alone);
            }
        }
        self.count_block(loser, false);
        let last = self.live - 1;
        if loser != last {
            self.points.copy_within(last * k..(last + 1) * k, loser * k);
            self.member.copy_within(last * v..(last + 1) * v, loser * v);
            self.left.copy_within(last * v..(last + 1) * v, loser * v);
            self.entered
                .copy_within(last * v..(last + 1) * v, loser * v);
        }
        self.live = last;
    }

    /// Goes back to the last covering and swaps a few random points, so that the next try at the
    /// level below starts from somewhere else.
    fn retry(&mut self) {
        let (k, v) = (self.k, self.v);
        self.cover.fill(0);
        self.weight.fill(1);
        self.member.fill(false);
        self.left.fill(LONG_AGO);
        self.entered.fill(LONG_AGO);
        self.open.clear();
        self.open.extend(0..self.sets.len() as u32);
        for (set, place) in self.place.iter_mut().enumerate() {
            *place = set as u32;
        }
        self.live = self.saved_live;
        self.points[..self.live * k].copy_from_slice(&self.saved);
        for block in 0..self.live {
            for i in 0..k {
                self.member[block * v + usize::from(self.points[block * k + i])] = true;
            }
            self.count_block(block, true);
        }
        for _ in 0..self.settings.kick {
            let (block, out, into) = (
                self.rng.below(self.live),
                self.rng.below(k),
                self.rng.below(v) as u8,
            );
            if !self.member[block * v + usize::from(into)] {
                self.swap(block, out, into);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bound::schoenheim;
    use crate::check::check;

    fn hunt(v: usize, k: usize, t: usize, seed: u64, budget: u64) -> Search {
        let goal = schoenheim(v as u64, k as u64, t as u64) as usize;
        let mut search = Search::new(v, k, t, goal + 2, seed, Settings::default()).unwrap();
        assert!(
            search.run(budget, goal),
            "no {goal}-block covering for ({v},{k},{t}) within {budget} steps"
        );
        search
    }

    fn assert_exact(search: &Search, blocks: usize) {
        let best = search.best().expect("a covering was found");
        assert_eq!(best.blocks.len(), blocks);
        assert!(
            check(best).unwrap().is_covering(),
            "the search's own covering failed the checker"
        );
        assert_eq!(search.progress().best, Some(blocks));
    }

    #[test]
    fn finds_the_fano_plane() {
        assert_exact(&hunt(7, 3, 2, 1, 100_000), 7);
    }

    #[test]
    fn finds_the_affine_plane_of_order_three() {
        assert_exact(&hunt(9, 3, 2, 1, 400_000), 12);
    }

    #[test]
    fn finds_the_steiner_quadruples_on_eight_points() {
        assert_exact(&hunt(8, 4, 3, 1, 400_000), 14);
    }

    #[test]
    fn the_same_seed_gives_the_same_run() {
        let (first, second) = (hunt(9, 3, 2, 5, 400_000), hunt(9, 3, 2, 5, 400_000));
        assert_eq!(
            first.best().unwrap().canonical(),
            second.best().unwrap().canonical()
        );
        assert_eq!(first.progress(), second.progress());
        let other = hunt(9, 3, 2, 6, 400_000);
        assert_ne!(first.progress().steps, other.progress().steps);
    }

    #[test]
    fn a_budget_can_be_spent_in_pieces() {
        let whole = hunt(9, 3, 2, 3, 400_000);
        let mut pieces = Search::new(9, 3, 2, 14, 3, Settings::default()).unwrap();
        while !pieces.run(777, 12) {}
        assert_eq!(
            pieces.best().unwrap().canonical(),
            whole.best().unwrap().canonical()
        );
        assert_eq!(pieces.progress().steps, whole.progress().steps);
    }

    #[test]
    fn the_nearest_miss_lacks_exactly_what_was_reported() {
        // No six blocks cover the pairs of seven points, so the search must end one level short.
        let mut search = Search::new(7, 3, 2, 8, 2, Settings::default()).unwrap();
        assert!(!search.run(50_000, 6));
        let (level, open) = search.progress().lowest.unwrap();
        let near = search.closest().unwrap();
        assert_eq!((level, near.blocks.len()), (6, 6));
        assert_eq!(check(&near).unwrap().uncovered, open as u64);
        assert_eq!(search.best().unwrap().blocks.len(), 7);
    }

    #[test]
    fn patience_follows_the_cost_of_a_step() {
        assert_eq!(
            Settings::for_blocks(17, 2).patience,
            20_000_000_000 / (24 * 2 * 17 * 17)
        );
        assert!(Settings::for_blocks(9, 2).patience > Settings::for_blocks(17, 2).patience);
        assert_eq!(Settings::for_blocks(60, 3).patience, 100_000);
        assert_eq!(
            Settings {
                patience: 0,
                ..Settings::for_blocks(9, 2)
            },
            Settings {
                patience: 0,
                ..Settings::default()
            }
        );
    }

    #[test]
    fn bad_requests_are_refused() {
        let settings = Settings::default;
        assert!(Search::new(7, 3, 4, 7, 1, settings()).is_err());
        assert!(Search::new(7, 2, 2, 7, 1, settings()).is_err());
        assert!(Search::new(7, 7, 2, 7, 1, settings()).is_err());
        assert!(Search::new(7, 3, 2, 0, 1, settings()).is_err());
        assert!(Search::new(
            7,
            3,
            2,
            7,
            1,
            Settings {
                smooth: 0,
                ..settings()
            }
        )
        .is_err());
    }
}
