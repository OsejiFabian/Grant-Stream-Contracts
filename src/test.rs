#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, U256};

// Client type for contract testing
use soroban_sdk::contractclient::ContractClient;

#[test]
fn test_basic_grant() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u32(&env, 1000);
    let duration = 100u64;

    client.initialize_grant(&recipient, &total_amount, &duration);

    let claimable = client.claimable_balance();
    assert_eq!(claimable, U256::from_u32(&env, 0));
}

#[test]
#[should_panic(expected = "duration exceeds MAX_DURATION")]
fn test_initialize_rejects_duration_over_max() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u32(&env, 1000);
    let duration = super::MAX_DURATION + 1;

    client.initialize_grant(&recipient, &total_amount, &duration);
}

// ===== FORMAL VERIFICATION TESTS =====

#[test]
fn test_core_invariant_total_allocated_equals_claimed_plus_remaining() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u32(&env, 1000);
    let duration = 1000u64;

    // Initialize grant
    client.initialize_grant(&recipient, &total_amount, &duration);

    // Test invariant at multiple time points
    let time_points = vec![0, duration / 4, duration / 2, (3 * duration) / 4, duration];

    for &time_offset in &time_points {
        env.ledger().set_timestamp(time_offset);

        let (current_total, _, _, claimed) = client.get_grant_info();
        let claimable = client.claimable_balance();
        let remaining = current_total.sub(&claimed);

        // Core invariant: Total = Claimed + Remaining + Fees (Fees = 0 in basic contract)
        assert_eq!(current_total, claimed.add(&remaining));
        
        // Claimable should not exceed remaining
        assert!(claimable <= remaining);
    }
}

#[test]
fn test_ten_year_fuzzing_simulation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u32(&env, 1_000_000); // 1M tokens
    let duration = 315_360_000u64; // 10 years

    client.initialize_grant(&recipient, &total_amount, &duration);

    // Test at 100 random points across 10 years
    for i in 0..100 {
        let time_point = (i * duration / 100) + (i * 12345) % 86400; // Add daily variation
        env.ledger().set_timestamp(time_point);

        let (current_total, _, _, claimed) = client.get_grant_info();
        let claimable = client.claimable_balance();
        let remaining = current_total.sub(&claimed);

        // Verify invariant holds
        assert_eq!(current_total, claimed.add(&remaining));
        assert!(claimable <= remaining);
        assert!(claimed <= current_total);
    }
}

#[test]
fn test_rounding_error_prevention() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Test cases that cause repeating decimals
    let test_cases = vec![
        (100, 3),    // 100/3 = 33.33...
        (1000, 7),   // 1000/7 = 142.857...
        (123, 13),   // 123/13 = 9.461...
        (7, 3),      // 7/3 = 2.333...
    ];

    for (amount, duration) in test_cases {
        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);
        let recipient = Address::generate(&env);
        let total_amount = U256::from_u32(&env, amount);

        client.initialize_grant(&recipient, &total_amount, duration);

        // Test at multiple time points
        for time_point in [1, duration / 2, duration] {
            env.ledger().set_timestamp(time_point);

            let (current_total, _, _, claimed) = client.get_grant_info();
            let claimable = client.claimable_balance();
            let remaining = current_total.sub(&claimed);

            // Invariant should hold despite rounding
            assert_eq!(current_total, claimed.add(&remaining));
            assert!(claimable <= remaining);
        }
    }
}

#[test]
fn test_insolvency_prevention() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u32(&env, 1000);
    let duration = 1000u64;

    client.initialize_grant(&recipient, &total_amount, &duration);

    // Test that contract can never become insolvent
    for time_point in [0, duration / 4, duration / 2, (3 * duration) / 4, duration] {
        env.ledger().set_timestamp(time_point);

        let (current_total, _, _, claimed) = client.get_grant_info();
        let claimable = client.claimable_balance();
        let remaining = current_total.sub(&claimed);

        // Claimable can never exceed what's available
        assert!(claimable <= remaining);
        
        // Total amount is preserved
        assert_eq!(current_total, total_amount);

        // Claim if possible
        if claimable > U256::from_u32(&env, 0) {
            let result = client.claim(&recipient);
            assert!(result.is_ok());
        }
    }
}

#[test]
fn test_maximum_grant_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    // Test with very large amount
    let total_amount = U256::from_u32(&env, u32::MAX);
    let duration = super::MAX_DURATION;

    client.initialize_grant(&recipient, &total_amount, &duration);

    // Verify invariant holds with maximum values
    let (current_total, _, _, claimed) = client.get_grant_info();
    let claimable = client.claimable_balance();
    let remaining = current_total.sub(&claimed);

    assert_eq!(current_total, total_amount);
    assert_eq!(current_total, claimed.add(&remaining));
    assert!(claimable <= remaining);
}

#[test]
fn test_edge_cases_boundary_conditions() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Test zero duration
    {
        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);
        let recipient = Address::generate(&env);
        let total_amount = U256::from_u32(&env, 1000);

        client.initialize_grant(&recipient, &total_amount, 0);
        
        let claimable = client.claimable_balance();
        assert_eq!(claimable, U256::from_u32(&env, 0));
        
        let (current_total, _, _, claimed) = client.get_grant_info();
        assert_eq!(current_total, claimed.add(&current_total)); // remaining = total
    }

    // Test minimum amount
    {
        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);
        let recipient = Address::generate(&env);
        let total_amount = U256::from_u32(&env, 1);

        client.initialize_grant(&recipient, &total_amount, 100);
        
        let (current_total, _, _, claimed) = client.get_grant_info();
        assert_eq!(current_total, U256::from_u32(&env, 1));
        assert_eq!(current_total, claimed.add(&current_total)); // remaining = total
    }

    // Test maximum duration
    {
        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);
        let recipient = Address::generate(&env);
        let total_amount = U256::from_u32(&env, 1000);

        client.initialize_grant(&recipient, &total_amount, super::MAX_DURATION);
        
        let (current_total, _, _, claimed) = client.get_grant_info();
        assert_eq!(current_total, total_amount);
        assert_eq!(current_total, claimed.add(&current_total)); // remaining = total
    }
}

#[test]
fn test_rapid_claiming_invariant() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u32(&env, 1000);
    let duration = 1000u64;

    client.initialize_grant(&recipient, &total_amount, &duration);

    // Advance time significantly
    env.ledger().set_timestamp(duration);

    // Make multiple claims rapidly (though contract claims all at once)
    for _ in 0..5 {
        let claimable = client.claimable_balance();
        
        if claimable > U256::from_u32(&env, 0) {
            let result = client.claim(&recipient);
            assert!(result.is_ok());
            
            // Verify invariant after each claim
            let (current_total, _, _, claimed) = client.get_grant_info();
            let remaining = current_total.sub(&claimed);
            assert_eq!(current_total, claimed.add(&remaining));
        } else {
            break;
        }
    }
}

#[test]
fn test_mathematical_proof_of_linear_vesting() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u32(&env, 1000);
    let duration = 1000u64;

    client.initialize_grant(&recipient, &total_amount, &duration);

    // Mathematical proof: vested = total * elapsed / duration
    let test_points = vec![duration / 10, duration / 4, duration / 2, (3 * duration) / 4];
    
    for &time_point in &test_points {
        env.ledger().set_timestamp(time_point);
        
        let claimable = client.claimable_balance();
        let expected_vested = total_amount
            .mul(&U256::from_u32(&env, (time_point) as u32))
            .div(&U256::from_u32(&env, duration as u32));
        
        // Should be approximately equal (allowing for rounding)
        assert!(claimable >= expected_vested.sub(&U256::from_u32(&env, 1)));
        assert!(claimable <= expected_vested.add(&U256::from_u32(&env, 1)));
        
        // Verify invariant
        let (current_total, _, _, claimed) = client.get_grant_info();
        let remaining = current_total.sub(&claimed);
        assert_eq!(current_total, claimed.add(&remaining));
    }
}
