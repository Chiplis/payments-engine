use crate::error::Rejection;
use crate::money::{Amount, Balance};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Account {
    available: Balance,
    held: Balance,
    locked: bool,
}

impl Account {
    pub fn available(&self) -> Balance {
        self.available
    }

    pub fn held(&self) -> Balance {
        self.held
    }

    /// Cannot overflow: `deposit` refuses to raise the total past the largest
    /// balance, nothing else raises it, and `held` is never negative.
    pub fn total(&self) -> Balance {
        self.available + self.held
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn deposit(&mut self, amount: Amount) -> Result<(), Rejection> {
        self.ensure_unlocked()?;
        self.total().plus(amount)?;
        self.available = self.available.plus(amount)?;
        Ok(())
    }

    pub fn withdraw(&mut self, amount: Amount) -> Result<(), Rejection> {
        self.ensure_unlocked()?;
        if !self.available.covers(amount) {
            return Err(Rejection::InsufficientFunds);
        }
        self.available = self.available.minus(amount)?;
        Ok(())
    }

    pub fn hold(&mut self, amount: Amount) -> Result<(), Rejection> {
        (self.available, self.held) = (self.available.minus(amount)?, self.held.plus(amount)?);
        Ok(())
    }

    pub fn release(&mut self, amount: Amount) -> Result<(), Rejection> {
        (self.available, self.held) = (self.available.plus(amount)?, self.held.minus(amount)?);
        Ok(())
    }

    pub fn charge_back(&mut self, amount: Amount) -> Result<(), Rejection> {
        self.held = self.held.minus(amount)?;
        self.locked = true;
        Ok(())
    }

    fn ensure_unlocked(&self) -> Result<(), Rejection> {
        if self.locked {
            Err(Rejection::AccountLocked)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::BalanceOverflow;

    const LARGEST_AMOUNT: &str = "922337203685477.5807";

    fn amount(text: &str) -> Amount {
        text.parse().unwrap()
    }

    fn account_with(deposited: &str) -> Account {
        let mut account = Account::default();
        account.deposit(amount(deposited)).unwrap();
        account
    }

    fn balances(account: &Account) -> [String; 3] {
        [account.available(), account.held(), account.total()].map(|balance| balance.to_string())
    }

    #[test]
    fn a_deposit_increases_available_and_total() {
        let account = account_with("2.5");
        assert_eq!(balances(&account), ["2.5000", "0.0000", "2.5000"]);
    }

    #[test]
    fn a_withdrawal_decreases_available_and_total() {
        let mut account = account_with("2.5");
        account.withdraw(amount("1")).unwrap();
        assert_eq!(balances(&account), ["1.5000", "0.0000", "1.5000"]);
    }

    #[test]
    fn the_whole_available_balance_can_be_withdrawn() {
        let mut account = account_with("2.5");
        assert_eq!(account.withdraw(amount("2.5")), Ok(()));
    }

    #[test]
    fn a_withdrawal_beyond_available_funds_changes_nothing() {
        let mut account = account_with("2.5");
        assert_eq!(
            account.withdraw(amount("2.5001")),
            Err(Rejection::InsufficientFunds)
        );
        assert_eq!(account, account_with("2.5"));
    }

    #[test]
    fn held_funds_cannot_be_withdrawn() {
        let mut account = account_with("2.5");
        account.hold(amount("2.5")).unwrap();
        assert_eq!(
            account.withdraw(amount("1")),
            Err(Rejection::InsufficientFunds)
        );
    }

    #[test]
    fn a_hold_moves_funds_from_available_to_held() {
        let mut account = account_with("2.5");
        account.hold(amount("1")).unwrap();
        assert_eq!(balances(&account), ["1.5000", "1.0000", "2.5000"]);
    }

    #[test]
    fn holding_funds_that_were_already_withdrawn_leaves_available_negative() {
        let mut account = account_with("2.5");
        account.withdraw(amount("2")).unwrap();
        account.hold(amount("2.5")).unwrap();
        assert_eq!(balances(&account), ["-2.0000", "2.5000", "0.5000"]);
    }

    #[test]
    fn a_release_undoes_a_hold() {
        let mut account = account_with("2.5");
        account.hold(amount("1")).unwrap();
        account.release(amount("1")).unwrap();
        assert_eq!(account, account_with("2.5"));
    }

    #[test]
    fn a_chargeback_removes_held_funds_and_locks_the_account() {
        let mut account = account_with("2.5");
        account.hold(amount("1")).unwrap();
        account.charge_back(amount("1")).unwrap();
        assert_eq!(balances(&account), ["1.5000", "0.0000", "1.5000"]);
        assert!(account.is_locked());
    }

    #[test]
    fn a_locked_account_refuses_deposits_and_withdrawals() {
        let mut account = account_with("2.5");
        account.hold(amount("1")).unwrap();
        account.charge_back(amount("1")).unwrap();

        assert_eq!(account.deposit(amount("1")), Err(Rejection::AccountLocked));
        assert_eq!(account.withdraw(amount("1")), Err(Rejection::AccountLocked));
    }

    #[test]
    fn a_deposit_that_would_overflow_the_total_is_refused_even_when_funds_are_held() {
        let mut account = account_with(LARGEST_AMOUNT);
        account.hold(amount(LARGEST_AMOUNT)).unwrap();

        assert_eq!(
            account.deposit(amount("0.0001")),
            Err(Rejection::BalanceOverflow(BalanceOverflow))
        );
        assert_eq!(account.total().to_string(), LARGEST_AMOUNT);
    }

    #[test]
    fn a_hold_that_would_overflow_changes_nothing() {
        let mut account = account_with(LARGEST_AMOUNT);
        account.withdraw(amount(LARGEST_AMOUNT)).unwrap();
        account.deposit(amount(LARGEST_AMOUNT)).unwrap();
        account.withdraw(amount(LARGEST_AMOUNT)).unwrap();
        account.hold(amount(LARGEST_AMOUNT)).unwrap();
        let before = balances(&account);

        assert_eq!(
            account.hold(amount(LARGEST_AMOUNT)),
            Err(Rejection::BalanceOverflow(BalanceOverflow))
        );
        assert_eq!(balances(&account), before);
    }
}
