//! A hunt over a few entries whose answers are known.

use std::sync::Mutex;
use std::time::Duration;
use std::{env, fs, process};
use zetema::lab::{hunt, parse_targets, Plan};
use zetema::{check, Covering};

#[test]
fn a_hunt_writes_checked_certificates() {
    let out = env::temp_dir().join(format!("zetema-hunt-{}", process::id()));
    let targets = parse_targets("7 3 2 8\n9 3 2 13\n13 4 2 14\n8 4 3 15\n9 4 2 8\n").unwrap();
    let near = out.join("near");
    let plan = Plan {
        workers: 3,
        limit: Duration::from_millis(1500),
        round: 1,
        out: out.clone(),
        near: Some((near.clone(), 9)),
    };
    let outcomes = Mutex::new(Vec::new());
    hunt(&targets, &plan, &|outcome| {
        outcomes.lock().unwrap().push(outcome.clone())
    })
    .unwrap();
    let outcomes = outcomes.into_inner().unwrap();
    assert_eq!(outcomes.len(), targets.len());

    for (v, k, t, blocks) in [(7, 3, 2, 7), (9, 3, 2, 12), (13, 4, 2, 13), (8, 4, 3, 14)] {
        let text = fs::read_to_string(out.join(format!("covering-{v}-{k}-{t}.txt"))).unwrap();
        let design = Covering::parse(&text).unwrap();
        assert_eq!(design.canonical(), text);
        assert_eq!(design.blocks.len(), blocks);
        assert!(check(&design).unwrap().is_covering());
        let outcome = outcomes
            .iter()
            .find(|o| (o.target.v, o.target.k, o.target.t, o.target.below) == (v, k, t, blocks + 1))
            .unwrap();
        assert_eq!(outcome.record.as_ref().unwrap().0, design.digest());
        assert!(outcome.json().contains("\"hit\":true"));
    }
    // Mills: C(9,4,2) = 8, one more than the Schoenheim bound, so no search can get below 8.
    // The attempt must end at seven blocks with some pairs open, and keep those blocks as a near miss.
    let impossible = outcomes
        .iter()
        .find(|o| o.target.v == 9 && o.target.k == 4)
        .unwrap();
    assert_eq!(
        (impossible.record.clone(), impossible.best),
        (None, Some(8))
    );
    assert!(impossible.json().contains("\"hit\":false"));
    let file = near.join("near-9-4-2.txt");
    let kept = Covering::parse(&fs::read_to_string(&file).unwrap()).unwrap();
    let (level, open) = impossible.lowest.unwrap();
    assert_eq!((level, kept.blocks.len()), (7, 7));
    assert_eq!(check(&kept).unwrap().uncovered, open as u64);
    assert_eq!(impossible.near, Some(file));
    assert!(outcomes
        .iter()
        .filter(|o| o.record.is_some())
        .all(|o| o.near.is_none()));
    fs::remove_dir_all(&out).unwrap();
}
