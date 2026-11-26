pub mod account;
pub mod csv_io;
pub mod dispute;
pub mod engine;
pub mod error;
pub mod money;
pub mod transaction;

use std::fmt::Display;
use std::io::{Read, Write};

use csv_io::{RecordError, TransactionReader, write_accounts};
use engine::Engine;

/// Streams transactions from `input` through a fresh [`Engine`] and writes the
/// resulting accounts to `output`.
///
/// Unreadable records and rejected transactions are passed to `report_skipped`
/// and processing continues. Only a failure of the input or output stream
/// itself is returned as an error.
pub fn process(
    input: impl Read,
    output: impl Write,
    mut report_skipped: impl FnMut(&dyn Display),
) -> Result<(), csv::Error> {
    let mut engine = Engine::default();

    for record in TransactionReader::new(input)? {
        match record {
            Ok(transaction) => {
                if let Err(rejection) = engine.apply(transaction) {
                    report_skipped(&format_args!("{transaction}: {rejection}"));
                }
            }
            Err(RecordError::Csv(error)) if error.is_io_error() => return Err(error),
            Err(unreadable) => report_skipped(&unreadable),
        }
    }

    write_accounts(engine.accounts(), output)
}
