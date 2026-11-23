//! The exact checker.
//!
//! It marks every t-set that lies inside some block and counts the t-sets left unmarked. A set of
//! points `p1 < p2 < ... < pt` (numbered from 0) gets the index `C(p1, 1) + C(p2, 2) + ... + C(pt, t)`,
//! its place in colexicographic order, so one bit per t-set is enough. There is no search, no
//! tolerance and no randomness: the same blocks always give the same answer.

use crate::design::{Covering, MAX_POINTS};
use std::fmt;

/// The most t-sets the checker will mark: 2^28, which is 32 MiB of marks.
pub const MAX_SETS: u64 = 1 << 28;

/// What the checker found in a well-formed design.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// Number of blocks.
    pub blocks: usize,
    /// Number of t-sets of points, `C(v, t)`.
    pub sets: u64,
    /// How many of them lie in no block.
    pub uncovered: u64,
    /// One t-set that lies in no block (points from 0, ascending), if there is any.
    pub witness: Option<Vec<u8>>,
}

impl Report {
    /// True when every t-set lies in at least one block.
    pub fn is_covering(&self) -> bool {
        self.uncovered == 0
    }
}

/// Why a design is not well formed. Blocks count from 1, as lines do in a certificate; points too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The parameters do not satisfy `1 <= t <= k <= v <= 255`.
    Parameters {
        /// Number of points.
        v: usize,
        /// Points per block.
        k: usize,
        /// Size of the sets to cover.
        t: usize,
    },
    /// `C(v, t)` is above [`MAX_SETS`].
    TooManySets {
        /// How many t-sets there are.
        sets: u64,
    },
    /// A block does not have exactly `k` points.
    BlockSize {
        /// Which block, from 1.
        block: usize,
        /// How many points it holds.
        len: usize,
    },
    /// A block names a point above `v`.
    PointRange {
        /// Which block, from 1.
        block: usize,
        /// The offending point, from 1.
        point: usize,
    },
    /// A block names a point twice.
    RepeatedPoint {
        /// Which block, from 1.
        block: usize,
        /// The repeated point, from 1.
        point: usize,
    },
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Parameters { v, k, t } => {
                write!(
                    f,
                    "parameters v={v} k={k} t={t} do not satisfy 1 <= t <= k <= v <= {MAX_POINTS}"
                )
            }
            Self::TooManySets { sets } => {
                write!(f, "{sets} sets to mark, the checker stops at {MAX_SETS}")
            }
            Self::BlockSize { block, len } => write!(f, "block {block} has {len} points"),
            Self::PointRange { block, point } => {
                write!(f, "block {block} names point {point}, which does not exist")
            }
            Self::RepeatedPoint { block, point } => {
                write!(f, "block {block} names point {point} twice")
            }
        }
    }
}

impl std::error::Error for Fault {}

/// Checks a design: first that it is well formed, then how many t-sets its blocks leave uncovered.
pub fn check(design: &Covering) -> Result<Report, Fault> {
    let (v, k, t) = (design.v, design.k, design.t);
    if t < 1 || t > k || k > v || v > MAX_POINTS {
        return Err(Fault::Parameters { v, k, t });
    }
    let choose = Choose::new(v, t);
    let sets = choose.get(v, t);
    if sets > MAX_SETS {
        return Err(Fault::TooManySets { sets });
    }
    let mut marks = vec![0u64; (sets as usize).div_ceil(64)];
    let mut sorted: Vec<u8> = Vec::with_capacity(k);
    let mut pick = vec![0usize; t];
    for (index, block) in design.blocks.iter().enumerate() {
        let number = index + 1;
        if block.len() != k {
            return Err(Fault::BlockSize {
                block: number,
                len: block.len(),
            });
        }
        sorted.clear();
        sorted.extend_from_slice(block);
        sorted.sort_unstable();
        if let Some(&point) = sorted.iter().find(|&&point| usize::from(point) >= v) {
            return Err(Fault::PointRange {
                block: number,
                point: usize::from(point) + 1,
            });
        }
        if let Some(pair) = sorted.windows(2).find(|pair| pair[0] == pair[1]) {
            return Err(Fault::RepeatedPoint {
                block: number,
                point: usize::from(pair[0]) + 1,
            });
        }
        for (place, slot) in pick.iter_mut().enumerate() {
            *slot = place;
        }
        loop {
            let rank: u64 = pick
                .iter()
                .enumerate()
                .map(|(i, &at)| choose.get(usize::from(sorted[at]), i + 1))
                .sum();
            marks[(rank / 64) as usize] |= 1 << (rank % 64);
            if !next_subset(&mut pick, k) {
                break;
            }
        }
    }
    let covered: u64 = marks.iter().map(|word| u64::from(word.count_ones())).sum();
    let uncovered = sets - covered;
    let witness = (uncovered > 0).then(|| {
        let (word, bits) = marks
            .iter()
            .enumerate()
            .find(|(_, &bits)| bits != u64::MAX)
            .expect("an unmarked set exists");
        choose.unrank(word as u64 * 64 + u64::from((!bits).trailing_zeros()), t)
    });
    Ok(Report {
        blocks: design.blocks.len(),
        sets,
        uncovered,
        witness,
    })
}

/// Steps `pick`, an increasing choice of positions below `k`, to the next one. False after the last.
fn next_subset(pick: &mut [usize], k: usize) -> bool {
    let t = pick.len();
    for i in (0..t).rev() {
        if pick[i] < i + k - t {
            pick[i] += 1;
            for j in i + 1..t {
                pick[j] = pick[j - 1] + 1;
            }
            return true;
        }
    }
    false
}

/// Binomial coefficients `C(n, j)` for `n <= v`, `j <= t`, saturating at `u64::MAX`.
struct Choose {
    width: usize,
    table: Vec<u64>,
}

impl Choose {
    fn new(v: usize, t: usize) -> Self {
        let width = t + 1;
        let mut table = vec![0u64; (v + 1) * width];
        for n in 0..=v {
            table[n * width] = 1;
            for j in 1..=t.min(n) {
                table[n * width + j] =
                    table[(n - 1) * width + j - 1].saturating_add(table[(n - 1) * width + j]);
            }
        }
        Self { width, table }
    }

    fn get(&self, n: usize, j: usize) -> u64 {
        self.table[n * self.width + j]
    }

    /// The t-set whose colexicographic index is `rank`.
    fn unrank(&self, mut rank: u64, t: usize) -> Vec<u8> {
        let mut points = vec![0u8; t];
        let mut top = self.table.len() / self.width - 1;
        for j in (1..=t).rev() {
            let point = (j - 1..top)
                .rev()
                .find(|&p| self.get(p, j) <= rank)
                .expect("rank is below C(v, t)");
            rank -= self.get(point, j);
            points[j - 1] = point as u8;
            top = point;
        }
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn design(v: usize, k: usize, t: usize, blocks: &[&[u8]]) -> Covering {
        Covering {
            v,
            k,
            t,
            blocks: blocks.iter().map(|block| block.to_vec()).collect(),
        }
    }

    const FANO: [&[u8]; 7] = [
        &[0, 1, 2],
        &[0, 3, 4],
        &[0, 5, 6],
        &[1, 3, 5],
        &[1, 4, 6],
        &[2, 3, 6],
        &[2, 4, 5],
    ];

    #[test]
    fn the_fano_plane_covers_every_pair() {
        let report = check(&design(7, 3, 2, &FANO)).unwrap();
        assert_eq!(
            report,
            Report {
                blocks: 7,
                sets: 21,
                uncovered: 0,
                witness: None
            }
        );
        assert!(report.is_covering());
    }

    #[test]
    fn a_missing_line_leaves_its_three_pairs_open() {
        let report = check(&design(7, 3, 2, &FANO[..6])).unwrap();
        assert!(!report.is_covering());
        assert_eq!((report.uncovered, report.witness), (3, Some(vec![2, 4])));
    }

    #[test]
    fn the_planes_of_the_cube_cover_every_triple() {
        // The 14 planes of the affine space of order 2 and dimension 3: the Steiner system S(3, 4, 8).
        let mut blocks = Vec::new();
        for normal in 1u8..8 {
            for side in 0..2 {
                blocks.push(
                    (0u8..8)
                        .filter(|&x| (x & normal).count_ones() % 2 == side)
                        .collect::<Vec<u8>>(),
                );
            }
        }
        let report = check(&Covering {
            v: 8,
            k: 4,
            t: 3,
            blocks,
        })
        .unwrap();
        assert_eq!((report.blocks, report.sets, report.uncovered), (14, 56, 0));
    }

    #[test]
    fn order_inside_a_block_does_not_matter() {
        let shuffled: [&[u8]; 7] = [
            &[2, 0, 1],
            &[4, 3, 0],
            &[6, 0, 5],
            &[5, 3, 1],
            &[1, 6, 4],
            &[6, 2, 3],
            &[4, 5, 2],
        ];
        assert!(check(&design(7, 3, 2, &shuffled)).unwrap().is_covering());
    }

    #[test]
    fn no_blocks_cover_nothing() {
        let report = check(&design(5, 3, 2, &[])).unwrap();
        assert_eq!(
            (report.sets, report.uncovered, report.witness),
            (10, 10, Some(vec![0, 1]))
        );
    }

    #[test]
    fn one_block_of_all_points_covers_everything() {
        assert!(check(&design(4, 4, 3, &[&[3, 2, 1, 0]]))
            .unwrap()
            .is_covering());
    }

    #[test]
    fn the_witness_is_really_uncovered() {
        let blocks: [&[u8]; 3] = [&[0, 1, 2, 3], &[0, 4, 5, 6], &[1, 4, 7, 8]];
        let report = check(&design(9, 4, 3, &blocks)).unwrap();
        let witness = report.witness.unwrap();
        assert!(witness.windows(2).all(|pair| pair[0] < pair[1]) && witness.len() == 3);
        assert!(!blocks
            .iter()
            .any(|block| witness.iter().all(|point| block.contains(point))));
        assert_eq!(report.uncovered, 84 - 3 * 4);
    }

    #[test]
    fn malformed_designs_are_named() {
        assert_eq!(
            check(&design(7, 3, 0, &FANO)),
            Err(Fault::Parameters { v: 7, k: 3, t: 0 })
        );
        assert_eq!(
            check(&design(7, 8, 2, &FANO)),
            Err(Fault::Parameters { v: 7, k: 8, t: 2 })
        );
        assert_eq!(
            check(&design(7, 3, 4, &FANO)),
            Err(Fault::Parameters { v: 7, k: 3, t: 4 })
        );
        assert_eq!(
            check(&design(256, 3, 2, &FANO)),
            Err(Fault::Parameters { v: 256, k: 3, t: 2 })
        );
        assert_eq!(
            check(&design(7, 3, 2, &[&[0, 1]])),
            Err(Fault::BlockSize { block: 1, len: 2 })
        );
        assert_eq!(
            check(&design(7, 3, 2, &[&[0, 1, 2], &[0, 1, 7]])),
            Err(Fault::PointRange { block: 2, point: 8 })
        );
        assert_eq!(
            check(&design(7, 3, 2, &[&[4, 1, 4]])),
            Err(Fault::RepeatedPoint { block: 1, point: 5 })
        );
        assert!(matches!(
            check(&design(255, 9, 8, &[])),
            Err(Fault::TooManySets { .. })
        ));
    }
}
