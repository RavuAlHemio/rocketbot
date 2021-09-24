use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry as HashMapEntry;
use std::fmt::{self, Debug};
use std::sync::{Arc, Mutex};

use num_bigint::{BigUint, RandBigInt};
use num_traits::Zero;
use once_cell::sync::Lazy;
use rand::Rng;
use rand::rngs::StdRng;
use regex::{Captures, Regex};


static FIRST_LETTER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(
    "\\b\\pL"
).expect("failed to compile first-letter regex"));

const MAX_STACK_DEPTH: usize = 128;


#[derive(Debug)]
pub struct GeneratorState {
    pub rulebook: Rulebook,
    pub conditions: HashSet<String>,
    pub rng: Arc<Mutex<StdRng>>,
    pub memories: Arc<Mutex<HashMap<usize, Result<String, SoundnessError>>>>,
    pub prod_stack: Vec<usize>,
    pub regex_cache: Arc<Mutex<HashMap<String, Regex>>>,
    pub sound_productions: Arc<Mutex<HashSet<usize>>>,
}
impl GeneratorState {
    pub fn new(
        rulebook: Rulebook,
        conditions: HashSet<String>,
        rng: Arc<Mutex<StdRng>>,
        memories: Arc<Mutex<HashMap<usize, Result<String, SoundnessError>>>>,
        prod_stack: Vec<usize>,
        regex_cache: Arc<Mutex<HashMap<String, Regex>>>,
        sound_productions: Arc<Mutex<HashSet<usize>>>,
    ) -> GeneratorState {
        GeneratorState {
            rulebook,
            conditions,
            rng,
            memories,
            prod_stack,
            regex_cache,
            sound_productions,
        }
    }

    pub fn new_topmost(
        rulebook: Rulebook,
        conditions: HashSet<String>,
        rng: Arc<Mutex<StdRng>>,
    ) -> Self {
        Self::new(
            rulebook,
            conditions,
            rng,
            Arc::new(Mutex::new(HashMap::new())),
            Vec::new(),
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashSet::new())),
        )
    }

    pub fn verify_soundness(&mut self) -> Result<(), SoundnessError> {
        let top_rule = match self.rulebook.rule_definitions.get(&self.rulebook.name) {
            Some(tr) => tr.clone(),
            None => return Err(SoundnessError::TopRuleNotFound(self.rulebook.name.clone())),
        };
        top_rule.top_production.verify_soundness(self)
    }

    pub fn generate(&mut self) -> Result<String, SoundnessError> {
        let top_rule = match self.rulebook.rule_definitions.get(&self.rulebook.name) {
            Some(tr) => tr.clone(),
            None => return Err(SoundnessError::TopRuleNotFound(self.rulebook.name.clone())),
        };
        top_rule.top_production.generate(self)
    }

    pub fn get_or_compile_regex(&self, regex_str: &str) -> Result<Regex, SoundnessError> {
        let mut regex_guard = self.regex_cache
            .lock().expect("locking regex cache failed");
        match regex_guard.entry(regex_str.to_owned()) {
            HashMapEntry::Occupied(oe) => Ok(oe.get().clone()),
            HashMapEntry::Vacant(ve) => {
                let regex = match Regex::new(regex_str) {
                    Ok(r) => r,
                    Err(e) => return Err(SoundnessError::InvalidRegex {
                        regex_string: regex_str.to_owned(),
                        error: e,
                    }),
                };
                Ok(ve.insert(regex).clone())
            },
        }
    }
}
impl Clone for GeneratorState {
    fn clone(&self) -> Self {
        let rulebook = self.rulebook.clone();
        let conditions = self.conditions.clone();
        let rng = Arc::clone(&self.rng);
        let memories = Arc::clone(&self.memories);
        let prod_stack = self.prod_stack.clone();
        let regex_cache = Arc::clone(&self.regex_cache);
        let sound_productions = Arc::clone(&self.sound_productions);
        GeneratorState::new(
            rulebook,
            conditions,
            rng,
            memories,
            prod_stack,
            regex_cache,
            sound_productions,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SoundnessError {
    TopRuleNotFound(String),
    UnresolvedReference(String),
    NoAlternatives,
    ArgumentCountMismatch { target: String, expected: usize, obtained: usize },
    OperationArgumentCountMismatch { operation: InternalOperation, expected: usize, obtained: usize },
    MaxStackDepth { stack: Vec<usize> },
    InvalidRegex { regex_string: String, error: regex::Error },
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
            SoundnessError::ArgumentCountMismatch { target, expected, obtained }
                => write!(f, "call to production {:?} (which expects {} arguments) with {} arguments", target, expected, obtained),
            SoundnessError::OperationArgumentCountMismatch { operation, expected, obtained }
                => write!(f, "call to internal operation {:?} (which expects {} arguments) with {} arguments", operation, expected, obtained),
            SoundnessError::MaxStackDepth { stack }
                => write!(f, "maximum stack depth exceeded at stack: {:?}", stack),
            SoundnessError::InvalidRegex { regex_string, error }
                => write!(f, "invalid regex string {:?}: {}", regex_string, error),
        }
    }
}
impl std::error::Error for SoundnessError {
}

pub trait TextGenerator : Debug + Sync + Send {
    fn generate(&self, state: &mut GeneratorState) -> Result<String, SoundnessError>;
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
        let any_nick_production = Production::new(0, ProductionKind::Choice {
            options: nicks.iter()
                .map(|n| Alternative::new(
                    Vec::new(),
                    BigUint::from(1u32),
                    Production::new(0, ProductionKind::String { string: n.clone() }),
                ))
                .collect(),
        });

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
                    Production::new(0, ProductionKind::String { string: cn.to_owned() })
                } else {
                    any_nick_production
                },
                false,
            ),
        );

        let names_ops = &[
            ("__iop_uppercase", InternalOperation::Uppercase),
            ("__iop_lowercase", InternalOperation::Lowercase),
            ("__iop_title_case", InternalOperation::TitleCase),
            ("__iop_uppercase_first", InternalOperation::UppercaseFirst),
        ];
        for (name, op) in names_ops {
            self.rule_definitions.insert(
                (*name).to_owned(),
                RuleDefinition::new(
                    (*name).to_owned(),
                    vec!["str".to_owned()],
                    Production::new(0, ProductionKind::Operate {
                        operation: *op,
                        args: vec![
                            Production::new(0, ProductionKind::Call { name: "str".to_owned(), args: Vec::new() }),
                        ],
                    }),
                    false,
                ),
            );
        }

        let names_ops = &[
            ("__iop_regex_replace", InternalOperation::RegexReplace { all: false }),
            ("__iop_regex_replace_all", InternalOperation::RegexReplace { all: true }),
        ];
        for (name, op) in names_ops {
            self.rule_definitions.insert(
                (*name).to_owned(),
                RuleDefinition::new(
                    (*name).to_owned(),
                    vec!["regex".to_owned(), "subject".to_owned(), "replacement".to_owned()],
                    Production::new(0, ProductionKind::Operate {
                        operation: *op,
                        args: vec![
                            Production::new(0, ProductionKind::Call { name: "regex".to_owned(), args: Vec::new() }),
                            Production::new(0, ProductionKind::Call { name: "subject".to_owned(), args: Vec::new() }),
                            Production::new(0, ProductionKind::Call { name: "replacement".to_owned(), args: Vec::new() }),
                        ],
                    }),
                    false,
                ),
            );
        }

        self.rule_definitions.insert(
            "__iop_regex_if_then_else".to_owned(),
            RuleDefinition::new(
                "__iop_regex_if_then_else".to_owned(),
                vec!["subject".to_owned(), "if".to_owned(), "then".to_owned(), "else".to_owned()],
                Production::new(0, ProductionKind::Operate {
                    operation: InternalOperation::RegexIfThenElse,
                    args: vec![
                        Production::new(0, ProductionKind::Call { name: "subject".to_owned(), args: Vec::new() }),
                        Production::new(0, ProductionKind::Call { name: "if".to_owned(), args: Vec::new() }),
                        Production::new(0, ProductionKind::Call { name: "then".to_owned(), args: Vec::new() }),
                        Production::new(0, ProductionKind::Call { name: "else".to_owned(), args: Vec::new() }),
                    ],
                }),
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InternalOperation {
    Uppercase,
    Lowercase,
    UppercaseFirst,
    TitleCase,
    RegexReplace { all: bool },
    RegexIfThenElse,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ProductionKind {
    String { string: String },
    Sequence { prods: Vec<Production> },
    Choice { options: Vec<Alternative> },
    Optional { weight: BigUint, inner: Box<Production> },
    Kleene { at_least_one: bool, inner: Box<Production> },
    Call { name: String, args: Vec<Production> },
    Operate { operation: InternalOperation, args: Vec<Production> },
    VariableCall { name_production: Box<Production>, args: Vec<Production> },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Production {
    pub prod_id: usize,
    pub kind: ProductionKind,
}
impl Production {
    pub fn new(
        prod_id: usize,
        kind: ProductionKind,
    ) -> Self {
        Self {
            prod_id,
            kind,
        }
    }

    fn inner_generate(&self, state: &mut GeneratorState) -> Result<String, SoundnessError> {
        match &self.kind {
            ProductionKind::String { string } => Ok(string.clone()),
            ProductionKind::Sequence { prods } => {
                let mut ret = String::new();
                for prod in prods {
                    let piece = prod.generate(state)?;
                    ret.push_str(&piece);
                }
                Ok(ret)
            },
            ProductionKind::Choice { options } => {
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
                    return Err(SoundnessError::NoAlternatives);
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
            ProductionKind::Optional { weight, inner } => {
                let hundred = BigUint::from(100u8);

                let rand_val = {
                    let mut rng_guard = state.rng.lock().unwrap();
                    rng_guard.gen_biguint_range(&Zero::zero(), &hundred)
                };

                if &rand_val < weight {
                    inner.generate(state)
                } else {
                    Ok(String::new())
                }
            },
            ProductionKind::Kleene { at_least_one, inner } => {
                let mut ret = String::new();

                if *at_least_one {
                    let element = inner.generate(state)?;
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
                    let element = inner.generate(state)?;
                    ret.push_str(&element);
                }

                Ok(ret)
            },
            ProductionKind::Call { name, args } => {
                if let Some(rule) = state.rulebook.rule_definitions.get(name) {
                    if rule.param_names.len() != args.len() {
                        return Err(SoundnessError::ArgumentCountMismatch {
                            target: name.clone(),
                            expected: rule.param_names.len(),
                            obtained: args.len(),
                        });
                    }
                    assert_eq!(rule.param_names.len(), args.len());

                    if rule.memoize {
                        // have we generated this yet?
                        let memo_guard = state.memories.lock().unwrap();
                        if let Some(memoized) = memo_guard.get(&self.prod_id) {
                            // yup
                            return memoized.clone();
                        }
                    }

                    // generate each argument in turn
                    let mut arg_vals = Vec::with_capacity(args.len());
                    for arg in args {
                        let mut sub_state = state.clone();
                        let generated = arg.generate(&mut sub_state)?;
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
                                Production::new(0, ProductionKind::String { string: arg_val.clone() }),
                                false,
                            ),
                        );
                    }

                    // generate
                    let generated = rule.top_production.generate(&mut sub_state);

                    if rule.memoize {
                        // remember me
                        let mut memo_guard = state.memories.lock().unwrap();
                        memo_guard.insert(self.prod_id, generated.clone());
                    }

                    generated
                } else {
                    // call to undefined function
                    return Err(SoundnessError::UnresolvedReference(name.clone()));
                }
            },
            ProductionKind::Operate { operation, args } => {
                match operation {
                    InternalOperation::Uppercase|InternalOperation::Lowercase|InternalOperation::UppercaseFirst|InternalOperation::TitleCase => {
                        if args.len() != 1 {
                            // incorrect argument count
                            Err(SoundnessError::OperationArgumentCountMismatch {
                                operation: *operation,
                                expected: 1,
                                obtained: args.len(),
                            })
                        } else {
                            let mut sub_state = state.clone();
                            let generated = args[0].generate(&mut sub_state)?;
                            let operated = match operation {
                                InternalOperation::Uppercase => generated.to_uppercase(),
                                InternalOperation::Lowercase => generated.to_lowercase(),
                                InternalOperation::UppercaseFirst => {
                                    let chars: Vec<char> = generated.chars().collect();
                                    let mut upcased = String::new();
                                    if let Some(c) = chars.get(0) {
                                        for uc in c.to_uppercase() {
                                            upcased.push(uc);
                                        }
                                    }
                                    for c in chars.iter().skip(1) {
                                        upcased.push(*c);
                                    }
                                    upcased
                                },
                                InternalOperation::TitleCase => {
                                    FIRST_LETTER_RE.replace_all(&generated, |caps: &Captures| {
                                        caps
                                            .get(0).expect("capture group 0 not defined")
                                            .as_str()
                                            .to_uppercase()
                                    }).into_owned()
                                },
                                _ => unreachable!(),
                            };
                            Ok(operated)
                        }
                    },
                    InternalOperation::RegexReplace { all } => {
                        if args.len() != 3 {
                            // incorrect argument count
                            Err(SoundnessError::OperationArgumentCountMismatch {
                                operation: *operation,
                                expected: 3,
                                obtained: args.len(),
                            })
                        } else {
                            let mut generated_args = Vec::with_capacity(args.len());
                            for arg in args {
                                let mut sub_state = state.clone();
                                let generated = arg.generate(&mut sub_state)?;
                                generated_args.push(generated);
                            }

                            let regex = state.get_or_compile_regex(&generated_args[0])?;
                            let subject = &generated_args[1];
                            let replacement = &generated_args[2];

                            let replaced = if *all {
                                regex.replace_all(subject, replacement)
                            } else {
                                regex.replace(subject, replacement)
                            }.into_owned();
                            Ok(replaced)
                        }
                    },
                    InternalOperation::RegexIfThenElse => {
                        if args.len() != 4 {
                            // incorrect argument count
                            Err(SoundnessError::OperationArgumentCountMismatch {
                                operation: *operation,
                                expected: 4,
                                obtained: args.len(),
                            })
                        } else {
                            let generated_regex_string = {
                                let mut sub_state = state.clone();
                                args[0].generate(&mut sub_state)?
                            };
                            let subject = {
                                let mut sub_state = state.clone();
                                args[1].generate(&mut sub_state)?
                            };

                            let regex = state.get_or_compile_regex(&generated_regex_string)?;
                            let mut sub_state = state.clone();
                            if regex.is_match(&subject) {
                                args[2].generate(&mut sub_state)
                            } else {
                                args[3].generate(&mut sub_state)
                            }
                        }
                    },
                }
            },
            ProductionKind::VariableCall { name_production, args } => {
                let name_opt = {
                    let mut sub_state = state.clone();
                    name_production.generate(&mut sub_state)
                };
                let name = name_opt?;

                // use standard call process
                let call_prod = Production::new(self.prod_id, ProductionKind::Call {
                    name,
                    args: args.clone(),
                });
                call_prod.generate(state)
            },
        }
    }

    fn inner_verify_soundness(&self, state: &mut GeneratorState) -> Result<(), SoundnessError> {
        match &self.kind {
            ProductionKind::String { string: _ } => Ok(()),
            ProductionKind::Sequence { prods } => {
                for prod in prods {
                    if let Err(e) = prod.verify_soundness(state) {
                        return Err(e);
                    }
                }
                Ok(())
            },
            ProductionKind::Choice { options } => {
                let my_alternatives: Vec<&Alternative> = options
                    .iter()
                    .filter(|alt| alt.conditions.iter().all(|cond|
                        state.conditions.contains(&cond.identifier) != cond.negated
                    ))
                    .collect();
                if my_alternatives.len() == 0 {
                    Err(SoundnessError::NoAlternatives)
                } else {
                    let mut max_stack_overflow = Some(Vec::new());
                    for alt in my_alternatives {
                        match alt.verify_soundness(state) {
                            Err(SoundnessError::MaxStackDepth { stack: stk }) => {
                                // if at least one alternative does _not_ overflow the stack, we're fine
                                // it's better to initially assume they all do, though
                                // return the one that overflows it the most :-)
                                if let Some(mso) = &mut max_stack_overflow {
                                    if mso.len() < stk.len() {
                                        *mso = stk;
                                    }
                                }
                            },
                            Err(e) => {
                                // return this error directly
                                return Err(e);
                            },
                            Ok(()) => {
                                // if at least one option does not overflow the stack, we're fine
                                max_stack_overflow = None;
                            },
                        }
                    }

                    if let Some(mso) = max_stack_overflow {
                        // they really did all overflow
                        Err(SoundnessError::MaxStackDepth { stack: mso })
                    } else {
                        Ok(())
                    }
                }
            },
            ProductionKind::Optional { weight:_, inner } => {
                inner.verify_soundness(state)
            },
            ProductionKind::Kleene { at_least_one: _, inner } => {
                inner.verify_soundness(state)
            },
            ProductionKind::Call { name, args } => {
                if let Some(rule) = state.rulebook.rule_definitions.get(name) {
                    if rule.param_names.len() != args.len() {
                        return Err(SoundnessError::ArgumentCountMismatch {
                            target: name.clone(),
                            expected: rule.param_names.len(),
                            obtained: args.len(),
                        });
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
                                Production::new(0, ProductionKind::String { string: String::new() }),
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
            ProductionKind::Operate { operation, args } => {
                match operation {
                    InternalOperation::Uppercase|InternalOperation::Lowercase|InternalOperation::UppercaseFirst|InternalOperation::TitleCase => {
                        if args.len() != 1 {
                            Err(SoundnessError::OperationArgumentCountMismatch { operation: *operation, expected: 1, obtained: args.len() })
                        } else {
                            args[0].verify_soundness(state)
                        }
                    },
                    InternalOperation::RegexReplace { all: _ } => {
                        if args.len() != 3 {
                            Err(SoundnessError::OperationArgumentCountMismatch { operation: *operation, expected: 3, obtained: args.len() })
                        } else {
                            args[0].verify_soundness(state)?;
                            args[1].verify_soundness(state)?;
                            args[2].verify_soundness(state)
                        }
                    },
                    InternalOperation::RegexIfThenElse => {
                        if args.len() != 4 {
                            Err(SoundnessError::OperationArgumentCountMismatch { operation: *operation, expected: 4, obtained: args.len() })
                        } else {
                            args[0].verify_soundness(state)?;
                            args[1].verify_soundness(state)?;
                            args[2].verify_soundness(state)?;
                            args[3].verify_soundness(state)
                        }
                    },
                }
            },
            ProductionKind::VariableCall { name_production, args } => {
                // can't really check whether the name exists...

                {
                    let mut sub_state = state.clone();
                    name_production.verify_soundness(&mut sub_state)?;
                }

                for arg in args {
                    let mut sub_state = state.clone();
                    arg.verify_soundness(&mut sub_state)?;
                }

                Ok(())
            }
        }
    }
}
impl TextGenerator for Production {
    fn generate(&self, state: &mut GeneratorState) -> Result<String, SoundnessError> {
        state.prod_stack.push(self.prod_id);
        if state.prod_stack.len() > MAX_STACK_DEPTH {
            return Err(SoundnessError::MaxStackDepth { stack: state.prod_stack.clone() });
        }

        let generated_res = self.inner_generate(state);

        state.prod_stack.pop();

        generated_res
    }

    fn verify_soundness(&self, state: &mut GeneratorState) -> Result<(), SoundnessError> {
        state.prod_stack.push(self.prod_id);
        if state.prod_stack.len() > MAX_STACK_DEPTH {
            return Err(SoundnessError::MaxStackDepth { stack: state.prod_stack.clone() });
        }

        // are we already verified?
        if self.prod_id != 0 {
            let sound_prod_guard = state.sound_productions
                .lock().expect("failed to lock set of sound productions");
            if sound_prod_guard.contains(&self.prod_id) {
                // yes; don't verify us again
                state.prod_stack.pop();
                return Ok(());
            }
        }

        let result = self.inner_verify_soundness(state);

        if let Ok(_) = result {
            // mark us as verified
            state.sound_productions
                .lock().expect("failed to lock set of sound productions")
                .insert(self.prod_id);
        }

        state.prod_stack.pop();

        result
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
    fn generate(&self, state: &mut GeneratorState) -> Result<String, SoundnessError> {
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
