//! The arguments, parsed by hand from the contract's command table: which
//! flags stand alone and which short spellings exist both come from
//! `cli::COMMANDS` and `cli::GLOBAL_FLAGS`.
//!
//!   jot status abc123 resolved -m "Fixed in #42" --profile office
//!   jot profile office status abc123 resolved      (the same thing)

use std::collections::BTreeMap;

use crate::contract::cli::{self as c, copy::messages};
use crate::contract::Flag;
use crate::text::fill;
use crate::{CliError, Result};

/// A flag as given: on (a boolean flag) or with its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlagValue {
    On,
    Value(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    /// The command and its operands, flags removed: ["status", "abc123", "resolved"].
    pub positionals: Vec<String>,
    pub flags: BTreeMap<String, FlagValue>,
}

impl Parsed {
    /// True when a boolean flag was given.
    pub fn on(&self, name: &str) -> bool {
        self.flags.get(name) == Some(&FlagValue::On)
    }

    /// A flag's value, when it was given one.
    pub fn value(&self, name: &str) -> Option<&str> {
        match self.flags.get(name) {
            Some(FlagValue::Value(value)) => Some(value),
            _ => None,
        }
    }
}

/// Every flag the contract names, global ones first.
fn all_flags() -> impl Iterator<Item = &'static Flag> {
    c::GLOBAL_FLAGS.iter().chain(c::COMMANDS.iter().flat_map(|command| command.flags.iter()))
}

/// A flag stands alone when the contract gives it no value placeholder.
fn is_boolean(name: &str) -> bool {
    all_flags().any(|flag| flag.name == name && flag.value.is_none())
}

/// `m` → `message`, `p` → `profile`, …
fn long_name(short: &str) -> Option<&'static str> {
    all_flags().find(|flag| flag.short == Some(short)).map(|flag| flag.name)
}

pub fn parse(argv: &[String]) -> Result<Parsed> {
    let mut parsed = Parsed::default();

    // 1. split words into flags and operands; `--name=value` and `--name value` both work
    let mut i = 0;
    while i < argv.len() {
        let word = &argv[i];
        i += 1;
        if !word.starts_with('-') || word == "-" {
            parsed.positionals.push(word.clone());
            continue;
        }
        let bare = word.strip_prefix("--").or_else(|| word.strip_prefix('-')).unwrap_or(word);
        let (raw, inline) = match bare.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (bare, None),
        };
        let name = long_name(raw).unwrap_or(raw).to_string();
        if is_boolean(&name) {
            parsed.flags.insert(name, FlagValue::On);
            continue;
        }
        let value = match inline {
            Some(value) => value,
            None => {
                let next = argv.get(i).cloned();
                i += 1;
                next.ok_or_else(|| CliError(fill(messages::FLAG_NEEDS_VALUE, &[("name", &name)])))?
            }
        };
        parsed.flags.insert(name, FlagValue::Value(value));
    }

    // 2. `profile <name> <command…>` is `--profile <name> <command…>`
    if parsed.positionals.first().map(String::as_str) == Some("profile") {
        if parsed.positionals.len() < 3 || parsed.positionals[1].is_empty() {
            return Err(CliError(messages::PROFILE_COMMAND_USAGE.to_string()));
        }
        let name = parsed.positionals[1].clone();
        parsed.flags.insert("profile".to_string(), FlagValue::Value(name));
        parsed.positionals.drain(0..2);
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::tests::{conformance, error_text};
    use serde_json::Value;

    #[test]
    fn conformance_args() {
        let cases = conformance()["args"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let argv: Vec<String> =
                case["argv"].as_array().unwrap().iter().map(|word| word.as_str().unwrap().to_string()).collect();
            let result = parse(&argv);

            // an expected refusal: the contract message, filled
            if let Some(error) = case.get("error") {
                assert_eq!(result.unwrap_err().0, error_text(error), "{name}");
                continue;
            }
            let parsed = result.unwrap_or_else(|error| panic!("{name}: {error}"));
            let expect = &case["expect"];
            let positionals: Vec<&str> =
                expect["positionals"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(parsed.positionals, positionals, "{name}");
            let flags: BTreeMap<String, FlagValue> = expect["flags"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| {
                    let flag = match value {
                        Value::Bool(true) => FlagValue::On,
                        other => FlagValue::Value(other.as_str().unwrap().to_string()),
                    };
                    (key.clone(), flag)
                })
                .collect();
            assert_eq!(parsed.flags, flags, "{name}");
        }
    }
}
