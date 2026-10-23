use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::vec;

fn e(err: Error) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(err as u32)
}

struct F {
    env: Env,
    v: CallVerifierContractClient<'static>,
    admin: Address,
    guardian: Address,
    vault: Address,
    target: Address,
    account: Address,
}

fn setup() -> F {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let [admin, guardian, vault, target, account] = [0; 5].map(|_| Address::generate(&env));
    let v = CallVerifierContractClient::new(
        &env,
        &env.register(
            CallVerifierContract,
            (admin.clone(), guardian.clone(), 100u64),
        ),
    );
    F {
        env,
        v,
        admin,
        guardian,
        vault,
        target,
        account,
    }
}

impl F {
    /// `redeem(account, amount, to = target)`: amount free, `to` pinned.
    fn add(&self) -> u64 {
        let e = &self.env;
        self.v.add_rule(&Rule {
            caller: self.vault.clone(),
            target: self.target.clone(),
            func: Symbol::new(e, "redeem"),
            template: vec![
                e,
                7i128.into_val(e),
                99i128.into_val(e),
                self.target.into_val(e),
            ],
            free: 0b010,
            account: 0b001,
        })
    }

    fn ok(&self, account: &Address, amount: i128, to: &Address) -> bool {
        let e = &self.env;
        let args = vec![e, account.into_val(e), amount.into_val(e), to.into_val(e)];
        self.v.verify(
            &self.vault,
            &self.account,
            &self.target,
            &Symbol::new(e, "redeem"),
            &args,
        )
    }

    fn wait(&self, s: u64) {
        let t = self.env.ledger().timestamp();
        self.env.ledger().set_timestamp(t + s);
    }
}

#[test]
fn a_rule_pins_arguments_and_the_account_once_live() {
    let f = setup();
    f.add();
    assert!(!f.ok(&f.account, 5, &f.target), "not live before the delay");
    f.wait(100);
    assert!(f.ok(&f.account, 5, &f.target));
    assert!(f.ok(&f.account, 1_000_000, &f.target), "the amount is free");
    let other = Address::generate(&f.env);
    assert!(
        !f.ok(&other, 5, &f.target),
        "the first argument must be the account"
    );
    assert!(!f.ok(&f.account, 5, &other), "the destination is pinned");
    let e = &f.env;
    let short = vec![e, f.account.into_val(e), 5i128.into_val(e)];
    assert!(!f.v.verify(
        &f.vault,
        &f.account,
        &f.target,
        &Symbol::new(e, "redeem"),
        &short
    ));
    assert!(!f.v.verify(
        &f.target,
        &f.account,
        &f.target,
        &Symbol::new(e, "redeem"),
        &short
    ));
}

#[test]
fn revoking_is_immediate_and_regranting_waits_again() {
    let f = setup();
    let id = f.add();
    f.wait(100);
    f.v.set_enabled(&f.guardian, &id, &false);
    assert!(!f.ok(&f.account, 5, &f.target));
    assert_eq!(
        f.v.try_set_enabled(&f.guardian, &id, &true),
        Err(Ok(e(Error::NotAuthorized))),
        "the guardian only revokes"
    );
    f.v.set_enabled(&f.admin, &id, &true);
    assert!(
        !f.ok(&f.account, 5, &f.target),
        "re-enabled: the delay again"
    );
    f.wait(100);
    assert!(f.ok(&f.account, 5, &f.target));
    f.v.remove(&f.guardian, &id);
    assert!(!f.ok(&f.account, 5, &f.target));
    assert_eq!(f.v.entry(&id), None);
}

#[test]
fn malformed_rules_are_refused() {
    let f = setup();
    let env = &f.env;
    let rule = |free, account| Rule {
        caller: f.vault.clone(),
        target: f.target.clone(),
        func: Symbol::new(env, "redeem"),
        template: vec![env, 1i128.into_val(env)],
        free,
        account,
    };
    assert_eq!(
        f.v.try_add_rule(&rule(1, 1)),
        Err(Ok(e(Error::InvalidRule)))
    );
    assert_eq!(
        f.v.try_add_rule(&rule(0b10, 0)),
        Err(Ok(e(Error::InvalidRule)))
    );
}
