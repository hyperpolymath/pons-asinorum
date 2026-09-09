// SPDX-License-Identifier: MPL-2.0
//! Contract validation and checking of explicit event traces.
//! This does not extract events from arbitrary source or implement a Python CFG.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Protocol {
    pub schema_version: String,
    pub id: String,
    pub description: String,
    pub channel: String,
    pub states: Vec<String>,
    pub initial: String,
    #[serde(default)]
    pub transition: Vec<Transition>,
    #[serde(default)]
    pub operation: Vec<Operation>,
    #[serde(default)]
    pub must_exit_in: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub on: String,
    pub from: String,
    pub to: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub on: String,
    pub kind: String,
    pub forbidden_in: Vec<String>,
    pub message: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub instance: String,
    pub call: String,
    pub line: usize,
}
#[derive(Debug, Serialize)]
pub struct Violation {
    pub protocol: String,
    pub instance: String,
    pub state: String,
    pub entered_at: usize,
    pub observed_at: usize,
    pub message: String,
}
impl Protocol {
    pub fn parse(text: &str) -> Result<Self> {
        let protocol: Self = toml::from_str(text)?;
        protocol.validate()?;
        Ok(protocol)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != "0.1.0" {
            bail!("unsupported protocol schema {}", self.schema_version);
        }
        if self.id.trim().is_empty()
            || self.channel.trim().is_empty()
            || self.description.trim().is_empty()
        {
            bail!("protocol id, channel and description must be non-empty");
        }
        let states: BTreeSet<_> = self.states.iter().collect();
        if states.len() != self.states.len()
            || states.iter().any(|s| s.trim().is_empty())
            || !states.contains(&self.initial)
        {
            bail!("states must be unique and contain initial");
        }
        let mut seen = BTreeSet::new();
        for t in &self.transition {
            if !states.contains(&t.from) || !states.contains(&t.to) {
                bail!("transition {} refers to an unknown state", t.on);
            }
            if t.on.trim().is_empty() || !seen.insert((&t.on, &t.from)) {
                bail!("transition ({}, {}) is empty or ambiguous", t.on, t.from);
            }
        }
        for op in &self.operation {
            if op.on.trim().is_empty()
                || op.message.trim().is_empty()
                || op.kind != "emit"
                || op.forbidden_in.is_empty()
                || op.forbidden_in.iter().any(|s| !states.contains(s))
            {
                bail!(
                    "invalid operation {}: expected kind=emit and known forbidden states",
                    op.on
                );
            }
        }
        if self.must_exit_in.iter().any(|s| !states.contains(s)) {
            bail!("unknown must_exit_in state");
        }
        if self.transition.is_empty() && self.operation.is_empty() {
            bail!("protocol contains no transitions or operations");
        }
        Ok(())
    }
    /// Check a supplied, ordered concrete trace; unknown calls are errors so a
    /// misspelt event cannot silently make an incomplete trace look correct.
    pub fn check_trace(&self, events: &[Event]) -> Result<Vec<Violation>> {
        self.validate()?;
        let mut instances: BTreeMap<&str, (&str, usize, bool)> = BTreeMap::new();
        let mut violations = vec![];
        let mut last = 0;
        for event in events {
            if event.instance.trim().is_empty() || event.line == 0 || event.line < last {
                bail!(
                    "trace needs non-empty instance keys and positive nondecreasing source lines"
                );
            }
            last = event.line;
            if !self.transition.iter().any(|t| t.on == event.call)
                && !self.operation.iter().any(|o| o.on == event.call)
            {
                bail!("unknown event call {} in protocol {}", event.call, self.id);
            }
            let entry =
                instances
                    .entry(&event.instance)
                    .or_insert((&self.initial, event.line, false));
            for op in self
                .operation
                .iter()
                .filter(|o| o.on == event.call && o.forbidden_in.iter().any(|s| s == entry.0))
            {
                violations.push(Violation {
                    protocol: self.id.clone(),
                    instance: event.instance.clone(),
                    state: entry.0.into(),
                    entered_at: entry.1,
                    observed_at: event.line,
                    message: op.message.clone(),
                });
            }
            if let Some(t) = self
                .transition
                .iter()
                .find(|t| t.on == event.call && t.from == entry.0)
            {
                *entry = (&t.to, event.line, entry.2 || t.to != self.initial);
            }
        }
        if !self.must_exit_in.is_empty() {
            for (instance, (state, line, moved)) in instances {
                if moved && !self.must_exit_in.iter().any(|s| s == state) {
                    violations.push(Violation {
                        protocol: self.id.clone(),
                        instance: instance.into(),
                        state: state.into(),
                        entered_at: line,
                        observed_at: last,
                        message: format!(
                            "Trace exits outside required states {:?}",
                            self.must_exit_in
                        ),
                    });
                }
            }
        }
        Ok(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const EGG: &str = include_str!("../../../protocols/linear-egg.toml");
    fn event(call: &str, line: usize) -> Event {
        Event {
            instance: "breakfast.egg".into(),
            call: call.into(),
            line,
        }
    }
    #[test]
    fn linear_egg_is_consumed_once() {
        let p = Protocol::parse(EGG).unwrap();
        assert!(
            p.check_trace(&[event("acquire", 1), event("eat", 2)])
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            p.check_trace(&[event("acquire", 1), event("eat", 2), event("eat", 3)])
                .unwrap()
                .len(),
            1
        );
        assert_eq!(p.check_trace(&[event("acquire", 1)]).unwrap().len(), 1);
        assert!(p.check_trace(&[event("eet", 1)]).is_err());
    }
    #[test]
    fn malformed_and_ambiguous_contracts_fail() {
        assert!(Protocol::parse(&EGG.replace("to = \"available\"", "to = \"unknown\"")).is_err());
        assert!(
            Protocol::parse(&format!(
                "{EGG}\n[[transition]]\non=\"eat\"\nfrom=\"available\"\nto=\"absent\"\n"
            ))
            .is_err()
        );
    }
    #[test]
    fn instances_are_independent() {
        let p = Protocol::parse(EGG).unwrap();
        let mut second = event("eat", 3);
        second.instance = "other.egg".into();
        assert_eq!(
            p.check_trace(&[event("acquire", 1), event("eat", 2), second])
                .unwrap()
                .len(),
            1
        );
    }
}
