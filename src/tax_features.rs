// Tax withholding and clawback features for GrantContract
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, Symbol, U256, symbol_short
};

#[contracttype]
pub struct TaxVault {
    pub total_withheld: U256,
    pub tax_rate: u32,
}

#[contracttype]
pub struct ClawbackEvent {
    pub clawback_amount: U256,
    pub previous_balance: U256,
    pub new_balance: U256,
    pub timestamp: u64,
}

// Add tax and clawback functions to existing GrantContract
pub trait TaxWithholding {
    fn initialize_grant_with_tax(env: Env, recipient: Address, total_amount: U256, duration_seconds: u64, tax_rate: u32) -> u64;
    fn claim_with_tax(env: Env, recipient: Address) -> (U256, U256);
    fn withdraw_tax_vault(env: Env, grantor: Address, amount: U256) -> U256;
    fn balance_sync(env: Env, current_balance: U256);
}
