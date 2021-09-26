use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry as HashMapEntry;
use std::fmt::{self, Debug};
use std::io::BufRead;

use num_bigint::{BigUint, RandBigInt};
use once_cell::sync::Lazy;
use rand::Rng;
use rand::rngs::StdRng;
use regex::{Captures, Regex};


static FIRST_LETTER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(
    "\\b\\pL"
).expect("failed to compile first-letter regex"));


pub type ProductionIndex = usize;


#[derive(Debug)]
pub struct StateStackEntry {
    pub production: ProductionIndex,
    pub rulebook: Option<Rulebook>,
    pub args: Vec<String>,
}

#[derive(Debug)]
pub struct GeneratorState {
    pub stack: Vec<StateStackEntry>,
    pub conditions: HashSet<String>,
    pub return_value: Option<String>,
    pub rng: StdRng,
    pub memories: HashMap<ProductionIndex, String>,
    pub regex_cache: HashMap<String, Regex>,
    pub sound_productions: HashSet<usize>,
    pub previous_alternative: HashMap<ProductionIndex, usize>,
}
impl GeneratorState {
    pub fn new_topmost(
        rulebook: Rulebook,
        start_production: usize,
        conditions: HashSet<String>,
        rng: StdRng,
    ) -> Self {
        let initial_stack_entry = StateStackEntry {
            production: start_production,
            rulebook: Some(rulebook),
            args: Vec::new(),
        };

        Self {
            stack: vec![initial_stack_entry],
            conditions,
            return_value: None,
            rng,
            memories: HashMap::new(),
            regex_cache: HashMap::new(),
            sound_productions: HashSet::new(),
            previous_alternative: HashMap::new(),
        }
    }

    pub fn prepare_again(&mut self, rulebook: Rulebook, start_production: usize) {
        let initial_stack_entry = StateStackEntry {
            production: start_production,
            rulebook: Some(rulebook),
            args: Vec::new(),
        };
        self.stack.clear();
        self.stack.push(initial_stack_entry);
        self.return_value = None;
        self.memories.clear();
        self.sound_productions.clear();
        self.previous_alternative.clear();
    }

    pub fn rulebook<'a>(&'a self, current_stack_entry: &'a StateStackEntry) -> &'a Rulebook {
        if let Some(rb) = &current_stack_entry.rulebook {
            return rb;
        }

        self.stack
            .iter()
            .rev()
            .filter_map(|sse| sse.rulebook.as_ref())
            .nth(0)
            .expect("stack is empty")
    }

    pub fn get_or_compile_regex(&mut self, regex_str: &str) -> Result<Regex, SoundnessError> {
        match self.regex_cache.entry(regex_str.to_owned()) {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rulebook {
    pub name: String,
    pub productions: Vec<Production>,
    pub rule_definitions: HashMap<String, RuleDefinition>,
    pub metacommands: Vec<Metacommand>,
}
impl Rulebook {
    pub fn new(
        name: String,
        productions: Vec<Production>,
        rule_definitions: HashMap<String, RuleDefinition>,
        metacommands: Vec<Metacommand>,
    ) -> Rulebook {
        Rulebook {
            name,
            productions,
            rule_definitions,
            metacommands,
        }
    }

    pub fn add_production(&mut self, kind: ProductionKind) -> ProductionIndex {
        let prod_id = self.productions.len();
        self.productions.push(Production::new(prod_id, kind));
        prod_id
    }

    pub fn add_builtins(&mut self, nicks: &HashSet<String>, chosen_nick: Option<&str>) {
        let mut any_nick_alternatives: Vec<Alternative> = Vec::new();
        for nick in nicks {
            let inner_id = self.add_production(ProductionKind::String { string: nick.clone() });

            any_nick_alternatives.push(Alternative::new(
                Vec::new(),
                BigUint::from(1u32),
                inner_id,
            ));
        }
        let any_nick_id = self.add_production(ProductionKind::Choice { options: any_nick_alternatives });

        self.rule_definitions.insert(
            "__IRC_nick".to_owned(),
            RuleDefinition::new(
                "__IRC_nick".to_owned(),
                Vec::new(),
                any_nick_id,
                false,
            ),
        );

        let chosen_nick_prod = if let Some(cn) = chosen_nick {
            self.add_production(ProductionKind::String { string: cn.to_owned() })
        } else {
            any_nick_id
        };
        self.rule_definitions.insert(
            "__IRC_chosen_nick".to_owned(),
            RuleDefinition::new(
                "__IRC_chosen_nick".to_owned(),
                Vec::new(),
                chosen_nick_prod,
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
            let str_arg_prod_id = self.add_production(ProductionKind::Call { name: "str".to_owned(), args: Vec::new() });
            let op_prod_id = self.add_production(ProductionKind::Operate { operation: *op, args: vec![str_arg_prod_id] });

            self.rule_definitions.insert(
                (*name).to_owned(),
                RuleDefinition::new(
                    (*name).to_owned(),
                    vec!["str".to_owned()],
                    op_prod_id,
                    false,
                ),
            );
        }

        let names_ops = &[
            ("__iop_regex_replace", InternalOperation::RegexReplace { all: false }),
            ("__iop_regex_replace_all", InternalOperation::RegexReplace { all: true }),
        ];
        for (name, op) in names_ops {
            let regex_arg_prod_id = self.add_production(ProductionKind::Call { name: "regex".to_owned(), args: Vec::new() });
            let subject_arg_prod_id = self.add_production(ProductionKind::Call { name: "subject".to_owned(), args: Vec::new() });
            let replacement_arg_prod_id = self.add_production(ProductionKind::Call { name: "replacement".to_owned(), args: Vec::new() });

            let op_prod_id = self.add_production(ProductionKind::Operate { operation: *op, args: vec![
                regex_arg_prod_id,
                subject_arg_prod_id,
                replacement_arg_prod_id,
            ] });

            self.rule_definitions.insert(
                (*name).to_owned(),
                RuleDefinition::new(
                    (*name).to_owned(),
                    vec!["regex".to_owned(), "subject".to_owned(), "replacement".to_owned()],
                    op_prod_id,
                    false,
                ),
            );
        }

        let subject_arg_prod_id = self.add_production(ProductionKind::Call { name: "subject".to_owned(), args: Vec::new() });
        let if_arg_prod_id = self.add_production(ProductionKind::Call { name: "if".to_owned(), args: Vec::new() });
        let then_arg_prod_id = self.add_production(ProductionKind::Call { name: "then".to_owned(), args: Vec::new() });
        let else_arg_prod_id = self.add_production(ProductionKind::Call { name: "else".to_owned(), args: Vec::new() });

        let op_prod_id = self.add_production(ProductionKind::Operate { operation: InternalOperation::RegexIfThenElse, args: vec![
            subject_arg_prod_id,
            if_arg_prod_id,
            then_arg_prod_id,
            else_arg_prod_id,
        ] });

        self.rule_definitions.insert(
            "__iop_regex_if_then_else".to_owned(),
            RuleDefinition::new(
                "__iop_regex_if_then_else".to_owned(),
                vec!["subject".to_owned(), "if".to_owned(), "then".to_owned(), "else".to_owned()],
                op_prod_id,
                false,
            ),
        );
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuleDefinition {
    pub name: String,
    pub param_names: Vec<String>,
    pub top_production: ProductionIndex,
    pub memoize: bool,
}
impl RuleDefinition {
    pub fn new(
        name: String,
        param_names: Vec<String>,
        top_production: ProductionIndex,
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
impl InternalOperation {
    pub fn arg_count(&self) -> usize {
        match self {
            Self::Uppercase|Self::Lowercase|Self::UppercaseFirst|Self::TitleCase => 1,
            Self::RegexReplace { .. } => 3,
            Self::RegexIfThenElse => 4,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ProductionKind {
    String { string: String },
    Sequence { prods: Vec<ProductionIndex> },
    Choice { options: Vec<Alternative> },
    Optional { weight: BigUint, inner: ProductionIndex },
    Kleene { at_least_one: bool, inner: ProductionIndex },
    Call { name: String, args: Vec<ProductionIndex> },
    Operate { operation: InternalOperation, args: Vec<ProductionIndex> },
    VariableCall { name_production: ProductionIndex, args: Vec<ProductionIndex> },
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
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Alternative {
    pub conditions: Vec<Condition>,
    pub weight: BigUint,
    pub inner: ProductionIndex,
}
impl Alternative {
    pub fn new(
        conditions: Vec<Condition>,
        weight: BigUint,
        inner: ProductionIndex,
    ) -> Alternative {
        Alternative {
            conditions,
            weight,
            inner,
        }
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


pub fn generate(state: &mut GeneratorState) -> Result<String, SoundnessError> {
    while let Some(state_entry) = state.stack.pop() {
        generate_one_prod(state, state_entry)?;
    }

    // return the current return value
    Ok(state.return_value.clone().unwrap())
}

pub fn generate_step_by_step(state: &mut GeneratorState) -> Result<String, SoundnessError> {
    while let Some(state_entry) = state.stack.pop() {
        let prods: Vec<Production> = state.stack.iter()
            .map(|p| state.rulebook(&state_entry).productions[p.production].clone())
            .collect();
        println!("STACK: [");
        for prod in prods {
            println!("  {:?}", prod);
        }
        println!("]");
        println!("POPPED: {:?}", state.rulebook(&state_entry).productions[state_entry.production]);
        println!("hit Enter to keep going");

        {
            let stdin = std::io::stdin();
            let mut buf = String::new();
            stdin.lock().read_line(&mut buf).unwrap();
        }

        generate_one_prod(state, state_entry)?;
    }

    Ok(state.return_value.clone().unwrap())
}

fn generate_one_prod(state: &mut GeneratorState, mut state_entry: StateStackEntry) -> Result<(), SoundnessError> {
    let prod = state.rulebook(&state_entry).productions[state_entry.production].clone();
    match &prod.kind {
        ProductionKind::String { string } => {
            state.return_value = Some(string.clone());
        },
        ProductionKind::Sequence { prods } => {
            if let Some(rv) = &state.return_value {
                // a child is returning a value
                state_entry.args.push(rv.clone());
                state.return_value = None;
            }

            if state_entry.args.len() == prods.len() {
                // we have collected everything
                state.return_value = Some(state_entry.args.join(""));
            } else {
                // we need another element
                let next_prod = prods[state_entry.args.len()];

                // make sure it all returns to us
                state.stack.push(state_entry);

                // add the next production
                state.stack.push(StateStackEntry {
                    rulebook: None,
                    production: next_prod,
                    args: vec![],
                });
            }
        },
        ProductionKind::Choice { options } => {
            // this is a self-replacing call; clear the return value
            state.return_value = None;

            let mut my_alternatives: Vec<(usize, &Alternative)> = options
                .iter()
                .enumerate()
                .filter(|(_i, alt)| alt.conditions.iter().all(|c|
                    state.conditions.contains(&c.identifier) != c.negated
                ))
                .collect();
            if my_alternatives.len() == 0 {
                return Err(SoundnessError::NoAlternatives);
            } else if my_alternatives.len() == 1 {
                // the hardest decision
                state.stack.push(StateStackEntry {
                    rulebook: state_entry.rulebook,
                    production: my_alternatives[0].1.inner,
                    args: vec![],
                });
            } else {
                if let Some(pa) = state.previous_alternative.get(&state_entry.production) {
                    // don't generate the same thing like last time
                    my_alternatives.retain(|(i, _alt)| i != pa);
                }

                let total_weight: BigUint = my_alternatives.iter()
                    .map(|(_i, alt)| &alt.weight)
                    .sum();
                let mut random_weight = state.rng.gen_biguint_below(&total_weight);
                for (i, alt) in my_alternatives {
                    if random_weight >= alt.weight {
                        random_weight -= &alt.weight;
                        continue;
                    }

                    // this is the chosen one

                    // make sure we don't call it again too soon
                    state.previous_alternative.insert(state_entry.production, i);

                    // replace ourselves
                    state.stack.push(StateStackEntry {
                        rulebook: state_entry.rulebook.clone(),
                        production: alt.inner,
                        args: vec![],
                    });
                    break;
                }
            }
        },
        ProductionKind::Optional { weight, inner } => {
            let hundred = BigUint::from(100u8);
            let rand_val = state.rng.gen_biguint_below(&hundred);
            if &rand_val < weight {
                // this is a self-replacing call; clear the return value
                state.return_value = None;

                // generate the inner value
                state.stack.push(StateStackEntry {
                    rulebook: state_entry.rulebook,
                    production: *inner,
                    args: vec![],
                });
            } else {
                // don't generate the inner value
                state.return_value = Some(String::new());
            }
        },
        ProductionKind::Kleene { at_least_one, inner } => {
            if let Some(rv) = &state.return_value {
                state_entry.args.push(rv.clone());
                state.return_value = None;
            }

            let generate = if *at_least_one && state_entry.args.len() == 0 {
                true
            } else {
                state.rng.gen()
            };

            if generate {
                // make sure this returns to us
                state.stack.push(state_entry);

                // child!
                state.stack.push(StateStackEntry {
                    rulebook: None,
                    production: *inner,
                    args: vec![],
                });
            } else {
                // return
                state.return_value = Some(state_entry.args.join(""));
            }
        },
        ProductionKind::Call { name, args } => {
            if let Some(rv) = &state.return_value {
                if state_entry.args.len() == args.len() {
                    // we are returning from the sub-call to memoize the result
                    let rule = match state.rulebook(&state_entry).rule_definitions.get(name) {
                        Some(r) => r.clone(),
                        None => return Err(SoundnessError::UnresolvedReference(name.clone())),
                    };

                    if rule.memoize {
                        state.memories.insert(prod.prod_id, rv.clone());
                    }

                    // pass the return value
                    return Ok(());
                } else {
                    // we have obtained another argument
                    state_entry.args.push(rv.clone());
                    state.return_value = None;
                }
            }

            if state_entry.args.len() == 0 {
                let rule = match state.rulebook(&state_entry).rule_definitions.get(name) {
                    Some(r) => r.clone(),
                    None => return Err(SoundnessError::UnresolvedReference(name.clone())),
                };

                if rule.param_names.len() != args.len() {
                    // called with an incorrect number of arguments
                    return Err(SoundnessError::ArgumentCountMismatch {
                        target: rule.name.clone(),
                        expected: rule.param_names.len(),
                        obtained: args.len(),
                    });
                }

                if rule.memoize {
                    // we should reuse the previous result
                    if let Some(res) = state.memories.get(&prod.prod_id) {
                        // and there is one
                        state.return_value = Some(res.clone());
                        return Ok(());
                    }
                }
            }

            if state_entry.args.len() == args.len() {
                // we have collected all args
                let mut sub_rulebook = state.rulebook(&state_entry).clone();
                let rule = sub_rulebook.rule_definitions.get(name)
                    .expect("failed to find rule")
                    .clone();
                // we already verified the arg count before...
                assert_eq!(args.len(), rule.param_names.len());

                // map the arguments into the new rulebook
                for (param_name, arg) in rule.param_names.iter().zip(state_entry.args.iter()) {
                    let param_prod_id = sub_rulebook.add_production(ProductionKind::String { string: arg.clone() });
                    sub_rulebook.rule_definitions.insert(
                        param_name.clone(),
                        RuleDefinition::new(
                            param_name.clone(),
                            vec![],
                            param_prod_id,
                            false,
                        )
                    );
                }

                // remember ourselves (for memoization)
                state.stack.push(state_entry);

                // replace us with the call
                state.stack.push(StateStackEntry {
                    rulebook: Some(sub_rulebook),
                    production: rule.top_production,
                    args: vec![],
                });
            } else {
                // we need another argument
                let arg_prod = args[state_entry.args.len()];

                // remember ourselves
                state.stack.push(state_entry);

                // generate the next argument
                state.stack.push(StateStackEntry {
                    rulebook: None,
                    production: arg_prod,
                    args: vec![],
                });
            }
        },
        ProductionKind::Operate { operation, args } => {
            if let Some(rv) = &state.return_value {
                state_entry.args.push(rv.clone());
                state.return_value = None;
            }

            if args.len() != operation.arg_count() {
                return Err(SoundnessError::OperationArgumentCountMismatch {
                    operation: *operation,
                    expected: operation.arg_count(),
                    obtained: args.len(),
                });
            }

            // special-case this one
            if let InternalOperation::RegexIfThenElse = operation {
                if state_entry.args.len() == 2 {
                    // make this decision now, before collecting further arguments
                    let regex = state.get_or_compile_regex(&state_entry.args[0])
                        .expect("invalid regex");
                    let subject = &state_entry.args[1];

                    let replacement_arg_index = if regex.is_match(&subject) {
                        2
                    } else {
                        3
                    };

                    // replace myself
                    state.stack.push(StateStackEntry {
                        rulebook: state_entry.rulebook,
                        production: args[replacement_arg_index],
                        args: vec![],
                    });

                    return Ok(());
                }
            }

            if state_entry.args.len() == args.len() {
                match operation {
                    InternalOperation::Uppercase|InternalOperation::Lowercase|InternalOperation::UppercaseFirst|InternalOperation::TitleCase => {
                        state.return_value = Some(match operation {
                            InternalOperation::Uppercase => state_entry.args[0].to_uppercase(),
                            InternalOperation::Lowercase => state_entry.args[0].to_lowercase(),
                            InternalOperation::UppercaseFirst => {
                                let chars: Vec<char> = state_entry.args[0].chars().collect();
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
                                FIRST_LETTER_RE.replace_all(&state_entry.args[0], |caps: &Captures| {
                                    caps
                                        .get(0).expect("capture group 0 not defined")
                                        .as_str()
                                        .to_uppercase()
                                }).into_owned()
                            },
                            _ => unreachable!(),
                        });
                    },
                    InternalOperation::RegexReplace { all } => {
                        let regex = state.get_or_compile_regex(&state_entry.args[0])?;
                        let subject = &state_entry.args[1];
                        let replacement = &state_entry.args[2];

                        let replaced = if *all {
                            regex.replace_all(subject, replacement)
                        } else {
                            regex.replace(subject, replacement)
                        }.into_owned();
                        state.return_value = Some(replaced);
                    },
                    InternalOperation::RegexIfThenElse => {
                        // this shouldn't happen; we should have handled this before
                        unreachable!();
                    },
                }
            } else {
                // we need another argument
                let arg_prod = args[state_entry.args.len()];
                state.stack.push(state_entry);
                state.stack.push(StateStackEntry {
                    rulebook: None,
                    production: arg_prod,
                    args: vec![],
                });
            }
        },
        ProductionKind::VariableCall { name_production, args } => {
            if let Some(rv) = &state.return_value {
                state_entry.args.push(rv.clone());
                state.return_value = None;
            }

            if state_entry.args.len() == 1 {
                // we can replace ourselves with a call now
                let name = state_entry.args.remove(0);
                let call_args: Vec<usize> = args.iter().skip(1).map(|a| *a).collect();

                let mut sub_rulebook = state.rulebook(&state_entry).clone();
                let call_prod_id = sub_rulebook.add_production(ProductionKind::Call { name, args: call_args });
                state.stack.push(StateStackEntry {
                    rulebook: Some(sub_rulebook),
                    production: call_prod_id,
                    args: vec![],
                })
            } else if state_entry.args.len() == 0 {
                // generate the name
                state.stack.push(state_entry);
                state.stack.push(StateStackEntry {
                    rulebook: None,
                    production: *name_production,
                    args: vec![],
                });
            } else {
                // this shouldn't happen (we should have only generated one argument)
                unreachable!();
            }
        },
    };

    Ok(())
}


pub fn verify(state: &mut GeneratorState) -> Result<(), SoundnessError> {
    while let Some(state_entry) = state.stack.pop() {
        if state.sound_productions.contains(&state_entry.production) {
            continue;
        }

        let prod = state.rulebook(&state_entry).productions[state_entry.production].clone();
        match &prod.kind {
            ProductionKind::String { .. } => {
                // constants are always sound
            },
            ProductionKind::Sequence { prods } => {
                // just bosh the elements on the stack
                if let Some(p) = prods.get(0) {
                    state.stack.push(StateStackEntry {
                        rulebook: state_entry.rulebook,
                        production: *p,
                        args: vec![],
                    });
                }
                for p in &prods[1..] {
                    state.stack.push(StateStackEntry {
                        rulebook: None,
                        production: *p,
                        args: vec![],
                    });
                }
            },
            ProductionKind::Choice { options } => {
                // bosh the relevant options on the stack
                let my_options: Vec<&Alternative> = options.iter()
                    .filter(|alt| alt.conditions.iter().all(|c| state.conditions.contains(&c.identifier) != c.negated))
                    .collect();
                if my_options.len() == 0 {
                    return Err(SoundnessError::NoAlternatives);
                }
                state.stack.push(StateStackEntry {
                    rulebook: state_entry.rulebook,
                    production: my_options[0].inner,
                    args: vec![],
                });
                for option in my_options {
                    state.stack.push(StateStackEntry {
                        rulebook: None,
                        production: option.inner,
                        args: vec![],
                    });
                }
            },
            ProductionKind::Optional { weight: _, inner } => {
                // verify the inner production
                state.stack.push(StateStackEntry {
                    rulebook: state_entry.rulebook,
                    production: *inner,
                    args: vec![],
                });
            },
            ProductionKind::Kleene { at_least_one: _, inner } => {
                // verify the inner production
                state.stack.push(StateStackEntry {
                    rulebook: state_entry.rulebook,
                    production: *inner,
                    args: vec![],
                });
            },
            ProductionKind::Call { name, args } => {
                let (sub_rulebook, top_production) = {
                    let target_rule_opt = state.rulebook(&state_entry)
                        .rule_definitions.get(name);
                    let target_rule = match target_rule_opt {
                        Some(tr) => tr,
                        None => return Err(SoundnessError::UnresolvedReference(name.clone())),
                    };

                    // verify this call with a bunch of empty strings
                    let mut sub_rulebook = state.rulebook(&state_entry).clone();
                    let empty_prod = sub_rulebook.add_production(ProductionKind::String { string: String::new() });
                    for arg in &target_rule.param_names {
                        sub_rulebook.rule_definitions.insert(
                            arg.clone(),
                            RuleDefinition::new(
                                arg.clone(),
                                Vec::new(),
                                empty_prod,
                                false,
                            ),
                        );
                    }

                    (sub_rulebook, target_rule.top_production)
                };
                state.stack.push(StateStackEntry {
                    rulebook: Some(sub_rulebook),
                    production: top_production,
                    args: vec![],
                });

                // verify each argument
                for arg in args {
                    state.stack.push(StateStackEntry {
                        rulebook: None,
                        production: *arg,
                        args: vec![],
                    });
                }
            },
            ProductionKind::Operate { operation, args } => {
                if operation.arg_count() != args.len() {
                    return Err(SoundnessError::OperationArgumentCountMismatch {
                        operation: *operation,
                        expected: operation.arg_count(),
                        obtained: args.len(),
                    });
                }

                // verify each argument
                for arg in args {
                    state.stack.push(StateStackEntry {
                        rulebook: None,
                        production: *arg,
                        args: vec![],
                    });
                }
            },
            ProductionKind::VariableCall { name_production, args } => {
                // verify the name and each argument
                state.stack.push(StateStackEntry {
                    rulebook: state_entry.rulebook,
                    production: *name_production,
                    args: vec![],
                });
                for arg in args {
                    state.stack.push(StateStackEntry {
                        rulebook: None,
                        production: *arg,
                        args: vec![],
                    });
                }
            },
        }

        // at least this one didn't fail
        state.sound_productions.insert(state_entry.production);
    }

    Ok(())
}
