use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};

use num_bigint::BigUint;
use rocketbot_interface::add_thousands_separators;


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum FactorResult {
    Factored(PrimeFactors),
    Stuck(BigUint),
    Halted,
}
impl FactorResult {
    #[allow(unused)]
    pub fn as_factored(&self) -> Option<&PrimeFactors> {
        match self {
            Self::Factored(pf) => Some(pf),
            _ => None,
        }
    }

    #[allow(unused)]
    pub fn as_stuck(&self) -> Option<&BigUint> {
        match self {
            Self::Stuck(s) => Some(s),
            _ => None,
        }
    }

    #[allow(unused)]
    pub fn is_halted(&self) -> bool {
        matches!(self, Self::Halted)
    }
}


#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct PrimeFactors {
    factor_to_power: BTreeMap<BigUint, BigUint>,
}
impl PrimeFactors {
    #[allow(unused)] pub fn factor_to_power(&self) -> &BTreeMap<BigUint, BigUint> { &self.factor_to_power }

    pub fn pathological(value: &BigUint) -> Option<PrimeFactors> {
        let two = BigUint::from(2u8);
        if value < &two {
            let mut factor_to_power = BTreeMap::new();
            factor_to_power.insert(value.clone(), BigUint::from(1u8));
            Some(Self {
                factor_to_power,
            })
        } else {
            None
        }
    }

    fn format_wrap_biguint(
        number: &BigUint,
        start: &str, end: &str, start_multidigit: &str, end_multidigit: &str,
        thousands_separator: &str,
    ) -> String {
        let mut string = number.to_string();
        add_thousands_separators(&mut string, thousands_separator);
        let is_multidigit = string.chars().count() > 1;
        if is_multidigit {
            format!("{}{}{}", start_multidigit, string, end_multidigit)
        } else {
            format!("{}{}{}", start, string, end)
        }
    }

    pub fn to_formatted_string(
        &self,
        start_wrapper: &str,
        end_wrapper: &str,
        start_base: &str,
        end_base: &str,
        start_base_multidigit: &str,
        end_base_multidigit: &str,
        base_thousands_separator: &str,
        start_power: &str,
        end_power: &str,
        start_power_multidigit: &str,
        end_power_multidigit: &str,
        power_thousands_separator: &str,
        power_operator: &str,
        multiply_operator: &str,
    ) -> String {
        let one = BigUint::from(1u8);
        let power_strings: Vec<String> = self.factor_to_power
            .iter()
            .map(|(base, power)| {
                let base_wrapped = Self::format_wrap_biguint(
                    base,
                    start_base, end_base,
                    start_base_multidigit, end_base_multidigit,
                    base_thousands_separator,
                );
                if power == &one {
                    base_wrapped
                } else {
                    let power_wrapped = Self::format_wrap_biguint(
                        power,
                        start_power, end_power,
                        start_power_multidigit, end_power_multidigit,
                        power_thousands_separator,
                    );
                    format!("{}{}{}", base_wrapped, power_operator, power_wrapped)
                }
            })
            .collect();
        let all_powers_string = power_strings.join(multiply_operator);
        format!("{}{}{}", start_wrapper, all_powers_string, end_wrapper)
    }

    pub fn to_tex_string(&self) -> String {
        self.to_formatted_string(
            "\\[", "\\]",
            "", "", "", "",
            "\\,",
            "", "", "{", "}",
            "\\,",
            "^",
            "\\cdot ",
        )
    }

    pub fn to_code_string(&self) -> String {
        self.to_formatted_string(
            "`", "`",
            "", "", "", "",
            "",
            "", "", "", "",
            "",
            "**",
            " * ",
        )
    }
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PrimeCache {
    primes: BTreeSet<BigUint>,
    maximum: BigUint,
}
impl PrimeCache {
    pub fn new() -> Self { Self::default() }

    #[allow(unused)] pub fn primes(&self) -> &BTreeSet<BigUint> { &self.primes }
    #[allow(unused)] pub fn maximum(&self) -> &BigUint { &self.maximum }

    pub fn is_prime(&self, num: &BigUint) -> Option<bool> {
        if num > &self.maximum {
            None
        } else {
            Some(self.primes.contains(num))
        }
    }

    pub fn extend_while<P: FnMut(&BigUint) -> bool>(&mut self, stopper: &AtomicBool, mut predicate: P) {
        let zero = BigUint::from(0u8);
        let one = BigUint::from(1u8);

        let mut current = &self.maximum + &one;
        while predicate(&current) {
            let mut is_prime = true;
            for prime in &self.primes {
                if stopper.load(Ordering::Relaxed) {
                    return;
                }

                if &current % prime == zero {
                    // composite!
                    is_prime = false;
                    break;
                }
            }

            if is_prime {
                self.primes.insert(current.clone());
            }

            current += &one;
        }
        self.maximum = current - &one;
    }

    #[allow(unused)]
    pub fn extend_to(&mut self, new_maximum: &BigUint, stopper: &AtomicBool) {
        self.extend_while(stopper, |current| current <= new_maximum);
    }

    pub fn extend_until_divisible(&mut self, factor_me: &BigUint, stopper: &AtomicBool) {
        let zero = BigUint::from(0u8);
        let mut stop_marker = false;
        self.extend_while(stopper, |current| {
            if stop_marker {
                false
            } else if current > factor_me {
                // out of range
                false
            } else if factor_me % current == zero {
                // stop next time
                stop_marker = true;
                true
            } else {
                // keep going
                true
            }
        });
    }

    pub fn try_factor(&self, number: &BigUint, stopper: &AtomicBool) -> FactorResult {
        if stopper.load(Ordering::Relaxed) {
            return FactorResult::Halted;
        }

        let zero = BigUint::from(0u8);
        let one = BigUint::from(1u8);

        // fast-path
        if number <= &one {
            return FactorResult::Factored(PrimeFactors::default());
        }
        if self.is_prime(number).unwrap_or(false) {
            let mut quick_ret = BTreeMap::new();
            quick_ret.insert(number.clone(), one);
            return FactorResult::Factored(PrimeFactors { factor_to_power: quick_ret });
        }

        let mut ret = BTreeMap::new();
        let mut cur_number = number.clone();

        // try to factor it with what we have
        for prime in &self.primes {
            while &cur_number % prime == zero {
                if stopper.load(Ordering::Relaxed) {
                    return FactorResult::Halted;
                }

                // it's a factor!
                cur_number /= prime;

                let cur_count = ret.entry(prime.clone())
                    .or_insert_with(|| zero.clone());
                *cur_count += &one;
            }
        }

        // possibly prime
        if cur_number > self.maximum {
            // one of those unknowables
            FactorResult::Stuck(cur_number)
        } else {
            // provably prime!
            FactorResult::Factored(PrimeFactors { factor_to_power: ret })
        }
    }

    pub fn factor_caching(&mut self, number: &BigUint, stopper: &AtomicBool) -> Option<PrimeFactors> {
        loop {
            // try the standard factoring
            let stuck_number = match self.try_factor(number, stopper) {
                FactorResult::Factored(factors) => return Some(factors),
                FactorResult::Stuck(s) => s,
                FactorResult::Halted => return None,
            };

            // extend primeness knowledge to the number we are stuck at
            self.extend_until_divisible(&stuck_number, stopper);

            // try again
        }
    }
}
impl Default for PrimeCache {
    fn default() -> Self {
        Self {
            primes: BTreeSet::new(),
            maximum: BigUint::from(1u8),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prime_calculation() {
        let mut cache = PrimeCache::new();
        let stopper = AtomicBool::new(false);
        cache.extend_to(&BigUint::from(100u8), &stopper);

        // stolen from OEIS A000040
        let primes_to_100: [u8; 25] = [
            2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53,
            59, 61, 67, 71, 73, 79, 83, 89, 97
        ];

        assert_eq!(cache.primes().len(), primes_to_100.len());
        for prime in primes_to_100 {
            if !cache.primes().contains(&BigUint::from(prime)) {
                panic!("prime {} not contained", prime);
            }
        }
    }

    #[test]
    fn test_try_factor() {
        let mut cache = PrimeCache::new();
        let stopper = AtomicBool::new(false);
        cache.extend_to(&BigUint::from(100u8), &stopper);

        assert_eq!(cache.primes().len(), 25);

        let too_large_factors_u16: u16 = 2 * 3 * 109;
        let too_large_factors = BigUint::from(too_large_factors_u16);
        assert_eq!(cache.try_factor(&too_large_factors, &stopper), FactorResult::Stuck(BigUint::from(109u8)));

        assert_eq!(cache.primes().len(), 25);
    }

    #[test]
    fn test_factor_caching() {
        let mut cache = PrimeCache::new();
        let stopper = AtomicBool::new(false);
        cache.extend_to(&BigUint::from(100u8), &stopper);

        assert_eq!(cache.primes().len(), 25);

        let too_large_factors_u32: u32 = 2 * 3 * 109 * 127;
        let too_large_factors = BigUint::from(too_large_factors_u32);
        let factors = cache.factor_caching(&too_large_factors, &stopper).unwrap();

        fn factor_is(factors: &PrimeFactors, factor: u64, expected_power: u64) {
            let factor_bu = BigUint::from(factor);
            let expected_power_bu = BigUint::from(expected_power);
            assert_eq!(factors.factor_to_power().get(&factor_bu), Some(&expected_power_bu));
        }

        assert_eq!(factors.factor_to_power().len(), 4);
        factor_is(&factors, 2, 1);
        factor_is(&factors, 3, 1);
        factor_is(&factors, 109, 1);
        factor_is(&factors, 127, 1);

        // extended by 101, 103, 107, 109, 113, 127
        assert_eq!(cache.primes().len(), 31);
    }

    #[test]
    fn test_factor_formatting() {
        let mut cache = PrimeCache::new();
        let stopper = AtomicBool::new(false);
        cache.extend_to(&BigUint::from(100u8), &stopper);

        // 2**11 * 3 * 11**2 * 13 == 9_664_512
        // contains:
        // 1. single-digit base with multi-digit power
        // 2. single-digit base with power 1
        // 3. multi-digit base with single-digit power
        // 4. multi-digit base with power 1
        // and should therefore be a good test
        let number = BigUint::from(9_664_512u32);
        let number_factoring_result = cache.try_factor(&number, &stopper);
        let number_factors = number_factoring_result
            .as_factored().unwrap();

        assert_eq!(number_factors.to_tex_string(), "\\[2^{11}\\cdot 3\\cdot 11^2\\cdot 13\\]");
        assert_eq!(number_factors.to_code_string(), "`2**11 * 3 * 11**2 * 13`");
    }
}
