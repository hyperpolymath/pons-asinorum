// SPDX-License-Identifier: MPL-2.0
//! Model conformance only. Passing this corpus does not assert extraction from
//! the corresponding application repository or interpreter implementation.
use pons_protocols::{Event, Protocol};
#[test]
fn portfolio_protocols_have_positive_corrected_and_safe_traces() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/protocol-models");
    for entry in std::fs::read_dir(root).unwrap() {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let p =
            Protocol::parse(&std::fs::read_to_string(dir.join("contract.toml")).unwrap()).unwrap();
        for side in ["positive", "fixed", "safe"] {
            let events: Vec<Event> = serde_json::from_str(
                &std::fs::read_to_string(dir.join(format!("{side}.json"))).unwrap(),
            )
            .unwrap();
            let violations = p.check_trace(&events).unwrap();
            assert_eq!(violations.is_empty(), side != "positive", "{} {side}", p.id);
        }
        assert!(p.check_trace(&[]).is_err());
    }
}
