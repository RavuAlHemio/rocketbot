use std::collections::HashMap;

use num_bigint::BigUint;
use num_traits::Num;
use pest::Parser;
use pest::error::Error;
use pest::iterators::Pair;
use pest_derive::Parser;

use crate::grammar::{
    Alternative, Condition, Production, Rulebook, RuleDefinition, SequenceElement,
    SequenceElementCount, SingleSequenceElement,
};


#[derive(Parser)]
#[grammar = "grammar_gen_lang.pest"]
struct GrammarGenParser;


pub(crate) fn parse_grammar(text: &str) -> Result<Rulebook, Error<Rule>> {
    let pairs: Vec<Pair<'_, Rule>> = match GrammarGenParser::parse(Rule::ggrulebook, text) {
        Ok(p) => p,
        Err(e) => return Err(e),
    }.collect();

    assert_eq!(pairs.len(), 1);

    Ok(parse_rulebook(&pairs[0]))
}

fn parse_escaped_string(string_pair: &Pair<'_, Rule>) -> String {
    let mut inner = string_pair.clone().into_inner();
    let mut buf = String::new();

    let _open_quote_pair = inner.next().expect("no opening quote");

    while let Some(pair) = inner.next() {
        match pair.as_str() {
            "\"" => {
                // closing quote
                break;
            },
            "\\\"" => {
                // escaped quote
                buf.push('"');
            },
            "\\\\" => {
                // escaped backslash
                buf.push('\\')
            },
            other => {
                if other.starts_with("\\u") || other.starts_with("\\U") {
                    // Unicode escape
                    let hex_digits = &other[1..];
                    let unicode_value = u32::from_str_radix(hex_digits, 16)
                        .expect("failed to parse hex value");
                    let char_value = char::from_u32(unicode_value)
                        .expect("invalid character value");
                    buf.push(char_value);
                } else {
                    buf.push_str(other);
                }
            },
        }
    }

    buf
}

fn parse_identifier(ident_pair: &Pair<'_, Rule>) -> String {
    ident_pair.as_str().to_owned()
}

fn parse_number(number_pair: &Pair<'_, Rule>) -> BigUint {
    BigUint::from_str_radix(number_pair.as_str(), 10)
        .expect("failed to parse number")
}

fn parse_rulebook(rulebook_pair: &Pair<'_, Rule>) -> Rulebook {
    let inner = rulebook_pair.clone().into_inner();

    let mut rules: Vec<RuleDefinition> = inner
        .filter(|pair| pair.as_rule() == Rule::ruledef)
        .map(|pair| parse_ruledef(&pair))
        .collect();

    let mut rule_definitions = HashMap::new();
    for rule in rules.drain(..) {
        let rule_name = rule.name.clone();
        if let Some(rd) = rule_definitions.insert(rule_name, rule) {
            panic!("duplicate rule definition named {}", rd.name);
        }
    }
    Rulebook::new(rule_definitions)
}

fn parse_ruledef(ruledef_pair: &Pair<'_, Rule>) -> RuleDefinition {
    let mut inner = ruledef_pair.clone().into_inner();

    let def_pair = inner.next().expect("empty rule definition");
    match def_pair.as_rule() {
        Rule::ggrule => parse_rule(&def_pair),
        Rule::paramrule => parse_paramrule(&def_pair),
        other => panic!("unexpected rule definition type: {:?}", other),
    }
}

fn parse_paramrule(def_pair: &Pair<'_, Rule>) -> RuleDefinition {
    let mut inner = def_pair.clone().into_inner();

    let mut param_names = Vec::new();

    let identifier_pair = inner.next().expect("no identifier");
    let identifier = parse_identifier(&identifier_pair);

    let _open_brace_pair = inner.next().expect("no open brace");

    let arg_id_pair = inner.next().expect("no arg name identifier");
    param_names.push(parse_identifier(&arg_id_pair));

    loop {
        let arg_tail_pair = inner.next().expect("no comma or closing brace");
        match arg_tail_pair.as_str() {
            "," => {
                let arg_id_pair = inner.next().expect("no arg name identifier");
                param_names.push(parse_identifier(&arg_id_pair));
            },
            "}" => break,
            other => panic!("unexpected symbol: {:?}", other),
        };
    }

    let _colon_pair = inner.next().expect("no colon");

    let production_pair = inner.next().expect("no production");
    let production = parse_production(&production_pair);

    let _semicolon_pair = inner.next().expect("no semicolon");

    RuleDefinition::new(
        identifier,
        param_names,
        production,
    )
}

fn parse_rule(def_pair: &Pair<'_, Rule>) -> RuleDefinition {
    let mut inner = def_pair.clone().into_inner();

    let identifier_pair = inner.next().expect("no identifier");
    let identifier = parse_identifier(&identifier_pair);

    let _colon_pair = inner.next().expect("no colon");

    let production_pair = inner.next().expect("no production");
    let production = parse_production(&production_pair);

    let _semicolon_pair = inner.next().expect("no semicolon");

    RuleDefinition::new(
        identifier,
        Vec::new(),
        production,
    )
}

fn parse_production(prod_pair: &Pair<'_, Rule>) -> Production {
    let mut inner = prod_pair.clone().into_inner();
    let mut alternatives = Vec::new();

    let alternative_pair = inner.next().expect("no alternative");
    let alternative = parse_alternative(&alternative_pair);
    alternatives.push(alternative);

    while let Some(_comma_pair) = inner.next() {
        let alternative_pair = inner.next().expect("no alternative after comma");
        let alternative = parse_alternative(&alternative_pair);
        alternatives.push(alternative);
    }

    Production::new(alternatives)
}

fn parse_alternative(alt_pair: &Pair<'_, Rule>) -> Alternative {
    let mut inner = alt_pair.clone().into_inner();
    let mut conditions = Vec::new();
    let mut weight = BigUint::from(1u32);
    let mut sequence = Vec::new();

    while let Some(pair) = inner.next() {
        match pair.as_rule() {
            Rule::condition => {
                conditions.push(parse_condition(&pair));
            },
            Rule::weight => {
                weight = parse_weight(&pair);
            },
            Rule::sequence_elem => {
                sequence.push(parse_sequence_elem(&pair));
            },
            _ => {
                panic!("unexpected command {:?} in alternative", pair.as_rule());
            }
        }
    }

    Alternative::new(
        conditions,
        weight,
        sequence,
    )
}

fn parse_condition(cond_pair: &Pair<'_, Rule>) -> Condition {
    let mut inner = cond_pair.clone().into_inner();
    let mut negated = false;

    let _exclam_pair = inner.next().expect("no exclamation mark");

    let mut next_pair = inner.next().expect("no exclamation mark or identifier");
    if let Rule::negated = next_pair.as_rule() {
        negated = true;
        next_pair = inner.next().expect("no identifier");
    }

    let identifier = parse_identifier(&next_pair);

    Condition::new(
        negated,
        identifier,
    )
}

fn parse_weight(weight_pair: &Pair<'_, Rule>) -> BigUint {
    let mut inner = weight_pair.clone().into_inner();

    let _open_angle_pair = inner.next().expect("no opening angled bracket");

    let number_pair = inner.next().expect("no number");
    let number = parse_number(&number_pair);

    let _close_angle_pair = inner.next().expect("no closing angled bracket");

    number
}

fn parse_sequence_elem(seq_elem_pair: &Pair<'_, Rule>) -> SequenceElement {
    let mut inner = seq_elem_pair.clone().into_inner();

    let single_elem_pair = inner.next().expect("no single sequence element");
    let single_elem = parse_single_sequence_elem(&single_elem_pair);

    let mut count = SequenceElementCount::One;

    if let Some(kleene) = inner.next() {
        match kleene.as_str() {
            "*" => {
                count = SequenceElementCount::ZeroOrMore;
            },
            "+" => {
                count = SequenceElementCount::OneOrMore;
            },
            other => panic!("unexpected single-sequence count symbol: {}", other),
        };
    }

    SequenceElement::new(
        single_elem,
        count,
    )
}

fn parse_single_sequence_elem(sse_pair: &Pair<'_, Rule>) -> SingleSequenceElement {
    let mut inner = sse_pair.clone().into_inner();

    let elem_pair = inner.next().expect("no element");
    match elem_pair.as_rule() {
        Rule::parenthesized => {
            let mut innerer = elem_pair.clone().into_inner();

            let _open_pair = innerer.next().expect("no opening parenthesis");

            let production_pair = innerer.next().expect("no production");
            let production = parse_production(&production_pair);

            let _close_pair = innerer.next().expect("no closing parenthesis");

            SingleSequenceElement::Parenthesized {
                production,
            }
        },
        Rule::optional => {
            let mut innerer = elem_pair.clone().into_inner();

            let _open_pair = innerer.next().expect("no opening bracket");

            let mut next_pair = innerer.next().expect("no weight or production");

            let mut weight = BigUint::from(50u32);
            if let Rule::weight = next_pair.as_rule() {
                weight = parse_weight(&next_pair);

                next_pair = innerer.next().expect("no production");
            }

            let production = parse_production(&next_pair);

            let _close_pair = innerer.next().expect("no closing bracket");

            SingleSequenceElement::Optional {
                weight,
                production,
            }
        },
        Rule::call_params => {
            let mut innerer = elem_pair.clone().into_inner();
            let mut arguments = Vec::new();

            let identifier_pair = innerer.next().expect("no identifier");
            let identifier = parse_identifier(&identifier_pair);

            let _open_pair = innerer.next().expect("no opening brace");

            let arg_pair = innerer.next().expect("no argument production");
            let arg = parse_production(&arg_pair);
            arguments.push(arg);

            loop {
                let symbol_pair = innerer.next().expect("no symbol");
                match symbol_pair.as_str() {
                    "," => {
                        let arg_pair = innerer.next().expect("no argument production");
                        let arg = parse_production(&arg_pair);
                        arguments.push(arg);
                    },
                    "}" => {
                        break;
                    },
                    c => {
                        panic!("unexpected symbol in call: {}", c);
                    },
                }
            }

            SingleSequenceElement::Call {
                identifier,
                arguments,
            }
        },
        Rule::identifier => {
            let mut innerer = elem_pair.clone().into_inner();
            let arguments = Vec::new();

            let identifier_pair = innerer.next().expect("no identifier");
            let identifier = parse_identifier(&identifier_pair);

            SingleSequenceElement::Call {
                identifier,
                arguments,
            }
        },
        Rule::escaped_string => {
            let value = parse_escaped_string(&elem_pair);

            SingleSequenceElement::String {
                value,
            }
        },
        other => panic!("unexpected single sequence element: {:?}", other),
    }
}
