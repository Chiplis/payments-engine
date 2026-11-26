use std::fmt;

use serde::{Deserialize, Serialize};

use crate::dispute::DisputeAction;
use crate::money::Amount;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct ClientId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct TransactionId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transaction {
    pub client: ClientId,
    pub id: TransactionId,
    pub operation: Operation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Deposit(Amount),
    Withdrawal(Amount),
    /// Acts on the earlier deposit whose id this transaction carries.
    Dispute(DisputeAction),
}

impl fmt::Display for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let operation = match self.operation {
            Operation::Deposit(_) => "deposit",
            Operation::Withdrawal(_) => "withdrawal",
            Operation::Dispute(DisputeAction::Open) => "dispute",
            Operation::Dispute(DisputeAction::Resolve) => "resolve",
            Operation::Dispute(DisputeAction::ChargeBack) => "chargeback",
        };
        write!(f, "{operation} {} for client {}", self.id.0, self.client.0)
    }
}
