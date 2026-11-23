//! `zetema calldata` against the call data that `cast calldata` prints for the same arguments.

use std::process::Command;
use std::{env, fs, process};

/// The Fano plane, with its blocks and the points inside them out of order.
const FANO: &str = "zetema/1 covering 7 3 2\n7 6 1\n3 2 1\n5 4 1\n6 4 2\n7 5 2\n7 4 3\n6 5 3\n";

/// `cast calldata "submit(uint16,uint8,bytes,address)" 7 3 0x000102000304000506010305010406020306020405
/// 0x0000000000000000000000000000000000000000`
const FANO_CALL: &str = "0x83c60a2f000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000150001020003040005060103050104060203060204050000000000000000000000";

/// The same with the sponsor 0x0123456789abcdef0123456789abcdef01234567.
const FANO_CALL_SPONSORED: &str = "0x83c60a2f0000000000000000000000000000000000000000000000000000000000000007000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000000000800000000000000000000000000123456789abcdef0123456789abcdef0123456700000000000000000000000000000000000000000000000000000000000000150001020003040005060103050104060203060204050000000000000000000000";

/// Runs `zetema calldata` on `text`, kept in a file named after `name`: whether it succeeded, what it
/// printed and what it reported.
fn calldata(name: &str, text: &str, options: &[&str]) -> (bool, String, String) {
    let path = env::temp_dir().join(format!("zetema-calldata-{}-{name}.txt", process::id()));
    fs::write(&path, text).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_zetema"))
        .arg("calldata")
        .arg(&path)
        .args(options)
        .output()
        .unwrap();
    fs::remove_file(&path).unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn the_fano_plane_encodes_as_cast_does() {
    let plain = (true, format!("{FANO_CALL}\n"), String::new());
    assert_eq!(calldata("plain", FANO, &[]), plain);
    let sponsored = (true, format!("{FANO_CALL_SPONSORED}\n"), String::new());
    let lower = ["--sponsor", "0x0123456789abcdef0123456789abcdef01234567"];
    let upper = ["--sponsor", "0x0123456789ABCDEF0123456789ABCDEF01234567"];
    assert_eq!(calldata("lower", FANO, &lower), sponsored);
    assert_eq!(calldata("upper", FANO, &upper), sponsored);
}

#[test]
fn what_the_registry_would_refuse_is_refused() {
    let refused = |name: &str, text: &str, options: &[&str], reason: &str| {
        let (ok, out, err) = calldata(name, text, options);
        assert!(
            !ok && out.is_empty() && err.contains(reason),
            "{name}: {err}"
        );
    };
    refused("t3", "zetema/1 covering 8 4 3\n1 2 3 4\n", &[], "(v, k, 2)");
    refused(
        "v256",
        "zetema/1 covering 256 3 2\n1 2 3\n",
        &[],
        "(v, k, 2)",
    );
    refused(
        "one-block",
        "zetema/1 covering 3 3 2\n1 2 3\n",
        &[],
        "(v, k, 2)",
    );
    refused("open", &FANO.replace("6 5 3\n", ""), &[], "uncovered");
    refused("twice", &format!("{FANO}1 2 3\n"), &[], "given twice");
    refused("no-value", FANO, &["--sponsor"], "needs a value");
    for bad in [
        "0x0123",
        "0123456789abcdef0123456789abcdef0123456789",
        "0x0123456789abcdef0123456789abcdef0123456g",
        "0x0123456789abcdef0123456789abcdef012345678",
        "0x+123456789abcdef0123456789abcdef01234567",
    ] {
        refused("address", FANO, &["--sponsor", bad], "not an address");
    }
}
