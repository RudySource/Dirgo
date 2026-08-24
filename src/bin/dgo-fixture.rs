use std::{env, path::PathBuf, process::ExitCode};

use dirgo::fixture::{MAX_DIRECTORIES, create_batch};

const USAGE: &str = "Usage: dgo-fixture --output PATH --directories N [--fanout N] [--batch-size N] [--resume]\n\nCreates a deterministic directory fixture for Dirgo benchmarks.\nWithout --resume the output path must not exist. Resuming requires Dirgo's matching progress marker.\nN is between 1 and 1,000,000.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("dgo-fixture: {}", dirgo::terminal::safe_text(&message));
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let mut output = None;
    let mut directories = None;
    let mut fanout = 32_u16;
    let mut batch_size = MAX_DIRECTORIES;
    let mut resume = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "--output" => output = args.next().map(PathBuf::from),
            "--directories" => directories = args.next(),
            "--fanout" => {
                fanout = args
                    .next()
                    .ok_or_else(|| "--fanout requires a value".to_owned())?
                    .to_string_lossy()
                    .parse()
                    .map_err(|_| "--fanout must be an integer".to_owned())?;
            }
            "--batch-size" => {
                batch_size = args
                    .next()
                    .ok_or_else(|| "--batch-size requires a value".to_owned())?
                    .to_string_lossy()
                    .parse()
                    .map_err(|_| "--batch-size must be an integer".to_owned())?;
            }
            "--resume" => resume = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            value => return Err(format!("unknown argument {value:?}\n\n{USAGE}")),
        }
    }
    let output = output.ok_or_else(|| format!("--output is required\n\n{USAGE}"))?;
    let directories = directories
        .ok_or_else(|| format!("--directories is required\n\n{USAGE}"))?
        .to_string_lossy()
        .parse::<u64>()
        .map_err(|_| "--directories must be an integer".to_owned())?;
    if directories > MAX_DIRECTORIES {
        return Err(format!("--directories cannot exceed {MAX_DIRECTORIES}"));
    }
    let progress = create_batch(&output, directories, fanout, batch_size, resume)
        .map_err(|error| error.to_string())?;
    if progress.completed == progress.target {
        println!(
            "created {} directories at {} (fanout {})",
            directories,
            dirgo::terminal::safe_path(&output),
            fanout
        );
    } else {
        println!(
            "fixture progress: {}/{} directories at {} (resume with --resume)",
            progress.completed,
            progress.target,
            dirgo::terminal::safe_path(&output)
        );
    }
    Ok(())
}
