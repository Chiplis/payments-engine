use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let input = File::open(input_path()?)?;
    let output = io::stdout().lock();
    let mut diagnostics = BufWriter::new(io::stderr().lock());

    payments_engine::process(input, output, |skipped| {
        // Diagnostics are best effort: a closed stderr must not stop the run.
        let _ = writeln!(diagnostics, "skipped: {skipped}");
    })?;
    Ok(())
}

fn input_path() -> Result<String, &'static str> {
    let mut arguments = env::args().skip(1);
    match (arguments.next(), arguments.next()) {
        (Some(path), None) => Ok(path),
        _ => Err("usage: payments-engine <transactions.csv>"),
    }
}
