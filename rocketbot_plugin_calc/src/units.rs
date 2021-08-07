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
    ExistsAsBaseUnit,
    ExistsAsDerivedUnit,
    UnknownBaseUnit(String),
}
impl fmt::Display for UnitDatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExistsAsBaseUnit => write!(f, "new unit already exists as a base unit"),
            Self::ExistsAsDerivedUnit => write!(f, "new unit already exists as a derived unit"),
            Self::UnknownBaseUnit(s) => write!(f, "referenced unit {:?} not found", s),
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
        self.si_prefix_to_factor.insert("\u{03BC}".to_owned(), 1e-6);
        self.si_prefix_to_factor.insert("n".to_owned(), 1e-9);
        self.si_prefix_to_factor.insert("p".to_owned(), 1e-12);
        self.si_prefix_to_factor.insert("f".to_owned(), 1e-15);
        self.si_prefix_to_factor.insert("a".to_owned(), 1e-18);
        self.si_prefix_to_factor.insert("z".to_owned(), 1e-21);
        self.si_prefix_to_factor.insert("y".to_owned(), 1e-24);
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
            if !letters.starts_with(si_pfx) {
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
        if self.letters_to_derived_unit.get(&base_unit.letters).is_some() {
            return Err(UnitDatabaseError::ExistsAsDerivedUnit);
        }

        match self.letters_to_base_unit.entry(base_unit.letters.clone()) {
            Entry::Occupied(_oe) => {
                Err(UnitDatabaseError::ExistsAsBaseUnit)
            },
            Entry::Vacant(ve) => {
                self.letters_to_max_depth.insert(base_unit.letters.clone(), 0);
                ve.insert(base_unit);
                Ok(())
            },
        }
    }

    pub fn register_derived_unit(&mut self, derived_unit: DerivedUnit) -> Result<(), UnitDatabaseError> {
        if self.letters_to_base_unit.get(&derived_unit.letters).is_some() {
            return Err(UnitDatabaseError::ExistsAsBaseUnit);
        }

        match self.letters_to_derived_unit.entry(derived_unit.letters.clone()) {
            Entry::Occupied(oe) => {
                Err(UnitDatabaseError::ExistsAsDerivedUnit)
            },
            Entry::Vacant(ve) => {
                let mut parent_max_depth = 0;
                for (parent_letters, parent_power) in &derived_unit.parents {
                    match self.letters_to_max_depth.get(parent_letters) {
                        None => {
                            return Err(UnitDatabaseError::UnknownBaseUnit(parent_letters.clone()));
                        },
                        Some(d) => {
                            parent_max_depth = parent_max_depth.max(*d);
                        },
                    }
                }

                self.letters_to_max_depth.insert(derived_unit.letters.clone(), parent_max_depth + 1);
                ve.insert(derived_unit);
                Ok(())
            },
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

fn get_max_depth_of_units<'a, I: 'a + Iterator<Item = (&'a String, &'a BigInt)>>(units: I, database: &UnitDatabase) -> Option<usize> {
    let mut max_depth = 0;
    for (u_letters, _u_power) in units {
        match database.get_max_depth(&u_letters) {
            None => return None,
            Some(d) => {
                if max_depth < d {
                    max_depth = d;
                }
            },
        }
    }
    Some(max_depth)
}

pub(crate) fn expand_number_unit(num: &Number, unit_letters: &str, database: &UnitDatabase) -> Number {
    let unit = match database.get_derived_unit(unit_letters) {
        Some(u) => u,
        None => return num.clone(),
    };

    let new_value = num.value.checked_mul(NumberValue::Float(unit.factor_of_parents))
        .expect("multiplication failed");

    // update the units
    let mut new_units = NumberUnits::new();
    for (num_unit, num_pow) in &num.units {
        if let Some(parent_pow) = unit.parents.get(num_unit) {
            // number and parent are shared => add powers
            let new_pow = num_pow + parent_pow;
            if new_pow != BigInt::from(0) {
                new_units.insert(num_unit.clone(), new_pow);
            }
        } else {
            // only number contains this unit, not the parent => power from number
            new_units.insert(num_unit.clone(), num_pow.clone());
        }
    }
    for (parent_unit, parent_pow) in &unit.parents {
        if num.units.contains_key(parent_unit) {
            continue;
        }

        // only parent contains this unit, not the number => power from parent
        new_units.insert(parent_unit.clone(), parent_pow.clone());
    }

    // reduce original unit by 1
    let reduce_me = new_units.entry(unit_letters.to_owned())
        .or_insert(BigInt::from(0));
    *reduce_me -= 1;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_database() -> UnitDatabase {
        let mut db = UnitDatabase::new_empty();
        db.insert_canonical_si_prefixes();

        db.register_base_unit(BaseUnit::new("kg".to_owned())).unwrap();
        db.register_base_unit(BaseUnit::new("m".to_owned())).unwrap();
        db.register_base_unit(BaseUnit::new("s".to_owned())).unwrap();

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
}
