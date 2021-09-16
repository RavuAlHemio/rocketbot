use std::collections::{HashMap, HashSet};
use std::fmt::{self, Debug};
use std::sync::{Arc, Mutex};

use num_bigint::{BigUint, RandBigInt};
use num_traits::Zero;
use rand::Rng;
use rand::rngs::StdRng;


#[derive(Debug)]
pub struct GeneratorState {
    pub rulebook: Rulebook,
    pub conditions: HashSet<String>,
    pub rng: Arc<Mutex<StdRng>>,
    pub memories: Arc<Mutex<HashMap<usize, Option<String>>>>,
}
impl GeneratorState {
    pub fn new(
        rulebook: Rulebook,
        conditions: HashSet<String>,
        rng: Arc<Mutex<StdRng>>,
        memories: Arc<Mutex<HashMap<usize, Option<String>>>>,
    ) -> GeneratorState {
        GeneratorState {
            rulebook,
            conditions,
            rng,
            memories,
        }
    }

    pub fn verify_soundness(&mut self) -> Result<(), SoundnessError> {
        let top_rule = match self.rulebook.rule_definitions.get(&self.rulebook.name) {
            Some(tr) => tr.clone(),
            None => return Err(SoundnessError::TopRuleNotFound(self.rulebook.name.clone())),
        };
        top_rule.top_production.verify_soundness(self)
    }

    pub fn generate(&mut self) -> Option<String> {
        let top_rule = match self.rulebook.rule_definitions.get(&self.rulebook.name) {
            Some(tr) => tr.clone(),
            None => return None,
        };
        top_rule.top_production.generate(self)
    }
}
impl Clone for GeneratorState {
    fn clone(&self) -> Self {
        let rulebook = self.rulebook.clone();
        let conditions = self.conditions.clone();
        let rng = Arc::clone(&self.rng);
        let memories = Arc::clone(&self.memories);
        GeneratorState::new(
            rulebook,
            conditions,
            rng,
            memories,
        )
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SoundnessError {
    TopRuleNotFound(String),
    UnresolvedReference(String),
    NoAlternatives,
    ArgumentCountMismatch(String, usize, usize),
}
impl fmt::Display for SoundnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SoundnessError::TopRuleNotFound(identifier)
                => write!(f, "top rule {:?} not found", identifier),
            SoundnessError::UnresolvedReference(identifier)
                => write!(f, "unresolved reference to {:?}", identifier),
            SoundnessError::NoAlternatives
                => write!(f, "production is left with zero alternatives"),
            SoundnessError::ArgumentCountMismatch(production, expected_args, got_args)
                => write!(f, "call to production {:?} (which expects {} arguments) with {} arguments", production, expected_args, got_args),
        }
    }
}
impl std::error::Error for SoundnessError {
}

pub trait TextGenerator : Debug + Sync + Send {
    fn generate(&self, state: &mut GeneratorState) -> Option<String>;
    fn verify_soundness(&self, state: &mut GeneratorState) -> Result<(), SoundnessError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rulebook {
    pub name: String,
    pub rule_definitions: HashMap<String, RuleDefinition>,
    pub metacommands: Vec<Metacommand>,
}
impl Rulebook {
    pub fn new(
        name: String,
        rule_definitions: HashMap<String, RuleDefinition>,
        metacommands: Vec<Metacommand>,
    ) -> Rulebook {
        Rulebook {
            name,
            rule_definitions,
            metacommands,
        }
    }

    pub fn add_builtins(&mut self, nicks: &HashSet<String>, chosen_nick: Option<&str>) {
        let any_nick_production = Production::Choice {
            options: nicks.iter()
                .map(|n| Alternative::new(
                    Vec::new(),
                    BigUint::from(1u32),
                    Production::String { string: n.clone() },
                ))
                .collect(),
        };

        self.rule_definitions.insert(
            "__IRC_nick".to_owned(),
            RuleDefinition::new(
                "__IRC_nick".to_owned(),
                Vec::new(),
                any_nick_production.clone(),
                false,
            ),
        );

        self.rule_definitions.insert(
            "__IRC_chosen_nick".to_owned(),
            RuleDefinition::new(
                "__IRC_chosen_nick".to_owned(),
                Vec::new(),
                if let Some(cn) = chosen_nick {
                    Production::String { string: cn.to_owned() }
                } else {
                    any_nick_production
                },
                false,
            ),
        );
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuleDefinition {
    pub name: String,
    pub param_names: Vec<String>,
    pub top_production: Production,
    pub memoize: bool,
}
impl RuleDefinition {
    pub fn new(
        name: String,
        param_names: Vec<String>,
        top_production: Production,
        memoize: bool,
    ) -> RuleDefinition {
        RuleDefinition {
            name,
            param_names,
            top_production,
            memoize,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Metacommand {
    RandomizeCondition(String),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Production {
    String { string: String },
    Sequence { prods: Vec<Production> },
    Choice { options: Vec<Alternative> },
    Optional { weight: BigUint, inner: Box<Production> },
    Kleene { at_least_one: bool, inner: Box<Production> },
    Call { name: String, args: Vec<Production>, call_site_id: usize },
}
impl TextGenerator for Production {
    fn generate(&self, state: &mut GeneratorState) -> Option<String> {
        match self {
            Production::String { string } => Some(string.clone()),
            Production::Sequence { prods } => {
                let mut ret = String::new();
                for prod in prods {
                    let piece = match prod.generate(state) {
                        Some(s) => s,
                        None => return None,
                    };
                    ret.push_str(&piece);
                }
                Some(ret)
            },
            Production::Choice { options } => {
                let my_alternatives: Vec<&Alternative> = options
                    .iter()
                    .filter(|alt| alt.conditions.iter().all(|cond|
                        state.conditions.contains(&cond.identifier) != cond.negated
                    ))
                    .collect();
                if my_alternatives.len() == 1 {
                    // fast-path
                    return my_alternatives[0].generate(state);
                }

                let total_weight: BigUint = my_alternatives
                    .iter()
                    .map(|alt| &alt.weight)
                    .sum();

                if total_weight == Zero::zero() {
                    // this branch has been "sawed off"
                    return None;
                }

                let mut random_weight = {
                    let mut rng_guard = state.rng.lock().unwrap();
                    rng_guard.gen_biguint_range(&Zero::zero(), &total_weight)
                };

                for alternative in my_alternatives {
                    if random_weight >= alternative.weight {
                        random_weight -= &alternative.weight;
                        continue;
                    }

                    return alternative.generate(state)
                }

                unreachable!();
            },
            Production::Optional { weight, inner } => {
                let hundred = BigUint::from(100u8);

                let rand_val = {
                    let mut rng_guard = state.rng.lock().unwrap();
                    rng_guard.gen_biguint_range(&Zero::zero(), &hundred)
                };

                if &rand_val < weight {
                    inner.generate(state)
                } else {
                    Some(String::new())
                }
            },
            Production::Kleene { at_least_one, inner } => {
                let mut ret = String::new();

                if *at_least_one {
                    let element = match inner.generate(state) {
                        Some(s) => s,
                        None => return None,
                    };
                    ret.push_str(&element);
                }

                loop {
                    let rand_bool: bool = {
                        let mut rng_guard = state.rng.lock().unwrap();
                        rng_guard.gen()
                    };
                    if rand_bool {
                        break;
                    }
                    let element = match inner.generate(state) {
                        Some(s) => s,
                        None => return None,
                    };
                    ret.push_str(&element);
                }

                Some(ret)
            },
            Production::Call { name, args, call_site_id } => {
                if let Some(rule) = state.rulebook.rule_definitions.get(name) {
                    assert_eq!(rule.param_names.len(), args.len());

                    if rule.memoize {
                        // have we generated this yet?
                        let memo_guard = state.memories.lock().unwrap();
                        if let Some(memoized) = memo_guard.get(call_site_id) {
                            // yup
                            return memoized.clone();
                        }
                    }

                    // generate each argument in turn
                    let mut arg_vals = Vec::with_capacity(args.len());
                    for arg in args {
                        let mut sub_state = state.clone();
                        let generated = match arg.generate(&mut sub_state) {
                            None => return None,
                            Some(g) => g,
                        };
                        arg_vals.push(generated);
                    }

                    // link up arguments with their values
                    let mut sub_state = state.clone();
                    for (param_name, arg_val) in rule.param_names.iter().zip(arg_vals.iter()) {
                        sub_state.rulebook.rule_definitions.insert(
                            param_name.clone(),
                            RuleDefinition::new(
                                param_name.clone(),
                                Vec::new(),
                                Production::String { string: arg_val.clone() },
                                false,
                            ),
                        );
                    }

                    // generate
                    let generated = rule.top_production.generate(&mut sub_state);

                    if rule.memoize {
                        // remember me
                        let mut memo_guard = state.memories.lock().unwrap();
                        memo_guard.insert(*call_site_id, generated.clone());
                    }

                    generated
                } else {
                    // call to undefined function
                    None
                }
            },
        }
    }

    fn verify_soundness(&self, state: &mut GeneratorState) -> Result<(), SoundnessError> {
        match self {
            Production::String { string: _ } => Ok(()),
            Production::Sequence { prods } => {
                for prod in prods {
                    if let Err(e) = prod.verify_soundness(state) {
                        return Err(e);
                    }
                }
                Ok(())
            },
            Production::Choice { options } => {
                let my_alternatives: Vec<&Alternative> = options
                    .iter()
                    .filter(|alt| alt.conditions.iter().all(|cond|
                        state.conditions.contains(&cond.identifier) != cond.negated
                    ))
                    .collect();
                if my_alternatives.len() == 0 {
                    Err(SoundnessError::NoAlternatives)
                } else {
                    for alt in my_alternatives {
                        if let Err(e) = alt.verify_soundness(state) {
                            return Err(e);
                        }
                    }
                    Ok(())
                }
            },
            Production::Optional { weight:_, inner } => {
                inner.verify_soundness(state)
            },
            Production::Kleene { at_least_one: _, inner } => {
                inner.verify_soundness(state)
            },
            Production::Call { name, args, call_site_id: _ } => {
                if let Some(rule) = state.rulebook.rule_definitions.get(name) {
                    if rule.param_names.len() != args.len() {
                        return Err(SoundnessError::ArgumentCountMismatch(
                            name.clone(),
                            rule.param_names.len(),
                            args.len(),
                        ));
                    }

                    // validate each argument
                    for arg in args {
                        let mut sub_state = state.clone();
                        arg.verify_soundness(&mut sub_state)?;
                    }

                    // validate this call, substituting empty strings for each argument
                    let mut sub_state = state.clone();
                    for param_name in &rule.param_names {
                        sub_state.rulebook.rule_definitions.insert(
                            param_name.clone(),
                            RuleDefinition::new(
                                param_name.clone(),
                                Vec::new(),
                                Production::String { string: String::new() },
                                false,
                            ),
                        );
                    }

                    // recurse
                    rule.top_production.verify_soundness(&mut sub_state)
                } else {
                    // rule definition not found
                    Err(SoundnessError::UnresolvedReference(name.clone()))
                }
            },
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Alternative {
    pub conditions: Vec<Condition>,
    pub weight: BigUint,
    pub inner: Production,
}
impl Alternative {
    pub fn new(
        conditions: Vec<Condition>,
        weight: BigUint,
        inner: Production,
    ) -> Alternative {
        Alternative {
            conditions,
            weight,
            inner,
        }
    }
}
impl TextGenerator for Alternative {
    fn generate(&self, state: &mut GeneratorState) -> Option<String> {
        // weighting and conditioning is performed one level above (Production)
        self.inner.generate(state)
    }

    fn verify_soundness(&self, state: &mut GeneratorState) -> Result<(), SoundnessError> {
        self.inner.verify_soundness(state)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Condition {
    pub negated: bool,
    pub identifier: String,
}
impl Condition {
    pub fn new(
        negated: bool,
        identifier: String,
    ) -> Condition {
        Condition {
            negated,
            identifier,
        }
    }
}
