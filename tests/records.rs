//! Every certificate kept in `records/` must be in canonical form and pass the checker.

use std::fs;
use zetema::bound::schoenheim;
use zetema::{check, Covering};

fn certificates() -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = fs::read_dir("records")
        .expect("the records directory exists")
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .map(|path| {
            (
                path.display().to_string(),
                fs::read_to_string(&path).expect("a readable certificate"),
            )
        })
        .collect();
    found.sort();
    found
}

#[test]
fn every_record_is_canonical_and_covers() {
    let all = certificates();
    assert!(!all.is_empty(), "no certificates found");
    for (path, text) in all {
        let design = Covering::parse(&text).unwrap_or_else(|error| panic!("{path}: {error}"));
        assert_eq!(design.canonical(), text, "{path} is not in canonical form");
        let report = check(&design).unwrap_or_else(|fault| panic!("{path}: {fault}"));
        assert!(
            report.is_covering(),
            "{path} leaves {} sets uncovered",
            report.uncovered
        );
        let name = format!("covering-{}-{}-{}.txt", design.v, design.k, design.t);
        assert!(path.ends_with(&name), "{path} should be named {name}");
    }
}

#[test]
fn the_first_record_is_exact() {
    let text =
        fs::read_to_string("records/covering-83-17-2.txt").expect("the first record is kept");
    let design = Covering::parse(&text).unwrap();
    let report = check(&design).unwrap();
    assert_eq!(
        (report.blocks, report.sets, report.uncovered),
        (30, 3403, 0)
    );
    assert_eq!(
        schoenheim(83, 17, 2),
        30,
        "30 blocks meet the lower bound, so C(83,17,2) = 30"
    );
    assert_eq!(
        design.digest(),
        "a460d59ff9e4a87ec447280aa538d9b73880b453c691922487c775046ac6d7c9"
    );
}
