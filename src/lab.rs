//! Running many searches side by side.
//!
//! A hunt takes a list of targets (parameters and the block count to beat), gives each one a
//! search with its own seed and a time limit, and reports every outcome as one line of JSON. A
//! covering that beats its target is handed to the checker first and written as a certificate
//! only if the checker agrees.

use crate::bound::schoenheim;
use crate::check::check;
use crate::design::Covering;
use crate::search::{Search, Settings};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use std::{fs, thread};

/// One entry to attack: find a (v, k, t) covering with fewer than `below` blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    /// Number of points.
    pub v: usize,
    /// Points per block.
    pub k: usize,
    /// Size of the sets to cover.
    pub t: usize,
    /// The block count to beat.
    pub below: usize,
}

/// How a hunt is run.
#[derive(Clone, Debug)]
pub struct Plan {
    /// Searches running at the same time.
    pub workers: usize,
    /// Time limit of one search.
    pub limit: Duration,
    /// Mixed into every seed, so that a second pass over the same targets tries new runs.
    pub round: u64,
    /// Where certificates of coverings that beat their target are written.
    pub out: PathBuf,
    /// Where the nearest misses are written, and how many open sets still count as near.
    pub near: Option<(PathBuf, usize)>,
}

/// What one search came to.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    /// The entry that was attacked.
    pub target: Target,
    /// The seed of this search.
    pub seed: u64,
    /// Blocks in the smallest covering found, if any.
    pub best: Option<usize>,
    /// The lowest level tried without a covering, and the fewest sets left open there.
    pub lowest: Option<(usize, usize)>,
    /// Local search steps taken.
    pub steps: u64,
    /// How many times the search restarted.
    pub retries: u64,
    /// Wall-clock time spent.
    pub seconds: f64,
    /// Digest and file of the certificate, when the target was beaten and the checker agreed.
    pub record: Option<(String, PathBuf)>,
    /// File holding the nearest miss: the blocks one level below the best covering, or below the target
    /// when no covering was found, if only a few sets were open there.
    pub near: Option<PathBuf>,
}

impl Outcome {
    /// The outcome as one line of JSON.
    pub fn json(&self) -> String {
        let number =
            |value: Option<usize>| value.map_or("null".to_string(), |value| value.to_string());
        let (digest, file) = match &self.record {
            Some((digest, file)) => (format!("\"{digest}\""), format!("\"{}\"", file.display())),
            None => ("null".to_string(), "null".to_string()),
        };
        let near = self
            .near
            .as_ref()
            .map_or("null".to_string(), |file| format!("\"{}\"", file.display()));
        let Target { v, k, t, below } = self.target;
        format!(
            "{{\"v\":{v},\"k\":{k},\"t\":{t},\"below\":{below},\"hit\":{},\"best\":{},\"lowest\":{},\"open\":{},\
             \"steps\":{},\"retries\":{},\"seconds\":{:.1},\"seed\":{},\"sha256\":{digest},\"file\":{file},\"near\":{near}}}",
            self.record.is_some(),
            number(self.best),
            number(self.lowest.map(|low| low.0)),
            number(self.lowest.map(|low| low.1)),
            self.steps,
            self.retries,
            self.seconds,
            self.seed,
        )
    }
}

/// Reads targets, one per line as `V K T BELOW`. Blank lines and lines starting with `#` are skipped.
pub fn parse_targets(text: &str) -> Result<Vec<Target>, String> {
    let mut targets = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<usize> = line
            .split_whitespace()
            .map_while(|word| word.parse().ok())
            .collect();
        let bad = || format!("line {}: expected `V K T BELOW`, got `{line}`", index + 1);
        let [v, k, t, below] = <[usize; 4]>::try_from(fields).map_err(|_| bad())?;
        if line.split_whitespace().count() != 4 || below < 2 {
            return Err(bad());
        }
        targets.push(Target { v, k, t, below });
    }
    Ok(targets)
}

/// The seed of a target's search: fixed by the parameters and the round.
pub fn seed_for(target: Target, round: u64) -> u64 {
    let mut seed = round.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x7A65_7465_6D61;
    for part in [target.v, target.k, target.t, target.below] {
        seed = (seed ^ part as u64)
            .wrapping_mul(0x0000_0100_0000_01B3)
            .rotate_left(23);
    }
    seed
}

/// Runs every target and calls `report` once per finished search, from the worker that ran it.
pub fn hunt(
    targets: &[Target],
    plan: &Plan,
    report: &(dyn Fn(&Outcome) + Sync),
) -> Result<(), String> {
    fs::create_dir_all(&plan.out).map_err(|error| format!("{}: {error}", plan.out.display()))?;
    let queue = Mutex::new(targets.iter().copied().collect::<VecDeque<Target>>());
    let failure = Mutex::new(None);
    thread::scope(|scope| {
        for _ in 0..plan.workers.max(1) {
            scope.spawn(|| loop {
                let next = queue
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .pop_front();
                let Some(target) = next else { break };
                match attack(target, plan) {
                    Ok(outcome) => report(&outcome),
                    Err(error) => {
                        *failure
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error)
                    }
                }
            });
        }
    });
    let failure = failure
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    failure.map_or(Ok(()), Err)
}

/// One search, from its seed to its outcome.
fn attack(target: Target, plan: &Plan) -> Result<Outcome, String> {
    let Target { v, k, t, below } = target;
    let seed = seed_for(target, plan.round);
    let mut search = Search::new(v, k, t, below + 2, seed, Settings::for_blocks(k, t))
        .map_err(|error| format!("C({v},{k},{t}): {error}"))?;
    let floor = schoenheim(v as u64, k as u64, t as u64) as usize;
    let clock = Instant::now();
    while !search.run(2_000, floor) && clock.elapsed() < plan.limit {}
    let progress = search.progress();
    let record = match search.best().filter(|best| best.blocks.len() < below) {
        Some(best) => Some(keep(best, &plan.out)?),
        None => None,
    };
    // The blocks one level below the best covering (or below the target, when nothing was found) are
    // worth keeping if only a few sets were open there: a different attack can start from them.
    let near = match (&plan.near, progress.lowest) {
        (Some((dir, most)), Some((level, open)))
            if level < below && open <= *most && progress.best.is_none_or(|best| level < best) =>
        {
            let design = search
                .closest()
                .expect("a lowest level comes with its blocks");
            let file = dir.join(format!("near-{v}-{k}-{t}.txt"));
            fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
            fs::write(&file, design.canonical())
                .map_err(|error| format!("{}: {error}", file.display()))?;
            Some(file)
        }
        _ => None,
    };
    Ok(Outcome {
        target,
        seed,
        best: progress.best,
        lowest: progress.lowest,
        steps: progress.steps,
        retries: progress.retries,
        seconds: clock.elapsed().as_secs_f64(),
        record,
        near,
    })
}

/// Checks a covering and writes its certificate, unless a smaller one for the same parameters is
/// already in the directory. Returns the digest and the file.
fn keep(best: &Covering, out: &Path) -> Result<(String, PathBuf), String> {
    let name = best.name();
    let report = check(best)
        .map_err(|fault| format!("{name}: the search produced a malformed design: {fault}"))?;
    if !report.is_covering() {
        return Err(format!(
            "{name}: the search reported a covering that leaves {} sets open",
            report.uncovered
        ));
    }
    let file = out.join(format!("covering-{}-{}-{}.txt", best.v, best.k, best.t));
    let smaller_kept = fs::read_to_string(&file)
        .ok()
        .and_then(|text| Covering::parse(&text).ok())
        .is_some_and(|kept| {
            kept.blocks.len() <= best.blocks.len()
                && check(&kept).is_ok_and(|kept| kept.is_covering())
        });
    if !smaller_kept {
        fs::write(&file, best.canonical())
            .map_err(|error| format!("{}: {error}", file.display()))?;
    }
    Ok((best.digest(), file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_read_line_by_line() {
        let targets = parse_targets("# small ones\n7 3 2 8\n\n  13 4 2 14  \n").unwrap();
        assert_eq!(
            targets,
            vec![
                Target {
                    v: 7,
                    k: 3,
                    t: 2,
                    below: 8
                },
                Target {
                    v: 13,
                    k: 4,
                    t: 2,
                    below: 14
                }
            ]
        );
        assert!(parse_targets("7 3 2").unwrap_err().starts_with("line 1"));
        assert!(parse_targets("7 3 2 8 9").is_err());
        assert!(parse_targets("7 3 2 x").is_err());
        assert!(parse_targets("7 3 2 1").is_err());
    }

    #[test]
    fn seeds_depend_on_target_and_round() {
        let (fano, plane) = (
            Target {
                v: 7,
                k: 3,
                t: 2,
                below: 8,
            },
            Target {
                v: 13,
                k: 4,
                t: 2,
                below: 14,
            },
        );
        assert_eq!(seed_for(fano, 1), seed_for(fano, 1));
        assert_ne!(seed_for(fano, 1), seed_for(fano, 2));
        assert_ne!(seed_for(fano, 1), seed_for(plane, 1));
    }

    #[test]
    fn an_outcome_is_one_json_object() {
        let outcome = Outcome {
            target: Target {
                v: 7,
                k: 3,
                t: 2,
                below: 8,
            },
            seed: 5,
            best: Some(7),
            lowest: None,
            steps: 40,
            retries: 0,
            seconds: 0.04,
            record: Some(("ab".to_string(), PathBuf::from("out/covering-7-3-2.txt"))),
            near: None,
        };
        assert_eq!(
            outcome.json(),
            "{\"v\":7,\"k\":3,\"t\":2,\"below\":8,\"hit\":true,\"best\":7,\"lowest\":null,\"open\":null,\
             \"steps\":40,\"retries\":0,\"seconds\":0.0,\"seed\":5,\"sha256\":\"ab\",\"file\":\"out/covering-7-3-2.txt\",\"near\":null}"
        );
    }
}
