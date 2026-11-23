//! Command line: check certificates, print bounds, run one search or a whole hunt.

use std::fmt::Display;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use std::{env, fs};
use zetema::bound::schoenheim;
use zetema::lab::{hunt, parse_targets, Plan};
use zetema::search::{Search, Settings};
use zetema::{check, Covering};

const USAGE: &str = "\
zetema check FILE...               check certificates; print size, bound and digest
zetema bound V K T                 the Schoenheim lower bound for (V, K, T) coverings
zetema import V K T FILE           turn a bare list of blocks into a certificate
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
    --near-open N    (default 4)";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        Some("check") if args.len() > 1 => check_files(&args[1..]),
        Some("bound") if args.len() == 4 => bound(&args[1..]),
        Some("import") if args.len() == 5 => import(&args[1..]),
        Some("search") if args.len() >= 4 => search(&args[1..]),
        Some("hunt") if args.len() >= 2 => run_hunt(&args[1..]),
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
    let (mut near, mut near_open) = (None, 4usize);
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
