use std::fmt;

use rand::RngExt;
use rand::rngs::ThreadRng;


#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GeneratedTemperature {
    pub before_decimal_point: i8,
    pub after_decimal_point: u8,
}
impl GeneratedTemperature {
    pub fn generate_snowy_continental() -> Self {
        // before decimal point: -10..4 (0..14 - 10)
        // after decimal point: 0..10
        let mut rng: ThreadRng = rand::rng();
        let before_decimal_point = rng.random_range(0..14) - 10;
        let after_decimal_point = rng.random_range(0..10);
        Self {
            before_decimal_point,
            after_decimal_point,
        }
    }
}
impl fmt::Display for GeneratedTemperature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.before_decimal_point, self.after_decimal_point)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TemperatureRange {
    lower: GeneratedTemperature,
    higher: GeneratedTemperature,
}
impl TemperatureRange {
    pub fn new(one_end: GeneratedTemperature, other_end: GeneratedTemperature) -> Self {
        let mut ret = Self {
            lower: one_end,
            higher: other_end,
        };
        ret.ensure_order();
        ret
    }

    pub fn lower(&self) -> GeneratedTemperature { self.lower }
    pub fn higher(&self) -> GeneratedTemperature { self.higher }

    #[allow(unused)]
    pub fn set_lower(&mut self, new_lower: GeneratedTemperature) {
        self.lower = new_lower;
        self.ensure_order();
    }

    #[allow(unused)]
    pub fn set_higher(&mut self, new_higher: GeneratedTemperature) {
        self.higher = new_higher;
        self.ensure_order();
    }

    fn ensure_order(&mut self) {
        if self.higher < self.lower {
            (self.higher, self.lower) = (self.lower, self.higher);
        }
    }

    pub fn ensure_contains(&mut self, temperature: GeneratedTemperature) {
        if temperature < self.lower {
            self.lower = temperature;
        }
        if temperature > self.higher {
            self.higher = temperature;
        }
        self.ensure_order();
    }

    pub fn generate_snowy_continental() -> Self {
        let one_end = GeneratedTemperature::generate_snowy_continental();
        let other_end = GeneratedTemperature::generate_snowy_continental();
        Self::new(one_end, other_end)
    }
}
impl fmt::Display for TemperatureRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\u{2013}{}", self.lower(), self.higher())
    }
}
