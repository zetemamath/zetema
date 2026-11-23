//! The checker agrees with a plain count, and the text form is stable under reordering.

use zetema::rng::Rng;
use zetema::{check, Covering};

/// Calls `visit` with every t-subset of `0..v`, in ascending order.
fn subsets(v: u8, t: usize, from: u8, chosen: &mut Vec<u8>, visit: &mut impl FnMut(&[u8])) {
    if chosen.len() == t {
        visit(chosen);
        return;
    }
    for p in from..v {
        chosen.push(p);
        subsets(v, t, p + 1, chosen, visit);
        chosen.pop();
    }
}

fn random_block(rng: &mut Rng, v: usize, k: usize) -> Vec<u8> {
    let mut pool: Vec<u8> = (0..v as u8).collect();
    for i in 0..k {
        let j = i + rng.below(v - i);
        pool.swap(i, j);
    }
    pool.truncate(k);
    pool
}

fn random_design(rng: &mut Rng) -> Covering {
    let v = 1 + rng.below(12);
    let k = 1 + rng.below(v.min(6));
    let t = 1 + rng.below(k.min(4));
    let count = rng.below(12);
    let blocks = (0..count).map(|_| random_block(rng, v, k)).collect();
    Covering { v, k, t, blocks }
}

#[test]
fn checker_matches_a_naive_count() {
    let mut rng = Rng::new(20_261_123);
    for case in 0..300 {
        let design = random_design(&mut rng);
        let report = check(&design).unwrap_or_else(|fault| panic!("case {case}: {fault}"));
        let covered = |set: &[u8]| {
            design
                .blocks
                .iter()
                .any(|block| set.iter().all(|p| block.contains(p)))
        };
        let (mut sets, mut open) = (0u64, 0u64);
        subsets(design.v as u8, design.t, 0, &mut Vec::new(), &mut |set| {
            sets += 1;
            open += u64::from(!covered(set));
        });
        assert_eq!(report.sets, sets, "case {case}: {design:?}");
        assert_eq!(report.uncovered, open, "case {case}: {design:?}");
        match report.witness {
            Some(witness) => {
                assert!(open > 0, "case {case}: witness without open sets");
                assert_eq!(witness.len(), design.t, "case {case}");
                assert!(witness.iter().all(|&p| usize::from(p) < design.v));
                assert!(
                    !covered(&witness),
                    "case {case}: witness {witness:?} is covered"
                );
            }
            None => assert_eq!(open, 0, "case {case}: open sets but no witness"),
        }
    }
}

#[test]
fn canonical_text_is_a_fixed_point() {
    let mut rng = Rng::new(7);
    for _ in 0..50 {
        let text = random_design(&mut rng).canonical();
        let again = Covering::parse(&text).expect("canonical text parses");
        assert_eq!(again.canonical(), text);
    }
}

#[test]
fn digest_ignores_block_and_point_order() {
    let mut rng = Rng::new(11);
    for _ in 0..50 {
        let design = random_design(&mut rng);
        let mut shuffled = design.clone();
        for block in &mut shuffled.blocks {
            for i in (1..block.len()).rev() {
                block.swap(i, rng.below(i + 1));
            }
        }
        for i in (1..shuffled.blocks.len()).rev() {
            shuffled.blocks.swap(i, rng.below(i + 1));
        }
        assert_eq!(shuffled.digest(), design.digest());
        assert_eq!(shuffled.canonical(), design.canonical());
    }
}
