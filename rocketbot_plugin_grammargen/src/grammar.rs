use std::collections::{HashMap, HashSet};
use std::fmt::{self, Debug};
use std::sync::Arc;

use async_trait::async_trait;
use num_bigint::{BigUint, RandBigInt};
use num_traits::Zero;
use rand::Rng;
use rand::rngs::StdRng;
use tokio::sync::Mutex;


#[derive(Debug)]
pub struct GeneratorState {
    pub rulebook: Rulebook,
    pub conditions: HashSet<String>,
    pub rng: Arc<Mutex<StdRng>>,
}
impl GeneratorState {
    pub fn new(
        rulebook: Rulebook,
        conditions: HashSet<String>,
        rng: Arc<Mutex<StdRng>>,
    ) -> GeneratorState {
        GeneratorState {
            rulebook,
            conditions,
            rng,
        }
    }
}
impl Clone for GeneratorState {
    fn clone(&self) -> Self {
        let rulebook = self.rulebook.clone();
        let conditions = self.conditions.clone();
        let rng = Arc::clone(&self.rng);
        GeneratorState::new(
            rulebook,
            conditions,
            rng,
        )
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SoundnessError {
    UnresolvedReference(String),
    NoAlternatives,
    ArgumentCountMismatch(String, usize, usize),
}
impl fmt::Display for SoundnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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

#[async_trait]
pub trait TextGenerator : Debug + Sync + Send {
    async fn generate(&self, state: &GeneratorState) -> Option<String>;
    async fn verify_soundness(&self, state: &GeneratorState) -> Result<(), SoundnessError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rulebook {
    pub rule_definitions: HashMap<String, RuleDefinition>,
}
impl Rulebook {
    pub fn new(
        rule_definitions: HashMap<String, RuleDefinition>,
    ) -> Rulebook {
        Rulebook {
            rule_definitions,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuleDefinition {
    pub name: String,
    pub param_names: Vec<String>,
    pub top_production: Production,
}
impl RuleDefinition {
    pub fn new(
        name: String,
        param_names: Vec<String>,
        top_production: Production,
    ) -> RuleDefinition {
        RuleDefinition {
            name,
            param_names,
            top_production,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Production {
    pub alternatives: Vec<Alternative>,
}
impl Production {
    pub fn new(
        alternatives: Vec<Alternative>,
    ) -> Production {
        Production {
            alternatives,
        }
    }
}
#[async_trait]
impl TextGenerator for Production {
    async fn generate(&self, state: &GeneratorState) -> Option<String> {
        let my_alternatives: Vec<&Alternative> = self.alternatives
            .iter()
            .filter(|alt| alt.conditions.iter().all(|cond|
                state.conditions.contains(&cond.identifier) != cond.negated
            ))
            .collect();
        let total_weight: BigUint = my_alternatives
            .iter()
            .map(|alt| &alt.weight)
            .sum();

        if total_weight == Zero::zero() {
            // this branch has been "sawed off"
            return None;
        }

        let mut random_weight = {
            let mut rng_guard = state.rng
                .lock().await;
            rng_guard.gen_biguint_range(&Zero::zero(), &total_weight)
        };

        for alternative in my_alternatives {
            if random_weight >= alternative.weight {
                random_weight -= &alternative.weight;
                continue;
            }

            return alternative.generate(&state).await;
        }

        unreachable!();
    }

    async fn verify_soundness(&self, state: &GeneratorState) -> Result<(), SoundnessError> {
        let my_alternatives: Vec<&Alternative> = self.alternatives
            .iter()
            .filter(|alt| alt.conditions.iter().all(|cond|
                state.conditions.contains(&cond.identifier) != cond.negated
            ))
            .collect();
        if my_alternatives.len() == 0 {
            Err(SoundnessError::NoAlternatives)
        } else {
            for alt in my_alternatives {
                if let Err(e) = alt.verify_soundness(&state).await {
                    return Err(e);
                }
            }
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Alternative {
    pub conditions: Vec<Condition>,
    pub weight: BigUint,
    pub sequence: Vec<SequenceElement>,
}
impl Alternative {
    pub fn new(
        conditions: Vec<Condition>,
        weight: BigUint,
        sequence: Vec<SequenceElement>,
    ) -> Alternative {
        Alternative {
            conditions,
            weight,
            sequence,
        }
    }
}
#[async_trait]
impl TextGenerator for Alternative {
    async fn generate(&self, state: &GeneratorState) -> Option<String> {
        // weighting and conditioning is performed one level above (Production)
        let mut ret = String::new();
        for element in &self.sequence {
            let piece = match element.generate(&state).await {
                Some(s) => s,
                None => return None,
            };
            ret.push_str(&piece);
        }
        Some(ret)
    }

    async fn verify_soundness(&self, state: &GeneratorState) -> Result<(), SoundnessError> {
        for element in &self.sequence {
            if let Err(e) = element.verify_soundness(&state).await {
                return Err(e);
            }
        }
        Ok(())
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SequenceElementCount {
    One,
    OneOrMore,
    ZeroOrMore,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SequenceElement {
    pub element: SingleSequenceElement,
    pub count: SequenceElementCount,
}
impl SequenceElement {
    pub fn new(
        element: SingleSequenceElement,
        count: SequenceElementCount,
    ) -> SequenceElement {
        SequenceElement {
            element,
            count,
        }
    }
}
#[async_trait]
impl TextGenerator for SequenceElement {
    async fn generate(&self, state: &GeneratorState) -> Option<String> {
        if self.count == SequenceElementCount::One {
            return self.element.generate(&state).await;
        }

        let mut ret = String::new();

        if self.count == SequenceElementCount::OneOrMore {
            let element = match self.element.generate(&state).await {
                Some(s) => s,
                None => return None,
            };
            ret.push_str(&element);
        } else {
            assert_eq!(self.count, SequenceElementCount::ZeroOrMore);
        }

        {
            let mut rng_guard = state.rng
                .lock().await;
            loop {
                let rand_bool: bool = rng_guard.gen();
                if rand_bool {
                    break;
                }
                let element = match self.element.generate(&state).await {
                    Some(s) => s,
                    None => return None,
                };
                ret.push_str(&element);
            }
        }

        Some(ret)
    }

    async fn verify_soundness(&self, state: &GeneratorState) -> Result<(), SoundnessError> {
        self.element.verify_soundness(&state).await
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SingleSequenceElement {
    Parenthesized {
        production: Production,
     },
    Optional {
        weight: BigUint,
        production: Production,
    },
    Call {
        identifier: String,
        arguments: Vec<Production>,
    },
    String {
        value: String,
    },
}
impl SingleSequenceElement {
    pub fn new_parenthesized(
        production: Production,
    ) -> SingleSequenceElement {
        SingleSequenceElement::Parenthesized {
            production,
        }
    }

    pub fn new_optional(
        weight: BigUint,
        production: Production,
    ) -> SingleSequenceElement {
        SingleSequenceElement::Optional {
            weight,
            production,
        }
    }

    pub fn new_call(
        identifier: String,
        arguments: Vec<Production>,
    ) -> SingleSequenceElement {
        SingleSequenceElement::Call {
            identifier,
            arguments,
        }
    }

    pub fn new_string(
        value: String,
    ) -> SingleSequenceElement {
        SingleSequenceElement::String {
            value,
        }
    }
}
#[async_trait]
impl TextGenerator for SingleSequenceElement {
    async fn generate(&self, state: &GeneratorState) -> Option<String> {
        match self {
            SingleSequenceElement::Parenthesized { production } => {
                production.generate(&state).await
            },
            SingleSequenceElement::Optional { weight, production } => {
                let hundred = BigUint::from(100u8);

                let rand_val = {
                    let mut rng_guard = state.rng
                        .lock().await;
                    rng_guard.gen_biguint_range(&Zero::zero(), &hundred)
                };

                if &rand_val < weight {
                    production.generate(&state).await
                } else {
                    Some(String::new())
                }
            },
            SingleSequenceElement::Call { identifier, arguments } => {
                if let Some(rule) = state.rulebook.rule_definitions.get(identifier) {
                    assert_eq!(rule.param_names.len(), arguments.len());

                    // link up arguments with their values
                    let mut sub_state = state.clone();
                    for (param_name, arg) in rule.param_names.iter().zip(arguments.iter()) {
                        sub_state.rulebook.rule_definitions.insert(
                            param_name.clone(),
                            RuleDefinition::new(
                                param_name.clone(),
                                Vec::new(),
                                arg.clone(),
                            ),
                        );
                    }

                    // generate
                    rule.top_production.generate(&sub_state).await
                } else {
                    // call to undefined function
                    None
                }
            },
            SingleSequenceElement::String { value } => {
                // finally, something simple to generate :-D
                Some(value.clone())
            },
        }
    }

    async fn verify_soundness(&self, state: &GeneratorState) -> Result<(), SoundnessError> {
        match self {
            SingleSequenceElement::Parenthesized { production } => {
                production.verify_soundness(&state).await
            },
            SingleSequenceElement::Optional { production, .. } => {
                production.verify_soundness(&state).await
            },
            SingleSequenceElement::Call { identifier, arguments } => {
                if let Some(rule) = state.rulebook.rule_definitions.get(identifier) {
                    if rule.param_names.len() != arguments.len() {
                        return Err(SoundnessError::ArgumentCountMismatch(
                            identifier.clone(),
                            rule.param_names.len(),
                            arguments.len(),
                        ));
                    }

                    let mut sub_state = state.clone();
                    for (param_name, arg) in rule.param_names.iter().zip(arguments.iter()) {
                        sub_state.rulebook.rule_definitions.insert(
                            param_name.clone(),
                            RuleDefinition::new(
                                param_name.clone(),
                                Vec::new(),
                                arg.clone(),
                            ),
                        );
                    }

                    // recurse
                    rule.top_production.verify_soundness(&sub_state).await
                } else {
                    // rule definition not found
                    Err(SoundnessError::UnresolvedReference(identifier.clone()))
                }
            },
            SingleSequenceElement::String { .. } => {
                // constant strings are always sound
                Ok(())
            },
        }
    }
}
