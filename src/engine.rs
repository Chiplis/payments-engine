use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};

use crate::account::Account;
use crate::dispute::{DisputeAction, DisputeState};
use crate::error::Rejection;
use crate::money::Amount;
use crate::transaction::{ClientId, Operation, Transaction, TransactionId};

#[derive(Debug, Default)]
pub struct Engine {
    accounts: BTreeMap<ClientId, Account>,
    deposits: HashMap<TransactionId, Deposit>,
}

/// What must be remembered about a deposit so that it can later be disputed.
#[derive(Debug)]
struct Deposit {
    client: ClientId,
    amount: Amount,
    dispute: DisputeState,
}

impl Engine {
    pub fn apply(&mut self, transaction: Transaction) -> Result<(), Rejection> {
        let Transaction {
            client,
            id,
            operation,
        } = transaction;
        match operation {
            Operation::Deposit(amount) => self.deposit(client, id, amount),
            Operation::Withdrawal(amount) => self.withdraw(client, amount),
            Operation::Dispute(action) => self.advance_dispute(client, id, action),
        }
    }

    pub fn accounts(&self) -> impl Iterator<Item = (ClientId, &Account)> {
        self.accounts
            .iter()
            .map(|(client, account)| (*client, account))
    }

    fn deposit(
        &mut self,
        client: ClientId,
        id: TransactionId,
        amount: Amount,
    ) -> Result<(), Rejection> {
        let account = self.accounts.entry(client).or_default();
        let Entry::Vacant(unused_id) = self.deposits.entry(id) else {
            return Err(Rejection::DuplicateDeposit);
        };

        account.deposit(amount)?;
        unused_id.insert(Deposit {
            client,
            amount,
            dispute: DisputeState::Undisputed,
        });
        Ok(())
    }

    fn withdraw(&mut self, client: ClientId, amount: Amount) -> Result<(), Rejection> {
        self.accounts.entry(client).or_default().withdraw(amount)
    }

    fn advance_dispute(
        &mut self,
        client: ClientId,
        id: TransactionId,
        action: DisputeAction,
    ) -> Result<(), Rejection> {
        let deposit = self
            .deposits
            .get_mut(&id)
            .filter(|deposit| deposit.client == client)
            .ok_or(Rejection::DepositNotFound)?;
        let next_state = deposit.dispute.after(action)?;
        let account = self.accounts.entry(client).or_default();

        match action {
            DisputeAction::Open => account.hold(deposit.amount)?,
            DisputeAction::Resolve => account.release(deposit.amount)?,
            DisputeAction::ChargeBack => account.charge_back(deposit.amount)?,
        }
        deposit.dispute = next_state;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deposit(client: u16, id: u32, amount: &str) -> Transaction {
        transaction(client, id, Operation::Deposit(amount.parse().unwrap()))
    }

    fn withdrawal(client: u16, id: u32, amount: &str) -> Transaction {
        transaction(client, id, Operation::Withdrawal(amount.parse().unwrap()))
    }

    fn dispute(client: u16, id: u32) -> Transaction {
        transaction(client, id, Operation::Dispute(DisputeAction::Open))
    }

    fn resolve(client: u16, id: u32) -> Transaction {
        transaction(client, id, Operation::Dispute(DisputeAction::Resolve))
    }

    fn chargeback(client: u16, id: u32) -> Transaction {
        transaction(client, id, Operation::Dispute(DisputeAction::ChargeBack))
    }

    fn transaction(client: u16, id: u32, operation: Operation) -> Transaction {
        Transaction {
            client: ClientId(client),
            id: TransactionId(id),
            operation,
        }
    }

    fn engine_after(transactions: impl IntoIterator<Item = Transaction>) -> Engine {
        let mut engine = Engine::default();
        for transaction in transactions {
            engine.apply(transaction).unwrap();
        }
        engine
    }

    /// Renders every account as `client: available held total [locked]`.
    fn summary(engine: &Engine) -> Vec<String> {
        engine
            .accounts()
            .map(|(client, account)| {
                let lock = if account.is_locked() { " locked" } else { "" };
                format!(
                    "{}: {} {} {}{lock}",
                    client.0,
                    account.available(),
                    account.held(),
                    account.total()
                )
            })
            .collect()
    }

    #[test]
    fn deposits_and_withdrawals_are_tracked_per_client() {
        let engine = engine_after([
            deposit(2, 1, "5"),
            deposit(1, 2, "3"),
            withdrawal(2, 3, "1.5"),
        ]);
        assert_eq!(
            summary(&engine),
            ["1: 3.0000 0.0000 3.0000", "2: 3.5000 0.0000 3.5000"]
        );
    }

    #[test]
    fn a_rejected_withdrawal_still_creates_the_client_record() {
        let mut engine = Engine::default();
        assert_eq!(
            engine.apply(withdrawal(1, 1, "1")),
            Err(Rejection::InsufficientFunds)
        );
        assert_eq!(summary(&engine), ["1: 0.0000 0.0000 0.0000"]);
    }

    #[test]
    fn a_dispute_holds_the_deposited_amount() {
        let engine = engine_after([deposit(1, 1, "5"), deposit(1, 2, "3"), dispute(1, 1)]);
        assert_eq!(summary(&engine), ["1: 3.0000 5.0000 8.0000"]);
    }

    #[test]
    fn a_resolve_releases_the_held_amount() {
        let engine = engine_after([deposit(1, 1, "5"), dispute(1, 1), resolve(1, 1)]);
        assert_eq!(summary(&engine), ["1: 5.0000 0.0000 5.0000"]);
    }

    #[test]
    fn a_chargeback_removes_the_held_amount_and_locks_the_account() {
        let engine = engine_after([
            deposit(1, 1, "5"),
            deposit(1, 2, "3"),
            dispute(1, 1),
            chargeback(1, 1),
        ]);
        assert_eq!(summary(&engine), ["1: 3.0000 0.0000 3.0000 locked"]);
    }

    #[test]
    fn a_resolved_deposit_can_be_disputed_again_and_charged_back() {
        let engine = engine_after([
            deposit(1, 1, "5"),
            dispute(1, 1),
            resolve(1, 1),
            dispute(1, 1),
            chargeback(1, 1),
        ]);
        assert_eq!(summary(&engine), ["1: 0.0000 0.0000 0.0000 locked"]);
    }

    #[test]
    fn charging_back_a_spent_deposit_leaves_the_client_in_debt() {
        let engine = engine_after([
            deposit(1, 1, "5"),
            withdrawal(1, 2, "4"),
            dispute(1, 1),
            chargeback(1, 1),
        ]);
        assert_eq!(summary(&engine), ["1: -4.0000 0.0000 -4.0000 locked"]);
    }

    #[test]
    fn disputes_on_a_locked_account_still_proceed() {
        let engine = engine_after([
            deposit(1, 1, "5"),
            deposit(1, 2, "3"),
            dispute(1, 1),
            chargeback(1, 1),
            dispute(1, 2),
        ]);
        assert_eq!(summary(&engine), ["1: 0.0000 3.0000 3.0000 locked"]);
    }

    #[test]
    fn a_locked_account_rejects_deposits_and_withdrawals() {
        let mut engine = engine_after([
            deposit(1, 1, "5"),
            deposit(1, 2, "3"),
            dispute(1, 1),
            chargeback(1, 1),
        ]);
        assert_eq!(
            engine.apply(deposit(1, 3, "1")),
            Err(Rejection::AccountLocked)
        );
        assert_eq!(
            engine.apply(withdrawal(1, 4, "1")),
            Err(Rejection::AccountLocked)
        );
        assert_eq!(summary(&engine), ["1: 3.0000 0.0000 3.0000 locked"]);
    }

    #[test]
    fn a_deposit_rejected_by_a_locked_account_cannot_be_disputed() {
        let mut engine = engine_after([deposit(1, 1, "5"), dispute(1, 1), chargeback(1, 1)]);
        assert_eq!(
            engine.apply(deposit(1, 2, "1")),
            Err(Rejection::AccountLocked)
        );
        assert_eq!(engine.apply(dispute(1, 2)), Err(Rejection::DepositNotFound));
    }

    #[test]
    fn a_dispute_for_an_unknown_transaction_is_rejected_without_creating_a_client() {
        let mut engine = Engine::default();
        assert_eq!(
            engine.apply(dispute(1, 99)),
            Err(Rejection::DepositNotFound)
        );
        assert!(summary(&engine).is_empty());
    }

    #[test]
    fn a_withdrawal_cannot_be_disputed() {
        let mut engine = engine_after([deposit(1, 1, "5"), withdrawal(1, 2, "1")]);
        assert_eq!(engine.apply(dispute(1, 2)), Err(Rejection::DepositNotFound));
    }

    #[test]
    fn a_client_cannot_dispute_another_clients_deposit() {
        let mut engine = engine_after([deposit(1, 1, "5")]);
        assert_eq!(engine.apply(dispute(2, 1)), Err(Rejection::DepositNotFound));
        assert_eq!(summary(&engine), ["1: 5.0000 0.0000 5.0000"]);
    }

    #[test]
    fn resolving_or_charging_back_an_undisputed_deposit_changes_nothing() {
        let mut engine = engine_after([deposit(1, 1, "5")]);
        assert_eq!(engine.apply(resolve(1, 1)), Err(Rejection::NotDisputed));
        assert_eq!(engine.apply(chargeback(1, 1)), Err(Rejection::NotDisputed));
        assert_eq!(summary(&engine), ["1: 5.0000 0.0000 5.0000"]);
    }

    #[test]
    fn disputing_a_deposit_twice_holds_it_only_once() {
        let mut engine = engine_after([deposit(1, 1, "5"), dispute(1, 1)]);
        assert_eq!(engine.apply(dispute(1, 1)), Err(Rejection::AlreadyDisputed));
        assert_eq!(summary(&engine), ["1: 0.0000 5.0000 5.0000"]);
    }

    #[test]
    fn a_charged_back_deposit_cannot_be_disputed_again() {
        let mut engine = engine_after([deposit(1, 1, "5"), dispute(1, 1), chargeback(1, 1)]);
        assert_eq!(
            engine.apply(dispute(1, 1)),
            Err(Rejection::AlreadyChargedBack)
        );
    }

    #[test]
    fn a_deposit_reusing_a_transaction_id_is_rejected() {
        let mut engine = engine_after([deposit(1, 1, "5")]);
        assert_eq!(
            engine.apply(deposit(2, 1, "9")),
            Err(Rejection::DuplicateDeposit)
        );
        assert_eq!(
            summary(&engine),
            ["1: 5.0000 0.0000 5.0000", "2: 0.0000 0.0000 0.0000"]
        );
    }
}
