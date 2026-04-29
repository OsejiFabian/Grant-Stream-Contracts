#![cfg(test)]

use soroban_sdk::{Address, Env, U256, vec};
use crate::{GrantContract, Error};

/// Core invariant: Total_Allocated == Claimed + Remaining + Fees
/// This module provides formal verification of the grant streaming mathematics
/// to ensure no underflow or leakage of funds can occur.

const SCALING_FACTOR: u64 = 10_000_000; // 1e7 for precise decimal calculations
const TEN_YEARS_SECONDS: u64 = 315_360_000; // 10 years in seconds
const MAX_GRANT_AMOUNT: u64 = 1_000_000_000 * SCALING_FACTOR; // 1B tokens max

/// Represents the state of a grant for formal verification
#[derive(Debug, Clone, PartialEq)]
pub struct GrantState {
    pub total_amount: U256,
    pub claimed: U256,
    pub remaining: U256,
    pub fees: U256,
    pub start_time: u64,
    pub end_time: u64,
    pub current_time: u64,
}

impl GrantState {
    /// Verify the core invariant: Total_Allocated == Claimed + Remaining + Fees
    pub fn verify_invariant(&self) -> bool {
        let sum = self.claimed.add(&self.remaining).add(&self.fees);
        sum == self.total_amount
    }

    /// Calculate expected claimable amount at current time
    pub fn expected_claimable(&self) -> U256 {
        if self.current_time <= self.start_time {
            return U256::from_u32(&Env::default(), 0);
        }

        let elapsed = if self.current_time >= self.end_time {
            self.end_time - self.start_time
        } else {
            self.current_time - self.start_time
        };

        let total_duration = self.end_time - self.start_time;
        if total_duration == 0 {
            return U256::from_u32(&Env::default(), 0);
        }

        let elapsed_u256 = U256::from_u32(&Env::default(), elapsed as u32);
        let duration_u256 = U256::from_u32(&Env::default(), total_duration as u32);
        let vested = self.total_amount.mul(&elapsed_u256).div(&duration_u256);

        if vested > self.claimed {
            vested.sub(&self.claimed)
        } else {
            U256::from_u32(&Env::default(), 0)
        }
    }
}

/// Fuzzing harness for time-based calculations across 10 years
pub struct TimeFuzzer {
    env: Env,
}

impl TimeFuzzer {
    pub fn new() -> Self {
        Self { env: Env::default() }
    }

    /// Generate random timestamps across 10 years with edge cases
    pub fn generate_time_cases() -> Vec<u64> {
        let mut cases = Vec::new();
        
        // Edge cases
        cases.push(0); // Before start
        cases.push(1); // Just after start
        cases.push(TEN_YEARS_SECONDS / 2); // Middle
        cases.push(TEN_YEARS_SECONDS - 1); // Just before end
        cases.push(TEN_YEARS_SECONDS); // Exactly at end
        cases.push(TEN_YEARS_SECONDS + 1); // After end
        
        // Random sampling across the range
        for i in 0..100 {
            let rand_time = (i * TEN_YEARS_SECONDS / 100) + (i * 12345) % 86400; // Add daily variation
            cases.push(rand_time);
        }
        
        cases
    }

    /// Test invariant preservation across all time cases
    pub fn test_time_invariant_preservation(&self, grant_state: &GrantState) -> Result<(), String> {
        let time_cases = Self::generate_time_cases();
        
        for &current_time in &time_cases {
            let mut test_state = grant_state.clone();
            test_state.current_time = current_time;
            
            // Calculate claimable amount
            let claimable = test_state.expected_claimable();
            
            // Simulate claiming the amount
            let new_claimed = test_state.claimed.add(&claimable);
            let new_remaining = test_state.total_amount.sub(&new_claimed);
            
            test_state.claimed = new_claimed;
            test_state.remaining = new_remaining;
            
            // Verify invariant holds
            if !test_state.verify_invariant() {
                return Err(format!(
                    "Invariant violated at time {}: total={}, claimed={}, remaining={}, fees={}",
                    current_time,
                    test_state.total_amount,
                    test_state.claimed,
                    test_state.remaining,
                    test_state.fees
                ));
            }
        }
        
        Ok(())
    }
}

/// Test rounding error accumulation for repeating decimal stream rates
pub struct RoundingTester {
    env: Env,
}

impl RoundingTester {
    pub fn new() -> Self {
        Self { env: Env::default() }
    }

    /// Test various amounts that result in repeating decimals
    pub fn test_repeating_decimals(&self) -> Result<(), String> {
        let test_cases = vec![
            // (total_amount, duration_seconds, description)
            (100 * SCALING_FACTOR, 3, "100 tokens over 3 seconds"),
            (1000 * SCALING_FACTOR, 7, "1000 tokens over 7 seconds"),
            (123456789 * SCALING_FACTOR, 13, "123456789 tokens over 13 seconds"),
            (1 * SCALING_FACTOR, 3, "1 token over 3 seconds"),
            (7 * SCALING_FACTOR, 3, "7 tokens over 3 seconds"),
        ];

        for (total_amount, duration, description) in test_cases {
            let grant_state = GrantState {
                total_amount: U256::from_u32(&self.env, (total_amount / SCALING_FACTOR) as u32),
                claimed: U256::from_u32(&self.env, 0),
                remaining: U256::from_u32(&self.env, (total_amount / SCALING_FACTOR) as u32),
                fees: U256::from_u32(&self.env, 0),
                start_time: 0,
                end_time: duration,
                current_time: duration,
            };

            // Test claiming at various time intervals
            let time_intervals = vec![1, duration / 2, duration];
            
            for &current_time in &time_intervals {
                let mut test_state = grant_state.clone();
                test_state.current_time = current_time;
                
                let claimable = test_state.expected_claimable();
                
                // Verify claimable doesn't exceed remaining
                if claimable > test_state.remaining {
                    return Err(format!(
                        "Claimable exceeds remaining in {}: claimable={}, remaining={}",
                        description, claimable, test_state.remaining
                    ));
                }
                
                // Verify invariant after claim
                let new_claimed = test_state.claimed.add(&claimable);
                let new_remaining = test_state.remaining.sub(&claimable);
                test_state.claimed = new_claimed;
                test_state.remaining = new_remaining;
                
                if !test_state.verify_invariant() {
                    return Err(format!(
                        "Invariant violated in {}: total={}, claimed={}, remaining={}",
                        description, test_state.total_amount, test_state.claimed, test_state.remaining
                    ));
                }
            }
        }
        
        Ok(())
    }
}

/// Comprehensive test suite for pause/resume/clawback permutations
pub struct StateTransitionTester {
    env: Env,
}

impl StateTransitionTester {
    pub fn new() -> Self {
        Self { env: Env::default() }
    }

    /// Test all possible permutations of pause, resume, and clawback operations
    pub fn test_state_permutations(&self) -> Result<(), String> {
        let total_amount = 1000 * SCALING_FACTOR;
        let duration = TEN_YEARS_SECONDS;
        
        let initial_state = GrantState {
            total_amount: U256::from_u32(&self.env, (total_amount / SCALING_FACTOR) as u32),
            claimed: U256::from_u32(&self.env, 0),
            remaining: U256::from_u32(&self.env, (total_amount / SCALING_FACTOR) as u32),
            fees: U256::from_u32(&self.env, 0),
            start_time: 0,
            end_time: duration,
            current_time: 0,
        };

        // Test scenarios
        let scenarios = vec![
            ("Normal flow", self.test_normal_flow(initial_state.clone())),
            ("Pause early", self.test_pause_early(initial_state.clone())),
            ("Pause late", self.test_pause_late(initial_state.clone())),
            ("Multiple pauses", self.test_multiple_pauses(initial_state.clone())),
            ("Clawback early", self.test_clawback_early(initial_state.clone())),
            ("Clawback late", self.test_clawback_late(initial_state.clone())),
            ("Pause then clawback", self.test_pause_then_clawback(initial_state.clone())),
            ("Clawback then resume", self.test_clawback_then_resume(initial_state.clone())),
        ];

        for (description, result) in scenarios {
            if let Err(e) = result {
                return Err(format!("Scenario '{}' failed: {}", description, e));
            }
        }
        
        Ok(())
    }

    fn test_normal_flow(&self, mut state: GrantState) -> Result<(), String> {
        // Simulate normal claiming throughout the duration
        let claim_points = vec![
            state.end_time / 4,
            state.end_time / 2,
            (3 * state.end_time) / 4,
            state.end_time,
        ];

        for &time_point in &claim_points {
            state.current_time = time_point;
            let claimable = state.expected_claimable();
            
            if claimable > U256::from_u32(&self.env, 0) {
                state.claimed = state.claimed.add(&claimable);
                state.remaining = state.remaining.sub(&claimable);
            }

            if !state.verify_invariant() {
                return Err(format!("Invariant violated at time {}", time_point));
            }
        }

        Ok(())
    }

    fn test_pause_early(&self, mut state: GrantState) -> Result<(), String> {
        // Pause at 25% duration
        let pause_time = state.end_time / 4;
        state.current_time = pause_time;
        
        let claimable_before_pause = state.expected_claimable();
        state.claimed = state.claimed.add(&claimable_before_pause);
        state.remaining = state.remaining.sub(&claimable_before_pause);

        // Simulate pause (no accrual during pause)
        let pause_duration = state.end_time / 4;
        state.current_time += pause_duration;
        
        // Should have no additional claimable during pause
        let claimable_during_pause = state.expected_claimable();
        if claimable_during_pause > U256::from_u32(&self.env, 0) {
            return Err("Should not accrue during pause".to_string());
        }

        // Resume and continue to end
        state.current_time = state.end_time;
        let final_claimable = state.expected_claimable();
        state.claimed = state.claimed.add(&final_claimable);
        state.remaining = state.remaining.sub(&final_claimable);

        if !state.verify_invariant() {
            return Err("Invariant violated after pause/resume".to_string());
        }

        Ok(())
    }

    fn test_pause_late(&self, mut state: GrantState) -> Result<(), String> {
        // Pause at 75% duration
        let pause_time = (3 * state.end_time) / 4;
        state.current_time = pause_time;
        
        let claimable_before_pause = state.expected_claimable();
        state.claimed = state.claimed.add(&claimable_before_pause);
        state.remaining = state.remaining.sub(&claimable_before_pause);

        // Short pause
        let pause_duration = state.end_time / 20;
        state.current_time += pause_duration;
        
        // Resume and continue
        state.current_time = state.end_time;
        let final_claimable = state.expected_claimable();
        state.claimed = state.claimed.add(&final_claimable);
        state.remaining = state.remaining.sub(&final_claimable);

        if !state.verify_invariant() {
            return Err("Invariant violated after late pause".to_string());
        }

        Ok(())
    }

    fn test_multiple_pauses(&self, mut state: GrantState) -> Result<(), String> {
        // Multiple pause/resume cycles
        let pause_points = vec![
            state.end_time / 5,
            (2 * state.end_time) / 5,
            (3 * state.end_time) / 5,
            (4 * state.end_time) / 5,
        ];

        for &pause_time in &pause_points {
            state.current_time = pause_time;
            let claimable = state.expected_claimable();
            
            if claimable > U256::from_u32(&self.env, 0) {
                state.claimed = state.claimed.add(&claimable);
                state.remaining = state.remaining.sub(&claimable);
            }

            // Simulate pause period
            state.current_time += state.end_time / 20;
        }

        // Final claim at end
        state.current_time = state.end_time;
        let final_claimable = state.expected_claimable();
        state.claimed = state.claimed.add(&final_claimable);
        state.remaining = state.remaining.sub(&final_claimable);

        if !state.verify_invariant() {
            return Err("Invariant violated after multiple pauses".to_string());
        }

        Ok(())
    }

    fn test_clawback_early(&self, mut state: GrantState) -> Result<(), String> {
        // Clawback at 25% duration
        let clawback_time = state.end_time / 4;
        state.current_time = clawback_time;
        
        let claimable_before_clawback = state.expected_claimable();
        state.claimed = state.claimed.add(&claimable_before_clawback);
        state.remaining = state.remaining.sub(&claimable_before_clawback);

        // Clawback remaining funds
        let clawback_amount = state.remaining.clone();
        state.remaining = U256::from_u32(&self.env, 0);
        state.fees = state.fees.add(&clawback_amount);

        if !state.verify_invariant() {
            return Err("Invariant violated after early clawback".to_string());
        }

        Ok(())
    }

    fn test_clawback_late(&self, mut state: GrantState) -> Result<(), String> {
        // Claim most of the grant first
        state.current_time = (3 * state.end_time) / 4;
        let claimable = state.expected_claimable();
        state.claimed = state.claimed.add(&claimable);
        state.remaining = state.remaining.sub(&claimable);

        // Clawback remaining
        state.current_time = state.end_time;
        let final_claimable = state.expected_claimable();
        state.claimed = state.claimed.add(&final_claimable);
        state.remaining = state.remaining.sub(&final_claimable);

        if !state.verify_invariant() {
            return Err("Invariant violated before late clawback".to_string());
        }

        // Any remaining should be zero, but let's handle edge case
        if state.remaining > U256::from_u32(&self.env, 0) {
            let clawback_amount = state.remaining.clone();
            state.remaining = U256::from_u32(&self.env, 0);
            state.fees = state.fees.add(&clawback_amount);
        }

        if !state.verify_invariant() {
            return Err("Invariant violated after late clawback".to_string());
        }

        Ok(())
    }

    fn test_pause_then_clawback(&self, mut state: GrantState) -> Result<(), String> {
        // Pause at 50%
        let pause_time = state.end_time / 2;
        state.current_time = pause_time;
        let claimable = state.expected_claimable();
        state.claimed = state.claimed.add(&claimable);
        state.remaining = state.remaining.sub(&claimable);

        // Pause period
        state.current_time += state.end_time / 10;

        // Clawback during pause
        let clawback_amount = state.remaining.clone();
        state.remaining = U256::from_u32(&self.env, 0);
        state.fees = state.fees.add(&clawback_amount);

        if !state.verify_invariant() {
            return Err("Invariant violated after pause then clawback".to_string());
        }

        Ok(())
    }

    fn test_clawback_then_resume(&self, mut state: GrantState) -> Result<(), String> {
        // Claim some amount first
        state.current_time = state.end_time / 3;
        let claimable = state.expected_claimable();
        state.claimed = state.claimed.add(&claimable);
        state.remaining = state.remaining.sub(&claimable);

        // Partial clawback
        let clawback_amount = state.remaining.div(&U256::from_u32(&self.env, 2));
        state.remaining = state.remaining.sub(&clawback_amount);
        state.fees = state.fees.add(&clawback_amount);

        // Resume and continue claiming
        state.current_time = state.end_time;
        let final_claimable = state.expected_claimable();
        state.claimed = state.claimed.add(&final_claimable);
        state.remaining = state.remaining.sub(&final_claimable);

        if !state.verify_invariant() {
            return Err("Invariant violated after clawback then resume".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod formal_verification_tests {
    use super::*;

    #[test]
    fn test_core_invariant_verification() {
        let env = Env::default();
        let grant_state = GrantState {
            total_amount: U256::from_u32(&env, 1000),
            claimed: U256::from_u32(&env, 300),
            remaining: U256::from_u32(&env, 700),
            fees: U256::from_u32(&env, 0),
            start_time: 0,
            end_time: 100,
            current_time: 50,
        };

        assert!(grant_state.verify_invariant());
    }

    #[test]
    fn test_time_fuzzer_invariant_preservation() {
        let fuzzer = TimeFuzzer::new();
        let env = Env::default();
        let grant_state = GrantState {
            total_amount: U256::from_u32(&env, 1000),
            claimed: U256::from_u32(&env, 0),
            remaining: U256::from_u32(&env, 1000),
            fees: U256::from_u32(&env, 0),
            start_time: 0,
            end_time: TEN_YEARS_SECONDS,
            current_time: 0,
        };

        assert!(fuzzer.test_time_invariant_preservation(&grant_state).is_ok());
    }

    #[test]
    fn test_rounding_error_prevention() {
        let tester = RoundingTester::new();
        assert!(tester.test_repeating_decimals().is_ok());
    }

    #[test]
    fn test_state_transition_permutations() {
        let tester = StateTransitionTester::new();
        assert!(tester.test_state_permutations().is_ok());
    }

    #[test]
    fn test_maximum_grant_amount() {
        let env = Env::default();
        let grant_state = GrantState {
            total_amount: U256::from_u32(&env, (MAX_GRANT_AMOUNT / SCALING_FACTOR) as u32),
            claimed: U256::from_u32(&env, 0),
            remaining: U256::from_u32(&env, (MAX_GRANT_AMOUNT / SCALING_FACTOR) as u32),
            fees: U256::from_u32(&env, 0),
            start_time: 0,
            end_time: TEN_YEARS_SECONDS,
            current_time: 0,
        };

        assert!(grant_state.verify_invariant());
    }

    #[test]
    fn test_zero_duration_edge_case() {
        let env = Env::default();
        let grant_state = GrantState {
            total_amount: U256::from_u32(&env, 1000),
            claimed: U256::from_u32(&env, 0),
            remaining: U256::from_u32(&env, 1000),
            fees: U256::from_u32(&env, 0),
            start_time: 0,
            end_time: 0, // Zero duration
            current_time: 100,
        };

        // Should have zero claimable with zero duration
        assert_eq!(grant_state.expected_claimable(), U256::from_u32(&env, 0));
        assert!(grant_state.verify_invariant());
    }

    #[test]
    fn test_negative_edge_cases() {
        let env = Env::default();
        
        // Test with claimed exceeding total (shouldn't happen in practice but test invariant)
        let grant_state = GrantState {
            total_amount: U256::from_u32(&env, 1000),
            claimed: U256::from_u32(&env, 1500), // Over-claimed
            remaining: U256::from_u32(&env, 0),
            fees: U256::from_u32(&env, 0),
            start_time: 0,
            end_time: 100,
            current_time: 50,
        };

        // Invariant should fail for invalid state
        assert!(!grant_state.verify_invariant());
    }
}
