use std::collections::{BTreeMap, HashMap};
use std::collections::hash_map::Entry;
use std::fmt;

use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

use crate::numbers::{Number, NumberValue};


pub type NumberUnits = BTreeMap<String, BigInt>;


#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub(crate) struct BaseUnit {
    pub letters: String,
}
impl BaseUnit {
    pub fn new(
        letters: String,
    ) -> Self {
        Self {
            letters,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct DerivedUnit {
    pub letters: String,
    #[serde(with = "number_units_serde")]
    pub parents: NumberUnits,
    pub factor_of_parents: f64,
}
impl DerivedUnit {
    pub fn new(
        letters: String,
        parents: NumberUnits,
        factor_of_parents: f64,
    ) -> Self {
        Self {
            letters,
            parents,
            factor_of_parents,
        }
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum UnitDatabaseError {
    ExistsAsBaseUnit(String),
    ExistsAsDerivedUnit(String),
    UnknownParentUnit(String),
    UnknownDerivedUnit(String),
}
impl fmt::Display for UnitDatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExistsAsBaseUnit(s) => write!(f, "new unit {:?} already exists as a base unit", s),
            Self::ExistsAsDerivedUnit(s) => write!(f, "new unit {:?} already exists as a derived unit", s),
            Self::UnknownParentUnit(s) => write!(f, "referenced base unit {:?} not found", s),
            Self::UnknownDerivedUnit(s) => write!(f, "derived unit {:?} not found", s),
        }
    }
}
impl std::error::Error for UnitDatabaseError {
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UnitDatabase {
    si_prefix_to_factor: HashMap<String, f64>,
    letters_to_base_unit: HashMap<String, BaseUnit>,
    letters_to_derived_unit: HashMap<String, DerivedUnit>,
    letters_to_max_depth: HashMap<String, usize>,
}
impl UnitDatabase {
    pub fn new_empty() -> Self {
        Self {
            si_prefix_to_factor: HashMap::new(),
            letters_to_base_unit: HashMap::new(),
            letters_to_derived_unit: HashMap::new(),
            letters_to_max_depth: HashMap::new(),
        }
    }

    pub fn insert_canonical_si_prefixes(&mut self) {
        self.si_prefix_to_factor.insert("Q".to_owned(), 1e30);
        self.si_prefix_to_factor.insert("R".to_owned(), 1e27);
        self.si_prefix_to_factor.insert("Y".to_owned(), 1e24);
        self.si_prefix_to_factor.insert("Z".to_owned(), 1e21);
        self.si_prefix_to_factor.insert("E".to_owned(), 1e18);
        self.si_prefix_to_factor.insert("P".to_owned(), 1e15);
        self.si_prefix_to_factor.insert("T".to_owned(), 1e12);
        self.si_prefix_to_factor.insert("G".to_owned(), 1e9);
        self.si_prefix_to_factor.insert("M".to_owned(), 1e6);
        self.si_prefix_to_factor.insert("k".to_owned(), 1e3);
        self.si_prefix_to_factor.insert("h".to_owned(), 1e2);
        self.si_prefix_to_factor.insert("da".to_owned(), 1e1);

        self.si_prefix_to_factor.insert("d".to_owned(), 1e-1);
        self.si_prefix_to_factor.insert("c".to_owned(), 1e-2);
        self.si_prefix_to_factor.insert("m".to_owned(), 1e-3);
        // allow two variants of micro-
        // rationale is documented in calc_lang.pest
        self.si_prefix_to_factor.insert("\u{00B5}".to_owned(), 1e-6);
        self.si_prefix_to_factor.insert("\u{03BC}".to_owned(), 1e-6);
        self.si_prefix_to_factor.insert("n".to_owned(), 1e-9);
        self.si_prefix_to_factor.insert("p".to_owned(), 1e-12);
        self.si_prefix_to_factor.insert("f".to_owned(), 1e-15);
        self.si_prefix_to_factor.insert("a".to_owned(), 1e-18);
        self.si_prefix_to_factor.insert("z".to_owned(), 1e-21);
        self.si_prefix_to_factor.insert("y".to_owned(), 1e-24);
        self.si_prefix_to_factor.insert("r".to_owned(), 1e-27);
        self.si_prefix_to_factor.insert("q".to_owned(), 1e-30);
    }

    pub fn get_base_unit(&self, letters: &str) -> Option<BaseUnit> {
        self.letters_to_base_unit.get(letters)
            .map(|bu| bu.clone())
    }

    pub fn get_derived_unit(&self, letters: &str) -> Option<DerivedUnit> {
        // does it exist?
        if let Some(du) = self.letters_to_derived_unit.get(letters) {
            // yes
            return Some(du.clone());
        }

        // try applying SI prefixes
        for (si_pfx, ten_pow) in &self.si_prefix_to_factor {
            if !letters.starts_with(si_pfx) {
                continue;
            }

            let non_prefix_unit = &letters[si_pfx.len()..];
            // don't allow multiprefix units (otherwise replace with recursive call)
            if self.letters_to_derived_unit.contains_key(non_prefix_unit) || self.letters_to_base_unit.contains_key(non_prefix_unit) {
                // perfect

                // synthesize a derived unit
                let mut synth_parents = BTreeMap::new();
                synth_parents.insert(non_prefix_unit.to_owned(), BigInt::from(1));
                let synth_derived_unit = DerivedUnit::new(
                    letters.to_owned(),
                    synth_parents,
                    *ten_pow,
                );
                return Some(synth_derived_unit);
            }
        }

        None
    }

    pub fn get_max_depth(&self, letters: &str) -> Option<usize> {
        // does it exist?
        if let Some(d) = self.letters_to_max_depth.get(letters).map(|s| *s) {
            // yes
            return Some(d);
        }

        // try applying SI prefixes
        for (si_pfx, _ten_pow) in &self.si_prefix_to_factor {
            if letters.starts_with(si_pfx) {
                let non_prefix_unit = &letters[si_pfx.len()..];
                // don't allow multiprefix units (otherwise replace with recursive call)
                if let Some(md) = self.letters_to_max_depth.get(non_prefix_unit) {
                    return Some(*md + 1);
                }
            }
        }

        None
    }

    pub fn register_base_unit(&mut self, base_unit: BaseUnit) -> Result<(), UnitDatabaseError> {
        if self.get_derived_unit(&base_unit.letters).is_some() {
            return Err(UnitDatabaseError::ExistsAsDerivedUnit(base_unit.letters.clone()));
        }

        match self.letters_to_base_unit.entry(base_unit.letters.clone()) {
            Entry::Occupied(_oe) => {
                Err(UnitDatabaseError::ExistsAsBaseUnit(base_unit.letters.clone()))
            },
            Entry::Vacant(ve) => {
                self.letters_to_max_depth.insert(base_unit.letters.clone(), 0);
                ve.insert(base_unit);
                Ok(())
            },
        }
    }

    pub fn register_derived_unit(&mut self, derived_unit: DerivedUnit) -> Result<(), UnitDatabaseError> {
        if self.get_base_unit(&derived_unit.letters).is_some() {
            return Err(UnitDatabaseError::ExistsAsBaseUnit(derived_unit.letters.clone()));
        }

        // don't call self.get_derived_unit() here
        // we want to allow custom units to trump autogenerated SI-prefixed units
        match self.letters_to_derived_unit.get(&derived_unit.letters) {
            Some(_derived_unit) => {
                Err(UnitDatabaseError::ExistsAsDerivedUnit(derived_unit.letters.clone()))
            },
            None => {
                let mut parent_max_depth = 0;
                for parent_letters in derived_unit.parents.keys() {
                    match self.get_max_depth(parent_letters) {
                        None => {
                            return Err(UnitDatabaseError::UnknownParentUnit(parent_letters.clone()));
                        },
                        Some(d) => {
                            parent_max_depth = parent_max_depth.max(d);
                        },
                    }
                }

                self.letters_to_max_depth.insert(derived_unit.letters.clone(), parent_max_depth + 1);
                self.letters_to_derived_unit.insert(derived_unit.letters.clone(),derived_unit);
                Ok(())
            },
        }
    }

    pub fn change_derived_unit_factor(&mut self, letters: &str, factor: f64) -> Result<(), UnitDatabaseError> {
        if self.letters_to_base_unit.contains_key(letters) {
            return Err(UnitDatabaseError::ExistsAsBaseUnit(letters.to_owned()));
        }

        match self.letters_to_derived_unit.get_mut(letters) {
            Some(derived_unit) => {
                derived_unit.factor_of_parents = factor;
                Ok(())
            },
            None => {
                Err(UnitDatabaseError::UnknownDerivedUnit(letters.to_owned()))
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct StoredUnitDatabase {
    base_units: Vec<BaseUnit>,
    derived_units: Vec<DerivedUnit>,
}
impl StoredUnitDatabase {
    pub fn to_unit_database(&self) -> Result<UnitDatabase, UnitDatabaseError> {
        let mut unit_db = UnitDatabase::new_empty();
        unit_db.insert_canonical_si_prefixes();

        for base_unit in &self.base_units {
            unit_db.register_base_unit(base_unit.clone())?;
        }

        for derived_unit in &self.derived_units {
            unit_db.register_derived_unit(derived_unit.clone())?;
        }

        Ok(unit_db)
    }
}

pub(crate) fn expand_number_unit(num: &Number, unit_letters: &str, database: &UnitDatabase) -> Number {
    let unit = match database.get_derived_unit(unit_letters) {
        Some(u) => u,
        None => return num.clone(),
    };

    let current_power = match num.units.get(unit_letters) {
        None => return num.clone(),
        Some(p) => p.clone(),
    };
    assert_ne!(current_power, BigInt::from(0));
    let reduce = current_power < BigInt::from(0);

    let new_value = if reduce {
        num.value.checked_div(NumberValue::Float(unit.factor_of_parents))
            .expect("division failed")
    } else {
        num.value.checked_mul(NumberValue::Float(unit.factor_of_parents))
            .expect("multiplication failed")
    };

    // update the units from the parent
    let mut new_units = num.units.clone();
    for (parent_letters, parent_pow) in &unit.parents {
        let new_pow = new_units.entry(parent_letters.clone())
            .or_insert_with(|| BigInt::from(0));
        if reduce {
            *new_pow -= parent_pow;
        } else {
            *new_pow += parent_pow;
        }
    }

    // remove one power of the original unit
    let reduce_me = new_units.entry(unit_letters.to_owned())
        .or_insert(BigInt::from(0));
    if reduce {
        *reduce_me += 1;
    } else {
        *reduce_me -= 1;
    }

    // remove zero units
    new_units.retain(|_name, power| power != &BigInt::from(0));

    Number::new(
        new_value,
        new_units,
    )
}

fn do_coerce_to_common_unit(
    left: Number,
    right: Number,
    database: &UnitDatabase,
) -> Option<(Number, Number)> {
    // fast-path
    if left.units == right.units {
        return Some((left.clone(), right.clone()));
    }

    // collect relevant units by depth
    let mut unit_by_depth = HashMap::new();
    for (unit, _power) in left.units.iter().chain(right.units.iter()) {
        match database.get_max_depth(&unit) {
            None => return None, // unknown unit
            Some(d) => {
                unit_by_depth.insert(unit.clone(), d);
            },
        }
    }
    let mut depths_and_units: Vec<(usize, String)> = unit_by_depth.drain()
        .map(|(u, d)| (d, u))
        .collect();
    depths_and_units.sort_unstable_by_key(|(u, d)| (usize::MAX - u, d.clone()));

    for (_depth, unit) in depths_and_units {
        let left_has_unit = left.units.contains_key(&unit);
        let right_has_unit = right.units.contains_key(&unit);

        if left_has_unit && !right_has_unit {
            // expand the left unit
            let new_left = expand_number_unit(&left, &unit, &database);
            if left != new_left {
                return do_coerce_to_common_unit(new_left, right, database);
            }
        } else if !left_has_unit && right_has_unit {
            // expand the right unit
            let new_right = expand_number_unit(&right, &unit, &database);
            if right != new_right {
                return do_coerce_to_common_unit(left, new_right, database);
            }
        }
    }

    // nothing...
    None
}

pub(crate) fn coerce_to_common_unit(
    left: &Number,
    right: &Number,
    database: &UnitDatabase,
) -> Option<(Number, Number)> {
    do_coerce_to_common_unit(left.clone(), right.clone(), database)
}

pub(crate) fn coerce_to_unit(
    number: &Number,
    units: &NumberUnits,
    database: &UnitDatabase,
) -> Option<Number> {
    let template_number = Number::new(
        NumberValue::Float(1.0),
        units.clone(),
    );

    // attempt to coerce them to a common unit
    let (c_number, c_template) = do_coerce_to_common_unit(
        number.clone(),
        template_number,
        database,
    )?;

    // that succeeded
    // c_template now contains the factor to convert from the target units to the common unit
    // divide by it to get from the value in the common unit to the value in the target unit
    Some(Number::new(
        c_number.value / c_template.value,
        units.clone(),
    ))
}

mod number_units_serde {
    use super::NumberUnits;
    use std::collections::BTreeMap;
    use num_bigint::BigInt;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde::de::Error;

    pub fn serialize<S: Serializer>(units: &NumberUnits, s: S) -> Result<S::Ok, S::Error> {
        // transform into string-to-string dict
        let mut units_stringy = BTreeMap::new();
        for (letters, power) in units {
            units_stringy.insert(letters.clone(), power.to_string());
        }
        units_stringy.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NumberUnits, D::Error> {
        // deserialize as string-to-string dict
        let units_stringy: BTreeMap<String, String> = BTreeMap::deserialize(d)?;
        let mut units = BTreeMap::new();
        for (letters, power_str) in &units_stringy {
            let power: BigInt = power_str.parse()
                .map_err(|e| D::Error::custom(format!("failed to make BigInt from {:?}: {}", power_str, e)))?;
            units.insert(letters.clone(), power);
        }
        Ok(units)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_database() -> UnitDatabase {
        let mut db = UnitDatabase::new_empty();
        db.insert_canonical_si_prefixes();

        db.register_base_unit(BaseUnit::new("kg".to_owned())).unwrap();
        db.register_base_unit(BaseUnit::new("m".to_owned())).unwrap();
        db.register_base_unit(BaseUnit::new("s".to_owned())).unwrap();
        db.register_base_unit(BaseUnit::new("A".to_owned())).unwrap();

        {
            let mut s = NumberUnits::new();
            s.insert("s".to_owned(), BigInt::from(1));
            db.register_derived_unit(DerivedUnit::new("h".to_owned(), s, 60.0*60.0)).unwrap();
        }

        {
            let mut sx1 = NumberUnits::new();
            sx1.insert("s".to_owned(), BigInt::from(-1));
            db.register_derived_unit(DerivedUnit::new("Hz".to_owned(), sx1, 1.0)).unwrap();
        }

        {
            let mut kg_m_sx2 = NumberUnits::new();
            kg_m_sx2.insert("kg".to_owned(), BigInt::from(1));
            kg_m_sx2.insert("m".to_owned(), BigInt::from(1));
            kg_m_sx2.insert("s".to_owned(), BigInt::from(-2));
            db.register_derived_unit(DerivedUnit::new("N".to_owned(), kg_m_sx2, 1.0)).unwrap();
        }

        {
            let mut n_mx2 = NumberUnits::new();
            n_mx2.insert("N".to_owned(), BigInt::from(1));
            n_mx2.insert("m".to_owned(), BigInt::from(-2));
            db.register_derived_unit(DerivedUnit::new("Pa".to_owned(), n_mx2, 1.0)).unwrap();
        }

        {
            let mut m = NumberUnits::new();
            m.insert("m".to_owned(), BigInt::from(1));
            db.register_derived_unit(DerivedUnit::new("in".to_owned(), m, 0.0254)).unwrap();
        }

        {
            let mut inch = NumberUnits::new();
            inch.insert("in".to_owned(), BigInt::from(1));
            db.register_derived_unit(DerivedUnit::new("ft".to_owned(), inch, 12.0)).unwrap();
        }

        {
            let mut kg = NumberUnits::new();
            kg.insert("kg".to_owned(), BigInt::from(1));
            db.register_derived_unit(DerivedUnit::new("lb".to_owned(), kg, 0.45359237)).unwrap();
        }

        {
            let mut ft_lb_sx2 = NumberUnits::new();
            ft_lb_sx2.insert("ft".to_owned(), BigInt::from(1));
            ft_lb_sx2.insert("lb".to_owned(), BigInt::from(1));
            ft_lb_sx2.insert("s".to_owned(), BigInt::from(-2));
            db.register_derived_unit(DerivedUnit::new("lbf".to_owned(), ft_lb_sx2, 32.1740485564304)).unwrap();
        }

        {
            let mut kg_m2_sx3_ax1 = NumberUnits::new();
            kg_m2_sx3_ax1.insert("kg".to_owned(), BigInt::from(1));
            kg_m2_sx3_ax1.insert("m".to_owned(), BigInt::from(2));
            kg_m2_sx3_ax1.insert("s".to_owned(), BigInt::from(-3));
            kg_m2_sx3_ax1.insert("A".to_owned(), BigInt::from(-1));
            db.register_derived_unit(DerivedUnit::new("V".to_owned(), kg_m2_sx3_ax1, 1.0)).unwrap();
        }

        {
            let mut kg_m2_sx3 = NumberUnits::new();
            kg_m2_sx3.insert("kg".to_owned(), BigInt::from(1));
            kg_m2_sx3.insert("m".to_owned(), BigInt::from(2));
            kg_m2_sx3.insert("s".to_owned(), BigInt::from(-3));
            db.register_derived_unit(DerivedUnit::new("W".to_owned(), kg_m2_sx3, 1.0)).unwrap();
        }

        db
    }

    #[test]
    fn test_synonymous_unit_coercion() {
        // N = kg1 m1 s-2
        // Pa = kg1 m-1 s-2

        // Pa = N m-2

        let database = make_database();

        let mut n_mx2_units = NumberUnits::new();
        n_mx2_units.insert("N".to_owned(), BigInt::from(1));
        n_mx2_units.insert("m".to_owned(), BigInt::from(-2));
        let n_mx2 = Number::new(NumberValue::Float(42.0), n_mx2_units);

        let mut pa_units = NumberUnits::new();
        pa_units.insert("Pa".to_owned(), BigInt::from(1));
        let pa = Number::new(NumberValue::Float(42.0), pa_units);

        let (new_n_mx2, new_pa) = coerce_to_common_unit(&n_mx2, &pa, &database).unwrap();
        assert_eq!(NumberValue::Float(42.0), new_n_mx2.value);
        assert_eq!(NumberValue::Float(42.0), new_pa.value);
    }

    #[test]
    fn test_converted_unit_coercion() {
        let database = make_database();

        let mut m_units = NumberUnits::new();
        m_units.insert("m".to_owned(), BigInt::from(1));
        let m = Number::new(NumberValue::Float(42.0), m_units);

        let mut inch_units = NumberUnits::new();
        inch_units.insert("in".to_owned(), BigInt::from(1));
        let inch = Number::new(NumberValue::Float(42.0), inch_units);

        let (new_m, new_inch) = coerce_to_common_unit(&m, &inch, &database).unwrap();

        assert_eq!(NumberValue::Float(42.0), new_m.value);
        assert_eq!(1, new_m.units.len());
        assert_eq!(Some(&BigInt::from(1)), new_m.units.get("m"));

        assert_eq!(NumberValue::Float(1.0668), new_inch.value);
        assert_eq!(1, new_inch.units.len());
        assert_eq!(Some(&BigInt::from(1)), new_inch.units.get("m"));
    }

    #[test]
    fn test_power() {
        let database = make_database();

        let mut km3_units = NumberUnits::new();
        km3_units.insert("km".to_owned(), BigInt::from(3));
        let km3 = Number::new(NumberValue::Float(42.0), km3_units);

        let mut m3_units = NumberUnits::new();
        m3_units.insert("m".to_owned(), BigInt::from(3));
        let m3 = Number::new(NumberValue::Float(42.0), m3_units);

        let (new_km3, new_m3) = coerce_to_common_unit(&km3, &m3, &database).unwrap();

        assert_eq!(NumberValue::Float(42.0e9), new_km3.value);
        assert_eq!(1, new_km3.units.len());
        assert_eq!(Some(&BigInt::from(3)), new_km3.units.get("m"));

        assert_eq!(NumberValue::Float(42.0), new_m3.value);
        assert_eq!(1, new_m3.units.len());
        assert_eq!(Some(&BigInt::from(3)), new_m3.units.get("m"));
    }

    #[test]
    fn test_converted_unit_power_coercion() {
        let database = make_database();

        let mut m_units = NumberUnits::new();
        m_units.insert("m".to_owned(), BigInt::from(3));
        let m = Number::new(NumberValue::Float(42.0), m_units);

        let mut inch_units = NumberUnits::new();
        inch_units.insert("in".to_owned(), BigInt::from(3));
        let inch = Number::new(NumberValue::Float(42.0), inch_units);

        let (new_m, new_inch) = coerce_to_common_unit(&m, &inch, &database).unwrap();

        assert_eq!(NumberValue::Float(42.0), new_m.value);
        assert_eq!(1, new_m.units.len());
        assert_eq!(Some(&BigInt::from(3)), new_m.units.get("m"));

        assert_eq!(NumberValue::Float(0.0006882566879999999), new_inch.value);
        assert_eq!(1, new_inch.units.len());
        assert_eq!(Some(&BigInt::from(3)), new_inch.units.get("m"));
    }

    #[test]
    fn test_unrelated_coercion() {
        let database = make_database();

        let mut m_units = NumberUnits::new();
        m_units.insert("m".to_owned(), BigInt::from(1));
        let m = Number::new(NumberValue::Float(42.0), m_units);

        let mut s_units = NumberUnits::new();
        s_units.insert("s".to_owned(), BigInt::from(1));
        let s = Number::new(NumberValue::Float(42.0), s_units);

        assert_eq!(None, coerce_to_common_unit(&m, &s, &database));
    }

    #[test]
    fn test_targeted_coercion() {
        let database = make_database();

        let mut m_units = NumberUnits::new();
        m_units.insert("m".to_owned(), BigInt::from(1));
        let m = Number::new(NumberValue::Float(42.0), m_units);

        let mut inch_units = NumberUnits::new();
        inch_units.insert("in".to_owned(), BigInt::from(1));

        let inch = coerce_to_unit(&m, &inch_units, &database).unwrap();
        assert_eq!(NumberValue::Float(1653.5433070866143), inch.value);
        assert_eq!(1, inch.units.len());
        assert_eq!(Some(&BigInt::from(1)), inch.units.get("in"));
    }

    #[test]
    fn test_indirect_targeted_coercion() {
        let database = make_database();

        let mut lbf_units = NumberUnits::new();
        lbf_units.insert("lbf".to_owned(), BigInt::from(1));
        let lbf = Number::new(NumberValue::Float(42.0), lbf_units);

        let mut n_units = NumberUnits::new();
        n_units.insert("N".to_owned(), BigInt::from(1));

        let n = coerce_to_unit(&lbf, &n_units, &database).unwrap();
        assert_eq!(NumberValue::Float(186.82530784094072), n.value);
        assert_eq!(1, n.units.len());
        assert_eq!(Some(&BigInt::from(1)), n.units.get("N"));
    }

    #[test]
    fn test_incompatible_targeted_coercion() {
        let database = make_database();

        let mut m_units = NumberUnits::new();
        m_units.insert("m".to_owned(), BigInt::from(1));
        let m = Number::new(NumberValue::Float(42.0), m_units);

        let mut s_units = NumberUnits::new();
        s_units.insert("s".to_owned(), BigInt::from(1));

        assert_eq!(None, coerce_to_unit(&m, &s_units, &database));
    }

    #[test]
    fn test_coercion_negative_powers() {
        // W = kg1 m2 s-3
        // V = kg1 m2 s-3 A-1

        // V = W A-1

        let database = make_database();

        let mut v_units = NumberUnits::new();
        v_units.insert("V".to_owned(), BigInt::from(1));
        let v = Number::new(NumberValue::Float(42.0), v_units);

        let mut w_ax1_units = NumberUnits::new();
        w_ax1_units.insert("W".to_owned(), BigInt::from(1));
        w_ax1_units.insert("A".to_owned(), BigInt::from(-1));
        let w_ax1 = Number::new(NumberValue::Float(42.0), w_ax1_units);

        let (new_v, new_w_ax1) = coerce_to_common_unit(&v, &w_ax1, &database).unwrap();
        assert_eq!(NumberValue::Float(42.0), new_v.value);
        assert_eq!(4, new_v.units.len());
        assert_eq!(Some(&BigInt::from(1)), new_v.units.get("kg"));
        assert_eq!(Some(&BigInt::from(2)), new_v.units.get("m"));
        assert_eq!(Some(&BigInt::from(-3)), new_v.units.get("s"));
        assert_eq!(Some(&BigInt::from(-1)), new_v.units.get("A"));
        assert_eq!(NumberValue::Float(42.0), new_w_ax1.value);
        assert_eq!(4, new_w_ax1.units.len());
        assert_eq!(Some(&BigInt::from(1)), new_w_ax1.units.get("kg"));
        assert_eq!(Some(&BigInt::from(2)), new_w_ax1.units.get("m"));
        assert_eq!(Some(&BigInt::from(-3)), new_w_ax1.units.get("s"));
        assert_eq!(Some(&BigInt::from(-1)), new_w_ax1.units.get("A"));
    }

    #[test]
    fn test_m_sx1_to_km_hx1() {
        let database = make_database();

        let mut m_sx1_units = NumberUnits::new();
        m_sx1_units.insert("m".to_owned(), BigInt::from(1));
        m_sx1_units.insert("s".to_owned(), BigInt::from(-1));
        let m_sx1 = Number::new(NumberValue::Float(1.0), m_sx1_units);

        let mut km_hx1_units = NumberUnits::new();
        km_hx1_units.insert("km".to_owned(), BigInt::from(1));
        km_hx1_units.insert("h".to_owned(), BigInt::from(-1));

        let epsilon = 1e-10;

        let km_hx1 = coerce_to_unit(&m_sx1, &km_hx1_units, &database).unwrap();
        if let NumberValue::Float(f) = km_hx1.value {
            assert!((f - 3.6).abs() < epsilon);
        } else {
            panic!("number value not float");
        }
        assert_eq!(2, km_hx1.units.len());
        assert_eq!(Some(&BigInt::from(1)), km_hx1.units.get("km"));
        assert_eq!(Some(&BigInt::from(-1)), km_hx1.units.get("h"));
    }
}
