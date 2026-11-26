use crate::error::Rejection;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisputeState {
    #[default]
    Undisputed,
    Disputed,
    ChargedBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisputeAction {
    Open,
    Resolve,
    ChargeBack,
}

impl DisputeState {
    pub fn after(self, action: DisputeAction) -> Result<DisputeState, Rejection> {
        use DisputeAction::{ChargeBack, Open, Resolve};
        use DisputeState::{ChargedBack, Disputed, Undisputed};

        match (self, action) {
            (Undisputed, Open) => Ok(Disputed),
            (Disputed, Resolve) => Ok(Undisputed),
            (Disputed, ChargeBack) => Ok(ChargedBack),

            (Undisputed, Resolve | ChargeBack) => Err(Rejection::NotDisputed),
            (Disputed, Open) => Err(Rejection::AlreadyDisputed),
            (ChargedBack, Open | Resolve | ChargeBack) => Err(Rejection::AlreadyChargedBack),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DisputeAction::{ChargeBack, Open, Resolve};
    use super::DisputeState::{ChargedBack, Disputed, Undisputed};
    use super::*;

    #[test]
    fn a_dispute_can_be_opened_then_resolved_or_charged_back() {
        assert_eq!(Undisputed.after(Open), Ok(Disputed));
        assert_eq!(Disputed.after(Resolve), Ok(Undisputed));
        assert_eq!(Disputed.after(ChargeBack), Ok(ChargedBack));
    }

    #[test]
    fn a_resolved_deposit_can_be_disputed_again() {
        let resolved = Undisputed
            .after(Open)
            .and_then(|state| state.after(Resolve));
        assert_eq!(resolved.and_then(|state| state.after(Open)), Ok(Disputed));
    }

    #[test]
    fn only_a_disputed_deposit_can_be_resolved_or_charged_back() {
        assert_eq!(Undisputed.after(Resolve), Err(Rejection::NotDisputed));
        assert_eq!(Undisputed.after(ChargeBack), Err(Rejection::NotDisputed));
    }

    #[test]
    fn a_dispute_cannot_be_opened_twice() {
        assert_eq!(Disputed.after(Open), Err(Rejection::AlreadyDisputed));
    }

    #[test]
    fn a_chargeback_is_final() {
        for action in [Open, Resolve, ChargeBack] {
            assert_eq!(
                ChargedBack.after(action),
                Err(Rejection::AlreadyChargedBack)
            );
        }
    }
}
