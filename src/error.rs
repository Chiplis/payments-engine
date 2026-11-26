use thiserror::Error;

use crate::money::BalanceOverflow;

/// Why a well-formed transaction was refused. A rejected transaction leaves
/// every account and every stored deposit exactly as it was.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Rejection {
    #[error("the account is locked")]
    AccountLocked,
    #[error("insufficient available funds")]
    InsufficientFunds,
    #[error(transparent)]
    BalanceOverflow(#[from] BalanceOverflow),
    #[error("a deposit with this transaction id already exists")]
    DuplicateDeposit,
    #[error("the client has no deposit with this transaction id")]
    DepositNotFound,
    #[error("the deposit is already under dispute")]
    AlreadyDisputed,
    #[error("the deposit is not under dispute")]
    NotDisputed,
    #[error("the deposit was already charged back")]
    AlreadyChargedBack,
}
