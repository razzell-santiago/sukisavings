#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token::StellarAssetClient,
    Env, String,
};

/// Spins up a mock token contract (acting as USDC) and returns
/// (token contract address, admin client, token client) for use in tests.
fn setup_token<'a>(env: &Env) -> (Address, StellarAssetClient<'a>, token::Client<'a>) {
    let admin = Address::generate(env);
    let token_contract_id = env.register_stellar_asset_contract_v2(admin.clone());
    let asset_client = StellarAssetClient::new(env, &token_contract_id.address());
    let token_client = token::Client::new(env, &token_contract_id.address());
    (token_contract_id.address(), asset_client, token_client)
}

fn setup_contract(env: &Env) -> SukiSaveContractClient {
    let contract_id = env.register(SukiSaveContract, ());
    SukiSaveContractClient::new(env, &contract_id)
}

/// Test 1 (Happy path): vendor sets a goal, deposits toward it across
/// multiple sales, hits the goal, and successfully withdraws the full amount.
#[test]
fn test_happy_path_save_and_withdraw() {
    let env = Env::default();
    env.mock_all_auths();

    let (token_address, asset_admin, token_client) = setup_token(&env);
    let contract = setup_contract(&env);
    contract.initialize(&token_address);

    let vendor = Address::generate(&env);
    asset_admin.mint(&vendor, &100_00); // mint 100.00 units of test USDC

    contract.set_goal(&vendor, &50_00, &String::from_str(&env, "New cart wheel"));

    // Three deposits simulating three days of sales, reaching the goal.
    contract.deposit(&vendor, &20_00);
    contract.deposit(&vendor, &20_00);
    let status = contract.deposit(&vendor, &10_00);

    assert!(status.is_goal_met);
    assert_eq!(status.total_saved, 50_00);

    let payout = contract.withdraw(&vendor);
    assert_eq!(payout, 50_00);
    assert_eq!(token_client.balance(&vendor), 100_00); // fully returned
}

/// Test 2 (Edge case): a caller cannot deposit into a vendor's vault without
/// that vendor's authorization (unauthorized-caller failure scenario).
#[test]
#[should_panic]
fn test_deposit_without_auth_panics() {
    let env = Env::default();
    // Note: auths are NOT mocked here, so require_auth() inside deposit()
    // will panic because no authorization was provided for `vendor`.

    let (token_address, asset_admin, _token_client) = setup_token(&env);
    let contract = setup_contract(&env);

    env.mock_all_auths();
    contract.initialize(&token_address);
    let vendor = Address::generate(&env);
    asset_admin.mint(&vendor, &50_00);
    contract.set_goal(&vendor, &50_00, &String::from_str(&env, "Umbrella cart"));

    env.set_auths(&[]); // explicitly strip auths before the unauthorized call
    contract.deposit(&vendor, &10_00);
}

/// Test 3 (State verification): after a sequence of deposits, contract
/// storage correctly reflects cumulative total_saved and goal progress.
#[test]
fn test_state_reflects_cumulative_deposits() {
    let env = Env::default();
    env.mock_all_auths();

    let (token_address, asset_admin, _token_client) = setup_token(&env);
    let contract = setup_contract(&env);
    contract.initialize(&token_address);

    let vendor = Address::generate(&env);
    asset_admin.mint(&vendor, &1000_00);
    contract.set_goal(&vendor, &100_00, &String::from_str(&env, "Fryer repair"));

    contract.deposit(&vendor, &15_00);
    contract.deposit(&vendor, &25_00);

    let status = contract.get_status(&vendor);
    assert_eq!(status.total_saved, 40_00);
    assert_eq!(status.goal_amount, 100_00);
    assert!(!status.is_goal_met);

    let balance = contract.get_balance(&vendor);
    assert_eq!(balance, 40_00);
}

/// Test 4 (Edge case): withdrawing before the goal is met must fail —
/// the vault should not release funds ahead of schedule.
#[test]
fn test_withdraw_before_goal_met_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (token_address, asset_admin, _token_client) = setup_token(&env);
    let contract = setup_contract(&env);
    contract.initialize(&token_address);

    let vendor = Address::generate(&env);
    asset_admin.mint(&vendor, &200_00);
    contract.set_goal(&vendor, &100_00, &String::from_str(&env, "New tarp"));
    contract.deposit(&vendor, &30_00);

    let result = contract.try_withdraw(&vendor);
    assert_eq!(result, Err(Ok(SukiSaveError::GoalNotYetMet)));
}

/// Test 5 (Early withdrawal path): vendor can withdraw early with a 5%
/// penalty, and the remaining vault balance is updated accordingly.
#[test]
fn test_early_withdraw_applies_penalty() {
    let env = Env::default();
    env.mock_all_auths();

    let (token_address, asset_admin, token_client) = setup_token(&env);
    let contract = setup_contract(&env);
    contract.initialize(&token_address);

    let vendor = Address::generate(&env);
    asset_admin.mint(&vendor, &200_00);
    contract.set_goal(&vendor, &100_00, &String::from_str(&env, "Cart repaint"));
    contract.deposit(&vendor, &60_00);

    // Vendor has an emergency and pulls 20_00 early; 5% penalty = 1_00.
    let payout = contract.withdraw_early(&vendor, &20_00);
    assert_eq!(payout, 19_00);

    let status = contract.get_status(&vendor);
    assert_eq!(status.total_saved, 40_00);

    // Wallet math: minted 200_00 -> deposited 60_00 (140_00 left) ->
    // received 19_00 back from early withdrawal -> 159_00.
    assert_eq!(token_client.balance(&vendor), 159_00);
}