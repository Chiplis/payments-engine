use std::io::{Read, Write};

use csv::{ReaderBuilder, StringRecord, Trim, WriterBuilder};
use serde::Deserialize;
use thiserror::Error;

use crate::account::Account;
use crate::dispute::DisputeAction;
use crate::money::{Amount, AmountError};
use crate::transaction::{ClientId, Operation, Transaction, TransactionId};

const ACCOUNT_COLUMNS: [&str; 5] = ["client", "available", "held", "total", "locked"];

#[derive(Debug, Error)]
pub enum RecordError {
    #[error(transparent)]
    Csv(#[from] csv::Error),
    #[error("line {line}: {reason}")]
    Invalid { line: u64, reason: InvalidRecord },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InvalidRecord {
    #[error("deposits and withdrawals need an amount")]
    MissingAmount,
    #[error(transparent)]
    InvalidAmount(#[from] AmountError),
}

/// Yields one transaction per CSV record, holding a single record in memory at a time.
pub struct TransactionReader<R> {
    reader: csv::Reader<R>,
    headers: StringRecord,
    record: StringRecord,
}

impl<R: Read> TransactionReader<R> {
    pub fn new(input: R) -> Result<Self, csv::Error> {
        let mut reader = ReaderBuilder::new()
            .trim(Trim::All)
            .flexible(true)
            .from_reader(input);
        let headers = reader.headers()?.clone();

        Ok(TransactionReader {
            reader,
            headers,
            record: StringRecord::new(),
        })
    }

    fn parse_current_record(&self) -> Result<Transaction, RecordError> {
        let row: Row = self.record.deserialize(Some(&self.headers))?;
        Transaction::try_from(row).map_err(|reason| RecordError::Invalid {
            line: self.current_line(),
            reason,
        })
    }

    fn current_line(&self) -> u64 {
        self.record.position().map_or(0, |position| position.line())
    }
}

impl<R: Read> Iterator for TransactionReader<R> {
    type Item = Result<Transaction, RecordError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.read_record(&mut self.record) {
            Ok(true) => Some(self.parse_current_record()),
            Ok(false) => None,
            Err(error) => Some(Err(error.into())),
        }
    }
}

#[derive(Deserialize)]
struct Row<'a> {
    #[serde(rename = "type")]
    kind: RowKind,
    client: ClientId,
    tx: TransactionId,
    #[serde(borrow)]
    amount: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum RowKind {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

impl Row<'_> {
    fn required_amount(&self) -> Result<Amount, InvalidRecord> {
        let text = self.amount.ok_or(InvalidRecord::MissingAmount)?;
        Ok(text.parse()?)
    }
}

impl TryFrom<Row<'_>> for Transaction {
    type Error = InvalidRecord;

    fn try_from(row: Row<'_>) -> Result<Transaction, InvalidRecord> {
        let operation = match row.kind {
            RowKind::Deposit => Operation::Deposit(row.required_amount()?),
            RowKind::Withdrawal => Operation::Withdrawal(row.required_amount()?),
            RowKind::Dispute => Operation::Dispute(DisputeAction::Open),
            RowKind::Resolve => Operation::Dispute(DisputeAction::Resolve),
            RowKind::Chargeback => Operation::Dispute(DisputeAction::ChargeBack),
        };

        Ok(Transaction {
            client: row.client,
            id: row.tx,
            operation,
        })
    }
}

pub fn write_accounts<'a>(
    accounts: impl IntoIterator<Item = (ClientId, &'a Account)>,
    output: impl Write,
) -> Result<(), csv::Error> {
    let mut writer = WriterBuilder::new().has_headers(false).from_writer(output);

    writer.write_record(ACCOUNT_COLUMNS)?;
    for (client, account) in accounts {
        writer.serialize((
            client,
            account.available(),
            account.held(),
            account.total(),
            account.is_locked(),
        ))?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(csv: &str) -> Vec<Result<Transaction, String>> {
        TransactionReader::new(csv.as_bytes())
            .unwrap()
            .map(|record| record.map_err(|error| error.to_string()))
            .collect()
    }

    fn read_valid(csv: &str) -> Vec<Transaction> {
        read(csv).into_iter().map(Result::unwrap).collect()
    }

    fn transaction(client: u16, id: u32, operation: Operation) -> Transaction {
        Transaction {
            client: ClientId(client),
            id: TransactionId(id),
            operation,
        }
    }

    #[test]
    fn reads_every_transaction_type() {
        let transactions = read_valid(
            "type,client,tx,amount\n\
             deposit,1,10,1.5\n\
             withdrawal,1,11,0.25\n\
             dispute,1,10,\n\
             resolve,1,10,\n\
             chargeback,1,10,\n",
        );
        assert_eq!(
            transactions,
            [
                transaction(1, 10, Operation::Deposit("1.5".parse().unwrap())),
                transaction(1, 11, Operation::Withdrawal("0.25".parse().unwrap())),
                transaction(1, 10, Operation::Dispute(DisputeAction::Open)),
                transaction(1, 10, Operation::Dispute(DisputeAction::Resolve)),
                transaction(1, 10, Operation::Dispute(DisputeAction::ChargeBack)),
            ]
        );
    }

    #[test]
    fn tolerates_whitespace_around_headers_and_fields() {
        let transactions = read_valid("type, client, tx, amount\n deposit ,  7,\t3 , 2.0 \n");
        assert_eq!(
            transactions,
            [transaction(7, 3, Operation::Deposit("2".parse().unwrap()))]
        );
    }

    #[test]
    fn dispute_rows_may_omit_the_amount_column_entirely() {
        let transactions = read_valid("type,client,tx,amount\ndispute,1,10\n");
        assert_eq!(
            transactions,
            [transaction(1, 10, Operation::Dispute(DisputeAction::Open))]
        );
    }

    #[test]
    fn columns_are_matched_by_header_name_rather_than_position() {
        let transactions = read_valid("amount,tx,type,client\n2.5,3,deposit,7\n");
        assert_eq!(
            transactions,
            [transaction(
                7,
                3,
                Operation::Deposit("2.5".parse().unwrap())
            )]
        );
    }

    #[test]
    fn a_bad_record_is_reported_with_its_line_and_does_not_stop_the_stream() {
        let records = read(
            "type,client,tx,amount\n\
             deposit,1,1,\n\
             deposit,1,2,-5\n\
             refund,1,3,1.0\n\
             deposit,70000,4,1.0\n\
             deposit,1,5,1.0\n",
        );

        assert_eq!(
            records[0],
            Err("line 2: deposits and withdrawals need an amount".to_owned())
        );
        assert_eq!(
            records[1],
            Err("line 3: amount is not an unsigned decimal number".to_owned())
        );
        assert!(
            records[2]
                .as_ref()
                .is_err_and(|error| error.contains("line: 4"))
        );
        assert!(
            records[3]
                .as_ref()
                .is_err_and(|error| error.contains("line: 5"))
        );
        assert_eq!(
            records[4],
            Ok(transaction(1, 5, Operation::Deposit("1".parse().unwrap())))
        );
    }

    #[test]
    fn writes_a_header_even_when_there_are_no_accounts() {
        let mut output = Vec::new();
        write_accounts([], &mut output).unwrap();
        assert_eq!(output, b"client,available,held,total,locked\n");
    }

    #[test]
    fn writes_one_row_per_account_with_four_decimal_places() {
        let mut account = Account::default();
        account.deposit("1.5".parse().unwrap()).unwrap();
        account.hold("0.25".parse().unwrap()).unwrap();

        let mut output = Vec::new();
        write_accounts([(ClientId(3), &account)], &mut output).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "client,available,held,total,locked\n3,1.2500,0.2500,1.5000,false\n"
        );
    }
}
