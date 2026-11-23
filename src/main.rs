//! Command line: check certificates, print bounds, run one search or a whole hunt, and turn a record
//! into the call that submits it to the registry on chain.

use std::fmt::Display;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use std::{env, fs};
use zetema::bound::schoenheim;
use zetema::design::MAX_POINTS;
use zetema::lab::{hunt, parse_targets, Plan};
use zetema::search::{Search, Settings};
use zetema::{check, Covering};

const USAGE: &str = "\
zetema check FILE...               check certificates; print size, bound and digest
zetema bound V K T                 the Schoenheim lower bound for (V, K, T) coverings
zetema import V K T FILE           turn a bare list of blocks into a certificate
zetema calldata FILE               call data for CoveringRegistry.submit, from a t = 2 certificate
    --sponsor A      the account the finder credits for the search (default: none)
zetema search V K T --below N      look for a covering with fewer than N blocks
    --start B        blocks to start from (default N + 2)
    --seed S         seed of the run (default 1)
    --seconds S      stop after this long (default 60)
    --steps N        stop after this many steps (default: no limit)
    --out FILE       write the smallest covering below N to this file
    --tabu N --noise N --flat N --walk N --smooth N --cap N --kick N --patience N
                     search settings (defaults are the calibrated ones; patience follows K and T)
zetema hunt TARGETS --out DIR      attack every target in a file of lines `V K T BELOW`
    --workers N      searches at a time (default 1)
    --seconds S      time limit of one search (default 60)
    --round R        mixed into every seed; use a new one for another pass (default 1)
    --log FILE       also append the outcomes, one line of JSON each, to this file
    --near DIR       keep the blocks of searches that ended at most --near-open sets short
    --near-open N    (default 4)
    --status FILE    rewrite FILE about once a second with how far every running search has come
zetema --version                   the version of this build";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        Some("check") if args.len() > 1 => check_files(&args[1..]),
        Some("bound") if args.len() == 4 => bound(&args[1..]),
        Some("import") if args.len() == 5 => import(&args[1..]),
        Some("calldata") if args.len() >= 2 => calldata(&args[1..]),
        Some("search") if args.len() >= 4 => search(&args[1..]),
        Some("hunt") if args.len() >= 2 => run_hunt(&args[1..]),
        Some("--version" | "-V") => {
            println!("zetema {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        Some("--help" | "-h" | "help") => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(USAGE.to_string()),
    };
    match outcome {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn number<T: FromStr>(text: &str, what: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("`{text}` is not a valid {what}"))
}

fn parameters(args: &[String]) -> Result<(usize, usize, usize), String> {
    let (v, k, t) = (
        number(&args[0], "V")?,
        number(&args[1], "K")?,
        number(&args[2], "T")?,
    );
    if t < 1 || t > k || k > v {
        return Err(format!(
            "need 1 <= T <= K <= V, got V = {v}, K = {k}, T = {t}"
        ));
    }
    Ok((v, k, t))
}

fn read(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))
}

/// One line per file. The exit code is 0 only if every file holds a real covering.
fn check_files(paths: &[String]) -> Result<ExitCode, String> {
    let mut all_good = true;
    for path in paths {
        let line = match judge(&read(path)?) {
            Ok(line) => format!("ok    {line}"),
            Err(reason) => {
                all_good = false;
                format!("FAIL  {reason}")
            }
        };
        println!("{line}  {path}");
    }
    Ok(if all_good {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn judge(text: &str) -> Result<String, String> {
    let design = Covering::parse(text).map_err(|error| error.to_string())?;
    let report = check(&design).map_err(|fault| fault.to_string())?;
    if let Some(witness) = &report.witness {
        let points: Vec<String> = witness
            .iter()
            .map(|&point| (usize::from(point) + 1).to_string())
            .collect();
        return Err(format!(
            "{} with {} blocks leaves {} of {} sets uncovered, for example {{{}}}",
            design.name(),
            report.blocks,
            report.uncovered,
            report.sets,
            points.join(", ")
        ));
    }
    let bound = schoenheim(design.v as u64, design.k as u64, design.t as u64);
    let standing = if report.blocks as u64 == bound {
        "exact"
    } else {
        "upper bound"
    };
    Ok(format!(
        "{} <= {}  lower bound {bound}  {standing}  sha256 {}",
        design.name(),
        report.blocks,
        design.digest()
    ))
}

fn bound(args: &[String]) -> Result<ExitCode, String> {
    let (v, k, t) = parameters(args)?;
    println!("{}", schoenheim(v as u64, k as u64, t as u64));
    Ok(ExitCode::SUCCESS)
}

fn import(args: &[String]) -> Result<ExitCode, String> {
    let (v, k, t) = parameters(args)?;
    let design =
        Covering::from_block_list(v, k, t, &read(&args[3])?).map_err(|error| error.to_string())?;
    let report = check(&design).map_err(|fault| fault.to_string())?;
    if !report.is_covering() {
        return Err(format!(
            "{} sets are uncovered: not a covering, nothing written",
            report.uncovered
        ));
    }
    print!("{design}");
    Ok(ExitCode::SUCCESS)
}

/// The selector of `submit(uint16,uint8,bytes,address)`, as `cast sig` prints it.
const SUBMIT: &str = "0x83c60a2f";

/// Prints the call that submits a certificate to the registry, as one line of hex.
fn calldata(args: &[String]) -> Result<ExitCode, String> {
    let mut sponsor = "0".repeat(40);
    let mut options = args[1..].iter();
    while let Some(option) = options.next() {
        let value = options
            .next()
            .ok_or_else(|| format!("{option} needs a value"))?;
        match option.as_str() {
            "--sponsor" => sponsor = address(value)?,
            _ => return Err(format!("unknown option {option}\n{USAGE}")),
        }
    }
    let design = Covering::parse(&read(&args[0])?).map_err(|error| error.to_string())?;
    println!("{}", submit_call(&design, &sponsor)?);
    Ok(ExitCode::SUCCESS)
}

/// The 40 hex digits of an address written as `0x` and 40 hex digits, in lower case. A mixed-case
/// address is taken as it is: checking its EIP-55 checksum would need Keccak-256.
fn address(text: &str) -> Result<String, String> {
    match text.strip_prefix("0x") {
        Some(digits) if digits.len() == 40 && digits.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Ok(digits.to_ascii_lowercase())
        }
        _ => Err(format!(
            "`{text}` is not an address: expected 0x and 40 hex digits"
        )),
    }
}

/// ABI-encoded call data for `submit(uint16 v, uint8 k, bytes blocks, address sponsor)`, with the
/// blocks in canonical order, one byte per point from 0. Refuses what the registry would refuse
/// whatever its record: anything but a (v, k, 2) covering with k < v <= 255, one that leaves a pair
/// uncovered, and a block given twice.
fn submit_call(design: &Covering, sponsor: &str) -> Result<String, String> {
    let (v, k) = (design.v, design.k);
    if design.t != 2 || k >= v || v > MAX_POINTS {
        return Err(format!(
            "the registry takes (v, k, 2) coverings with k < v <= {MAX_POINTS}, not {}",
            design.name()
        ));
    }
    let report = check(design).map_err(|fault| fault.to_string())?;
    if !report.is_covering() {
        return Err(format!(
            "{} with {} blocks leaves {} of {} pairs uncovered",
            design.name(),
            report.blocks,
            report.uncovered,
            report.sets
        ));
    }
    let blocks = design.sorted_blocks();
    if let Some(pair) = blocks.windows(2).find(|pair| pair[0] == pair[1]) {
        let points: Vec<String> = pair[0]
            .iter()
            .map(|&point| (usize::from(point) + 1).to_string())
            .collect();
        return Err(format!(
            "block {{{}}} is given twice; the registry takes every block once",
            points.join(", ")
        ));
    }
    let points = blocks.concat();
    // The head holds v, k, where the bytes start (after the four head words) and the sponsor; then
    // come the length of the bytes and the bytes, padded to a whole word.
    let mut call = format!(
        "{SUBMIT}{v:064x}{k:064x}{:064x}{sponsor:0>64}{:064x}",
        4 * 32,
        points.len()
    );
    call.extend(points.iter().map(|point| format!("{point:02x}")));
    call.extend(std::iter::repeat_n(
        "00",
        points.len().next_multiple_of(32) - points.len(),
    ));
    Ok(call)
}

fn search(args: &[String]) -> Result<ExitCode, String> {
    let (v, k, t) = parameters(args)?;
    let (mut below, mut start, mut seed, mut seconds, mut steps, mut out) =
        (None, None, 1u64, 60.0f64, u64::MAX, None);
    let mut settings = Settings::for_blocks(k, t);
    let mut options = args[3..].iter();
    while let Some(option) = options.next() {
        let value = options
            .next()
            .ok_or_else(|| format!("{option} needs a value"))?;
        match option.as_str() {
            "--below" => below = Some(number::<usize>(value, "block count")?),
            "--start" => start = Some(number::<usize>(value, "block count")?),
            "--seed" => seed = number(value, "seed")?,
            "--seconds" => seconds = number(value, "number of seconds")?,
            "--steps" => steps = number(value, "number of steps")?,
            "--out" => out = Some(value.clone()),
            "--tabu" => settings.tabu = number(value, "setting")?,
            "--noise" => settings.noise = number(value, "setting")?,
            "--flat" => settings.flat = number(value, "setting")?,
            "--walk" => settings.walk = number(value, "setting")?,
            "--smooth" => settings.smooth = number(value, "setting")?,
            "--cap" => settings.cap = number(value, "setting")?,
            "--kick" => settings.kick = number(value, "setting")?,
            "--patience" => settings.patience = number(value, "setting")?,
            _ => return Err(format!("unknown option {option}\n{USAGE}")),
        }
    }
    let below = below.ok_or("search needs --below N")?;
    if below < 2 || !seconds.is_finite() || seconds < 0.0 {
        return Err("need --below of at least 2 and --seconds of at least 0".to_string());
    }
    let floor = schoenheim(v as u64, k as u64, t as u64) as usize;
    let mut run = Search::new(v, k, t, start.unwrap_or(below + 2), seed, settings)?;
    let (clock, limit) = (Instant::now(), Duration::from_secs_f64(seconds));
    let mut shown = usize::MAX;
    loop {
        let piece = steps.saturating_sub(run.progress().steps).min(2_000);
        let done = run.run(piece, floor);
        if let Some(best) = run.best().filter(|best| best.blocks.len() < shown) {
            shown = best.blocks.len();
            println!(
                "cover {shown}  step {}  {:.1} s",
                run.progress().steps,
                clock.elapsed().as_secs_f64()
            );
            if shown < below {
                publish(best, out.as_deref())?;
            }
        }
        if done || piece == 0 || clock.elapsed() >= limit {
            break;
        }
    }
    let progress = run.progress();
    let show = |value: Option<usize>| value.map_or("-".to_string(), |value| value.to_string());
    println!(
        "result C({v},{k},{t})  below {below}  best {}  lowest {}  open {}  steps {}  retries {}  {:.1} s  seed {seed}",
        show(progress.best),
        show(progress.lowest.map(|low| low.0)),
        show(progress.lowest.map(|low| low.1)),
        progress.steps,
        progress.retries,
        clock.elapsed().as_secs_f64()
    );
    Ok(if shown < below {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Writes a covering the search reported, but only after the checker has agreed with it.
fn publish(best: &Covering, out: Option<&str>) -> Result<(), String> {
    let report =
        check(best).map_err(|fault| format!("the search produced a malformed design: {fault}"))?;
    if !report.is_covering() {
        return Err(format!(
            "the search reported a covering that leaves {} sets open",
            report.uncovered
        ));
    }
    if let Some(path) = out {
        fs::write(path, best.canonical()).map_err(|error| describe(path, error))?;
    }
    Ok(())
}

fn describe(path: &str, error: impl Display) -> String {
    format!("{path}: {error}")
}

fn run_hunt(args: &[String]) -> Result<ExitCode, String> {
    let targets = parse_targets(&read(&args[0])?)?;
    let (mut workers, mut seconds, mut round, mut out, mut log) =
        (1usize, 60.0f64, 1u64, None, None);
    let (mut near, mut near_open, mut status) = (None, 4usize, None);
    let mut options = args[1..].iter();
    while let Some(option) = options.next() {
        let value = options
            .next()
            .ok_or_else(|| format!("{option} needs a value"))?;
        match option.as_str() {
            "--workers" => workers = number(value, "number of workers")?,
            "--seconds" => seconds = number(value, "number of seconds")?,
            "--round" => round = number(value, "round")?,
            "--out" => out = Some(PathBuf::from(value)),
            "--log" => log = Some(value.clone()),
            "--near" => near = Some(PathBuf::from(value)),
            "--near-open" => near_open = number(value, "number of open sets")?,
            "--status" => status = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown option {option}\n{USAGE}")),
        }
    }
    if workers == 0 || !seconds.is_finite() || seconds < 0.0 {
        return Err("need at least one worker and --seconds of at least 0".to_string());
    }
    let plan = Plan {
        workers,
        limit: Duration::from_secs_f64(seconds),
        round,
        out: out.ok_or("hunt needs --out DIR")?,
        near: near.map(|dir| (dir, near_open)),
        status,
    };
    let log = match &log {
        Some(path) => {
            let file = fs::OpenOptions::new().create(true).append(true).open(path);
            Some(Mutex::new(file.map_err(|error| describe(path, error))?))
        }
        None => None,
    };
    hunt(&targets, &plan, &|outcome| {
        let line = outcome.json();
        println!("{line}");
        if let Some(log) = &log {
            let mut file = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Err(error) = writeln!(file, "{line}") {
                eprintln!("log: {error}");
            }
        }
    })?;
    Ok(ExitCode::SUCCESS)
}
