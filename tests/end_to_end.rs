//! Runs each `tests/fixtures/<name>.csv` and compares the result with the
//! hand-computed `<name>.expected.csv` beside it.

use std::fs::{self, File};
use std::path::PathBuf;
use std::process::Command;

fn fixture(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(file_name)
}

fn expected_output(name: &str) -> String {
    fs::read_to_string(fixture(&format!("{name}.expected.csv"))).unwrap()
}

struct Processed {
    output: String,
    skipped: Vec<String>,
}

fn process(name: &str) -> Processed {
    let input = File::open(fixture(&format!("{name}.csv"))).unwrap();
    let mut output = Vec::new();
    let mut skipped = Vec::new();

    payments_engine::process(input, &mut output, |reason| {
        skipped.push(reason.to_string())
    })
    .unwrap();

    Processed {
        output: String::from_utf8(output).unwrap(),
        skipped,
    }
}

#[test]
fn the_example_from_the_specification() {
    let processed = process("spec_example");

    assert_eq!(processed.output, expected_output("spec_example"));
    assert_eq!(
        processed.skipped,
        ["withdrawal 5 for client 2: insufficient available funds"]
    );
}

#[test]
fn disputes_resolutions_and_chargebacks() {
    let processed = process("disputes");

    assert_eq!(processed.output, expected_output("disputes"));
    assert_eq!(
        processed.skipped,
        ["deposit 6 for client 2: the account is locked"]
    );
}

#[test]
fn invalid_records_and_rejected_transactions_are_skipped() {
    let processed = process("invalid_and_rejected");

    assert_eq!(processed.output, expected_output("invalid_and_rejected"));
    assert_eq!(processed.skipped.len(), 14);
}

#[test]
fn the_binary_writes_accounts_to_stdout_and_diagnostics_to_stderr() {
    let run = Command::new(env!("CARGO_BIN_EXE_payments-engine"))
        .arg(fixture("spec_example.csv"))
        .output()
        .unwrap();

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8(run.stdout).unwrap(),
        expected_output("spec_example")
    );
    assert!(
        String::from_utf8(run.stderr)
            .unwrap()
            .starts_with("skipped: withdrawal 5")
    );
}

#[test]
fn the_binary_fails_without_exactly_one_argument() {
    let binary = env!("CARGO_BIN_EXE_payments-engine");
    let no_arguments = Command::new(binary).output().unwrap();
    let two_arguments = Command::new(binary)
        .args(["a.csv", "b.csv"])
        .output()
        .unwrap();

    for run in [no_arguments, two_arguments] {
        assert!(!run.status.success());
        assert!(run.stdout.is_empty());
        assert!(String::from_utf8(run.stderr).unwrap().contains("usage"));
    }
}

#[test]
fn the_binary_fails_when_the_input_file_cannot_be_opened() {
    let run = Command::new(env!("CARGO_BIN_EXE_payments-engine"))
        .arg(fixture("does_not_exist.csv"))
        .output()
        .unwrap();

    assert!(!run.status.success());
    assert!(run.stdout.is_empty());
}
