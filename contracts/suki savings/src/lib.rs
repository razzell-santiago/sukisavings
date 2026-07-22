#![no_std]

//! SukiSave — a per-vendor micro-savings vault built on Soroban.
//!
//! A street vendor sets a savings goal (e.g. "₱5,000 for a new cart wheel"),
//! then deposits small USDC-equivalent amounts after each sale. Funds are
//! locked in the contract until the goal is reached (full withdrawal) or the
//! vendor chooses to withdraw early and accepts a small penalty. This mirrors
//! informal "paluwagan" savings groups but removes the collector, the theft
//! risk, and the opacity — everything is verifiable on-chain.

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, token, Address, Env, String, Symbol};

/// Penalty (in basis points, 1/100 of a percent) applied to early withdrawals.
/// 500 = 5%.
const EARLY_WITHDRAW_PENALTY_BPS: i128 = 500;
const BPS_DENOMINATOR: i128 = 10_000;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Address of the token (e.g. USDC) this contract accepts.
    Token,
    /// Per-vendor savings record, keyed by vendor address.
    Vendor(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VendorData {
    /// Total amount currently saved in the vault (token base units).
    pub total_saved: i128,
    /// The vendor's savings goal (token base units).
    pub goal_amount: i128,
    /// Human-readable label for what the vendor is saving for.
    pub goal_name: String,
    /// True once total_saved >= goal_amount.
    pub is_goal_met: bool,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum SukiSaveError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    GoalMustBePositive = 3,
    DepositMustBePositive = 4,
    VendorNotFound = 5,
    GoalAlreadySet = 6,
    GoalNotYetMet = 7,
    NothingToWithdraw = 8,
    WithdrawExceedsBalance = 9,
}

#[contract]
pub struct SukiSaveContract;

#[contractimpl]
impl SukiSaveContract {
    /// One-time setup: records which token (e.g. USDC) this deployment accepts.
    /// Must be called once, immediately after deployment.
    pub fn initialize(env: Env, token: Address) -> Result<(), SukiSaveError> {
        if env.storage().instance().has(&DataKey::Token) {
            return Err(SukiSaveError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Token, &token);
        Ok(())
    }

    /// Vendor sets (or resets, before any deposit) their savings goal.
    /// Requires the vendor's signature.
    pub fn set_goal(
        env: Env,
        vendor: Address,
        goal_amount: i128,
        goal_name: String,
    ) -> Result<(), SukiSaveError> {
        vendor.require_auth();

        if goal_amount <= 0 {
            return Err(SukiSaveError::GoalMustBePositive);
        }

        let key = DataKey::Vendor(vendor.clone());
        if let Some(existing) = env.storage().persistent().get::<DataKey, VendorData>(&key) {
            // Don't let a vendor silently reset a goal they've already started saving toward.
            if existing.total_saved > 0 {
                return Err(SukiSaveError::GoalAlreadySet);
            }
        }

        let data = VendorData {
            total_saved: 0,
            goal_amount,
            goal_name,
            is_goal_met: false,
        };
        env.storage().persistent().set(&key, &data);
        Ok(())
    }

    /// Vendor deposits `amount` of the configured token into their vault.
    /// This is the core MVP action: tap "Save ₱20" after a sale.
    /// Requires the vendor's signature and moves real tokens vendor -> contract.
    pub fn deposit(env: Env, vendor: Address, amount: i128) -> Result<VendorData, SukiSaveError> {
        vendor.require_auth();

        if amount <= 0 {
            return Err(SukiSaveError::DepositMustBePositive);
        }

        let key = DataKey::Vendor(vendor.clone());
        let mut data = env
            .storage()
            .persistent()
            .get::<DataKey, VendorData>(&key)
            .ok_or(SukiSaveError::VendorNotFound)?;

        let token_address = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Token)
            .ok_or(SukiSaveError::NotInitialized)?;

        // Move the tokens from the vendor's wallet into this contract's custody.
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&vendor, &env.current_contract_address(), &amount);

        data.total_saved += amount;
        if data.total_saved >= data.goal_amount {
            data.is_goal_met = true;
        }

        env.storage().persistent().set(&key, &data);
        Ok(data)
    }

    /// Full withdrawal, only permitted once the goal has been met.
    /// Sends the entire saved balance back to the vendor and resets the vault.
    pub fn withdraw(env: Env, vendor: Address) -> Result<i128, SukiSaveError> {
        vendor.require_auth();

        let key = DataKey::Vendor(vendor.clone());
        let mut data = env
            .storage()
            .persistent()
            .get::<DataKey, VendorData>(&key)
            .ok_or(SukiSaveError::VendorNotFound)?;

        if !data.is_goal_met {
            return Err(SukiSaveError::GoalNotYetMet);
        }
        if data.total_saved <= 0 {
            return Err(SukiSaveError::NothingToWithdraw);
        }

        let payout = data.total_saved;
        let token_address = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Token)
            .ok_or(SukiSaveError::NotInitialized)?;

        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &vendor, &payout);

        data.total_saved = 0;
        data.is_goal_met = false;
        env.storage().persistent().set(&key, &data);

        Ok(payout)
    }

    /// Early withdrawal before the goal is met. A small penalty (5%) is
    /// deducted and kept in the contract, discouraging impulse withdrawals
    /// while still giving vendors access to their own money in emergencies.
    pub fn withdraw_early(env: Env, vendor: Address, amount: i128) -> Result<i128, SukiSaveError> {
        vendor.require_auth();

        if amount <= 0 {
            return Err(SukiSaveError::DepositMustBePositive);
        }

        let key = DataKey::Vendor(vendor.clone());
        let mut data = env
            .storage()
            .persistent()
            .get::<DataKey, VendorData>(&key)
            .ok_or(SukiSaveError::VendorNotFound)?;

        if amount > data.total_saved {
            return Err(SukiSaveError::WithdrawExceedsBalance);
        }

        let penalty = (amount * EARLY_WITHDRAW_PENALTY_BPS) / BPS_DENOMINATOR;
        let payout = amount - penalty;

        let token_address = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Token)
            .ok_or(SukiSaveError::NotInitialized)?;

        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &vendor, &payout);

        data.total_saved -= amount;
        data.is_goal_met = data.total_saved >= data.goal_amount;
        env.storage().persistent().set(&key, &data);

        Ok(payout)
    }

    /// Read-only: current saved balance for a vendor.
    pub fn get_balance(env: Env, vendor: Address) -> Result<i128, SukiSaveError> {
        let key = DataKey::Vendor(vendor);
        let data = env
            .storage()
            .persistent()
            .get::<DataKey, VendorData>(&key)
            .ok_or(SukiSaveError::VendorNotFound)?;
        Ok(data.total_saved)
    }

    /// Read-only: full vendor savings record (balance, goal, progress).
    pub fn get_status(env: Env, vendor: Address) -> Result<VendorData, SukiSaveError> {
        let key = DataKey::Vendor(vendor);
        env.storage()
            .persistent()
            .get::<DataKey, VendorData>(&key)
            .ok_or(SukiSaveError::VendorNotFound)
    }
}

// Helper symbol kept for potential future event topics (e.g. "deposit", "goalmet").
#[allow(dead_code)]
const EVENT_TOPIC: Symbol = Symbol::short("sukisave");

mod test;