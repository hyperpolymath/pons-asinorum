// SPDX-License-Identifier: MPL-2.0
//! Executable reference model for supplied Wharf events, not source extraction
//! or a cryptographic verifier. Boolean evidence is a producer assertion.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Action {
    Authenticate {
        key_present: bool,
        nonce: String,
        expires_at: u64,
    },
    Bind {
        artifact: String,
        policy: String,
        root_present: bool,
        signature_required: bool,
    },
    Transfer {
        artifact: String,
    },
    Verify {
        artifact: String,
        signature_valid: bool,
        root_valid: bool,
    },
    Commit {
        artifact: String,
        durable: bool,
    },
    Rollback,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub session: String,
    pub now: u64,
    pub action: Action,
}
#[derive(Debug, Serialize)]
pub struct Violation {
    pub event: usize,
    pub session: String,
    pub reason: String,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Authenticated,
    Bound,
    Transferred,
    Verified,
    Committed,
    RolledBack,
}
struct Session {
    state: State,
    expires: u64,
    artifact: String,
    policy: String,
}
/// Every denied event leaves state unchanged. Time must be nondecreasing;
/// replay protection is limited to nonces visible in this supplied trace.
pub fn check(events: &[Event]) -> Result<Vec<Violation>> {
    if events.is_empty() || events.len() > 100_000 {
        bail!("Wharf trace needs 1..100000 events");
    }
    let mut sessions: BTreeMap<String, Session> = BTreeMap::new();
    let mut nonces = BTreeSet::new();
    let mut violations = Vec::new();
    let mut last = 0;
    for (index, event) in events.iter().enumerate() {
        if event.session.trim().is_empty() || event.now < last {
            bail!("invalid session/time at event {index}");
        }
        last = event.now;
        let result: Result<(), String> = (|| {
            if let Action::Authenticate {
                key_present,
                nonce,
                expires_at,
            } = &event.action
            {
                if !key_present || nonce.trim().is_empty() || *expires_at <= event.now {
                    return Err("absent key/nonce or expired authentication".into());
                }
                if sessions.contains_key(&event.session) || nonces.contains(nonce) {
                    return Err("replayed session/nonce".into());
                }
                nonces.insert(nonce.clone());
                sessions.insert(
                    event.session.clone(),
                    Session {
                        state: State::Authenticated,
                        expires: *expires_at,
                        artifact: String::new(),
                        policy: String::new(),
                    },
                );
                return Ok(());
            }
            let s = sessions
                .get_mut(&event.session)
                .ok_or("unauthenticated session")?;
            if event.now >= s.expires {
                return Err("session expired".into());
            }
            match &event.action {
                Action::Bind {
                    artifact,
                    policy,
                    root_present,
                    signature_required,
                } => {
                    if s.state != State::Authenticated
                        || artifact.trim().is_empty()
                        || policy.trim().is_empty()
                        || !root_present
                        || !signature_required
                    {
                        return Err(
                            "binding requires artifact, policy, root and mandatory signature"
                                .into(),
                        );
                    }
                    s.artifact = artifact.clone();
                    s.policy = policy.clone();
                    s.state = State::Bound;
                }
                Action::Transfer { artifact } => {
                    if s.state != State::Bound || artifact != &s.artifact {
                        return Err("transfer is unbound or cross-session artifact".into());
                    }
                    s.state = State::Transferred;
                }
                Action::Verify {
                    artifact,
                    signature_valid,
                    root_valid,
                } => {
                    if s.state != State::Transferred
                        || artifact != &s.artifact
                        || !signature_valid
                        || !root_valid
                    {
                        return Err("verification requires bound transferred artifact, valid signature and root".into());
                    }
                    s.state = State::Verified;
                }
                Action::Commit { artifact, durable } => {
                    if s.state != State::Verified || artifact != &s.artifact || !durable {
                        return Err(
                            "commit requires matching verified artifact and durable application"
                                .into(),
                        );
                    }
                    s.state = State::Committed;
                }
                Action::Rollback => {
                    if matches!(s.state, State::Committed | State::RolledBack) {
                        return Err("terminal session cannot roll back".into());
                    }
                    s.state = State::RolledBack;
                }
                Action::Authenticate { .. } => unreachable!(),
            }
            Ok(())
        })();
        if let Err(reason) = result {
            violations.push(Violation {
                event: index + 1,
                session: event.session.clone(),
                reason,
            });
        }
    }
    for (session, s) in sessions {
        if !matches!(s.state, State::Committed | State::RolledBack) {
            violations.push(Violation {
                event: events.len(),
                session,
                reason: "trace ends without commit/rollback".into(),
            });
        }
    }
    Ok(violations)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn happy() -> Vec<Event> {
        vec![
            Action::Authenticate {
                key_present: true,
                nonce: "n1".into(),
                expires_at: 100,
            },
            Action::Bind {
                artifact: "a1".into(),
                policy: "p1".into(),
                root_present: true,
                signature_required: true,
            },
            Action::Transfer {
                artifact: "a1".into(),
            },
            Action::Verify {
                artifact: "a1".into(),
                signature_valid: true,
                root_valid: true,
            },
            Action::Commit {
                artifact: "a1".into(),
                durable: true,
            },
        ]
        .into_iter()
        .enumerate()
        .map(|(i, action)| Event {
            session: "s1".into(),
            now: i as u64 + 1,
            action,
        })
        .collect()
    }
    #[test]
    fn complete_and_rollback_traces_are_accepted() {
        assert!(check(&happy()).unwrap().is_empty());
        let mut t = happy();
        t.truncate(3);
        t.push(Event {
            session: "s1".into(),
            now: 9,
            action: Action::Rollback,
        });
        assert!(check(&t).unwrap().is_empty());
    }
    #[test]
    fn denied_transition_matrix() {
        let mutations = vec![
            (
                0,
                Action::Authenticate {
                    key_present: false,
                    nonce: "n1".into(),
                    expires_at: 100,
                },
            ),
            (
                1,
                Action::Bind {
                    artifact: "a1".into(),
                    policy: "p1".into(),
                    root_present: false,
                    signature_required: true,
                },
            ),
            (
                1,
                Action::Bind {
                    artifact: "a1".into(),
                    policy: "p1".into(),
                    root_present: true,
                    signature_required: false,
                },
            ),
            (
                2,
                Action::Transfer {
                    artifact: "other-session-artifact".into(),
                },
            ),
            (
                3,
                Action::Verify {
                    artifact: "a1".into(),
                    signature_valid: false,
                    root_valid: true,
                },
            ),
            (
                3,
                Action::Verify {
                    artifact: "a1".into(),
                    signature_valid: true,
                    root_valid: false,
                },
            ),
            (
                3,
                Action::Commit {
                    artifact: "a1".into(),
                    durable: true,
                },
            ),
            (
                4,
                Action::Commit {
                    artifact: "a1".into(),
                    durable: false,
                },
            ),
        ];
        for (index, action) in mutations {
            let mut t = happy();
            t[index].action = action;
            assert!(!check(&t).unwrap().is_empty(), "mutant {index}");
        }
    }
    #[test]
    fn replay_expiry_and_cross_session_fail() {
        let mut t = happy();
        let mut replay = t[0].clone();
        replay.now = 6;
        replay.session = "s2".into();
        t.push(replay);
        assert!(!check(&t).unwrap().is_empty());
        let mut t = happy();
        t[4].now = 100;
        assert!(!check(&t).unwrap().is_empty());
        let mut t = happy();
        t[3].session = "other".into();
        assert!(!check(&t).unwrap().is_empty());
        assert!(check(&[]).is_err());
    }
}
