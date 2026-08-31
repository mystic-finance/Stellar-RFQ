use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
pub enum K {
    Bal(Address),
    Allow(Address, Address),
    Tax,
    Bonus,
}

#[contract]
pub struct SkewToken;

#[contractimpl]
impl SkewToken {
    pub fn init(env: Env, tax_bps: i128, bonus_bps: i128) {
        env.storage().instance().set(&K::Tax, &tax_bps);
        env.storage().instance().set(&K::Bonus, &bonus_bps);
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&K::Bal(id)).unwrap_or(0)
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let now = Self::balance(env.clone(), to.clone());
        env.storage().persistent().set(&K::Bal(to), &(now + amount));
    }

    pub fn approve(env: Env, from: Address, spender: Address, amount: i128, _expiry: u32) {
        from.require_auth();
        env.storage()
            .persistent()
            .set(&K::Allow(from, spender), &amount);
    }

    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&K::Allow(from, spender))
            .unwrap_or(0)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        Self::mv(&env, &from, &to, amount);
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        let left = Self::allowance(env.clone(), from.clone(), spender.clone()) - amount;
        assert!(left >= 0, "insufficient allowance");
        env.storage()
            .persistent()
            .set(&K::Allow(from.clone(), spender), &left);
        Self::mv(&env, &from, &to, amount);
    }

    fn mv(env: &Env, from: &Address, to: &Address, amount: i128) {
        let tax: i128 = env.storage().instance().get(&K::Tax).unwrap_or(0);
        let bonus: i128 = env.storage().instance().get(&K::Bonus).unwrap_or(0);
        let credited = amount - amount * tax / 10_000 + amount * bonus / 10_000;

        let from_bal = Self::balance(env.clone(), from.clone()) - amount;
        assert!(from_bal >= 0, "insufficient balance");
        env.storage()
            .persistent()
            .set(&K::Bal(from.clone()), &from_bal);
        let to_bal = Self::balance(env.clone(), to.clone()) + credited;
        env.storage().persistent().set(&K::Bal(to.clone()), &to_bal);
    }
}
