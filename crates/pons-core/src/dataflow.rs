// SPDX-License-Identifier: MPL-2.0

//! The two T1 analyses over a [`FunctionCfg`], transcribed from ADR-0002:
//!
//! ```text
//! LiveOut(B) = ⋃ { LiveIn(S) : S ∈ succ(B) }
//! LiveIn(B)  = use(B) ∪ ( LiveOut(B) \ def(B) )
//!
//! Unbound_in(entry) = Locals \ Params
//! Unbound_in(B)     = ⋃ { Unbound_out(P) : P ∈ pred(B) }
//! ```
//!
//! Both are powersets of a finite set of locals with union as the meet, so
//! the worklist terminates without a widening operator. Do not add one.
//!
//! Facts are solved per block, then replayed per statement to recover the fact
//! at each event.
//!
//! One source location can appear in several blocks, because the builder
//! copies each `finally` once per way out. The queries at the bottom combine
//! those copies: a store is dead only if it is dead in *every* reachable copy,
//! and a read is may-unbound if it is in *any*.

use std::collections::{BTreeMap, VecDeque};

use crate::cfg::{BlockId, DefReport, Event, EventKind, FunctionCfg, LocalId};

/// A set of locals, as a bitset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSet {
    words: Vec<u64>,
}

impl LocalSet {
    /// An empty set for `n` locals, with capacity rounded up to 64-bit words.
    pub fn empty(n: usize) -> Self {
        Self {
            words: vec![0; n.div_ceil(64)],
        }
    }

    /// Contains exactly the local IDs in `0..n`; any padding bits stay clear.
    pub fn full(n: usize) -> Self {
        let mut s = Self::empty(n);
        for id in 0..n {
            s.insert(id);
        }
        s
    }

    /// Tests membership, including IDs in the allocated padding.
    ///
    /// # Panics
    /// Panics if `id / 64` is outside the allocated words.
    pub fn contains(&self, id: LocalId) -> bool {
        self.words[id / 64] & (1 << (id % 64)) != 0
    }

    /// Adds a local without growing the set's capacity.
    ///
    /// # Panics
    /// Panics if `id / 64` is outside the allocated words.
    pub fn insert(&mut self, id: LocalId) {
        self.words[id / 64] |= 1 << (id % 64);
    }

    /// Clears a local's membership; an absent local is unchanged.
    ///
    /// # Panics
    /// Panics if `id / 64` is outside the allocated words.
    pub fn remove(&mut self, id: LocalId) {
        self.words[id / 64] &= !(1 << (id % 64));
    }

    /// `self ∪= other`; true if `self` grew.
    /// Only overlapping words are merged if capacities differ; `self` is
    /// never resized and its remaining words are unchanged.
    pub fn union_with(&mut self, other: &LocalSet) -> bool {
        let mut changed = false;
        for (w, o) in self.words.iter_mut().zip(&other.words) {
            let merged = *w | o;
            changed |= merged != *w;
            *w = merged;
        }
        changed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

/// A gen/kill problem over [`LocalSet`] with union meet.
pub trait Problem {
    const DIRECTION: Direction;
    /// The fact at `ENTRY`'s start (forward) or `EXIT`'s end (backward).
    fn boundary(&self, cfg: &FunctionCfg) -> LocalSet;
    /// Updates the fact across one event in the analysis's direction.
    fn transfer(&self, event: &Event, state: &mut LocalSet);
}

/// The fact at the start and at the end of every block, in program order
/// whatever the direction of the analysis.
#[derive(Debug)]
pub struct Solution {
    pub at_start: Vec<LocalSet>,
    pub at_end: Vec<LocalSet>,
}

/// Every event of `block`, in the analysis's direction.
fn events_of<P: Problem>(cfg: &FunctionCfg, block: BlockId) -> Vec<&Event> {
    let events = cfg.blocks()[block].stmts.iter().flat_map(|s| &s.events);
    match P::DIRECTION {
        Direction::Forward => events.collect(),
        Direction::Backward => {
            let mut v: Vec<_> = events.collect();
            v.reverse();
            v
        }
    }
}

/// Solves a gen/kill problem using union at joins, returning facts indexed by
/// block ID in program order. All blocks participate, including unreachable
/// ones; callers must filter reachability when reporting findings.
///
/// The problem must use sets sized for `cfg.locals()` and monotone transfers
/// for convergence. Panics from the problem's callbacks propagate to the caller.
pub fn solve<P: Problem>(cfg: &FunctionCfg, problem: &P) -> Solution {
    let n_locals = cfg.locals().len();
    let n_blocks = cfg.blocks().len();
    let preds = cfg.preds();
    // `into[b]` is where facts enter `b`; `from[b]` where they leave it.
    let (into_of, flows_to): (Vec<Vec<BlockId>>, Vec<Vec<BlockId>>) = match P::DIRECTION {
        Direction::Forward => (
            preds,
            cfg.blocks().iter().map(|b| b.succs.clone()).collect(),
        ),
        Direction::Backward => (
            cfg.blocks().iter().map(|b| b.succs.clone()).collect(),
            preds,
        ),
    };
    let boundary_block = match P::DIRECTION {
        Direction::Forward => FunctionCfg::ENTRY,
        Direction::Backward => FunctionCfg::EXIT,
    };

    let mut into = vec![LocalSet::empty(n_locals); n_blocks];
    let mut from = vec![LocalSet::empty(n_locals); n_blocks];
    let boundary = problem.boundary(cfg);

    let mut queue: VecDeque<BlockId> = (0..n_blocks).collect();
    let mut queued = vec![true; n_blocks];
    while let Some(b) = queue.pop_front() {
        queued[b] = false;
        let mut state = if b == boundary_block {
            boundary.clone()
        } else {
            LocalSet::empty(n_locals)
        };
        // The meet. Both ADR-0002 analyses are may-analyses: union.
        for &p in &into_of[b] {
            state.union_with(&from[p]);
        }
        into[b] = state.clone();
        for event in events_of::<P>(cfg, b) {
            problem.transfer(event, &mut state);
        }
        if state != from[b] {
            from[b] = state;
            for &s in &flows_to[b] {
                if !queued[s] {
                    queued[s] = true;
                    queue.push_back(s);
                }
            }
        }
    }

    match P::DIRECTION {
        Direction::Forward => Solution {
            at_start: into,
            at_end: from,
        },
        Direction::Backward => Solution {
            at_start: from,
            at_end: into,
        },
    }
}

/// Walks `block` in the analysis's direction, calling `visit` with each event
/// and the fact *before that event's transfer* — so, in program order, the
/// fact just before the event (forward) or just after it (backward).
/// `solution` must come from the same CFG and problem; unreachable blocks are
/// replayed too if requested.
///
/// # Panics
/// Panics if `block` is outside the CFG or the selected solution vector.
/// Panics from `visit` or the transfer function propagate to the caller.
pub fn replay<'c, P: Problem>(
    cfg: &'c FunctionCfg,
    problem: &P,
    solution: &Solution,
    block: BlockId,
    mut visit: impl FnMut(&'c Event, &LocalSet),
) {
    let mut state = match P::DIRECTION {
        Direction::Forward => solution.at_start[block].clone(),
        Direction::Backward => solution.at_end[block].clone(),
    };
    for event in events_of::<P>(cfg, block) {
        visit(event, &state);
        problem.transfer(event, &mut state);
    }
}

/// Backward liveness.
pub struct Liveness;

impl Problem for Liveness {
    const DIRECTION: Direction = Direction::Backward;

    fn boundary(&self, cfg: &FunctionCfg) -> LocalSet {
        LocalSet::empty(cfg.locals().len())
    }

    fn transfer(&self, event: &Event, live: &mut LocalSet) {
        match event.kind {
            EventKind::Use { .. } => live.insert(event.local),
            EventKind::Def { .. } | EventKind::Kill => live.remove(event.local),
            // A walrus or a match capture may not have run; ending the live
            // range there would invent dead stores.
            EventKind::WeakDef => {}
        }
    }
}

/// Forward may-unbound.
pub struct MayUnbound;

impl Problem for MayUnbound {
    const DIRECTION: Direction = Direction::Forward;

    fn boundary(&self, cfg: &FunctionCfg) -> LocalSet {
        let mut unbound = LocalSet::full(cfg.locals().len());
        for &p in cfg.params() {
            unbound.remove(p);
        }
        unbound
    }

    fn transfer(&self, event: &Event, unbound: &mut LocalSet) {
        match event.kind {
            EventKind::Def { .. } | EventKind::WeakDef => unbound.remove(event.local),
            EventKind::Kill => unbound.insert(event.local),
            EventKind::Use { .. } => {}
        }
    }
}

/// Every reportable store whose value no path reads, one per source location.
/// A store in a copied `finally` counts only if it is dead in every reachable
/// copy. Unreachable stores are omitted; results are ordered by byte offset.
pub fn dead_stores(cfg: &FunctionCfg) -> Vec<(&Event, DefReport)> {
    let solution = solve(cfg, &Liveness);
    let reachable = cfg.reachable();
    // byte_start → (event, report, dead in every copy so far)
    let mut by_loc: BTreeMap<usize, (&Event, DefReport, bool)> = BTreeMap::new();
    for (b, _) in reachable.iter().enumerate().filter(|(_, r)| **r) {
        replay(cfg, &Liveness, &solution, b, |event, live_after| {
            if let EventKind::Def {
                report: Some(report),
            } = event.kind
            {
                let dead = !live_after.contains(event.local);
                by_loc
                    .entry(event.loc.byte_start)
                    .and_modify(|e| e.2 &= dead)
                    .or_insert((event, report, dead));
            }
        });
    }
    // `replay` hands out borrows of `cfg`'s events, so these outlive it.
    by_loc
        .into_values()
        .filter(|(_, _, dead)| *dead)
        .map(|(event, report, _)| (event, report))
        .collect()
}

/// Every checkable read some path reaches with the name unbound, one per
/// source location, ordered by byte offset. Unreachable reads are omitted;
/// a read in any reachable `finally` copy is enough to include the location.
pub fn unbound_reads(cfg: &FunctionCfg) -> Vec<&Event> {
    let solution = solve(cfg, &MayUnbound);
    let reachable = cfg.reachable();
    let mut by_loc: BTreeMap<usize, &Event> = BTreeMap::new();
    for (b, _) in reachable.iter().enumerate().filter(|(_, r)| **r) {
        replay(cfg, &MayUnbound, &solution, b, |event, unbound| {
            if event.kind == (EventKind::Use { checkable: true }) && unbound.contains(event.local) {
                by_loc.entry(event.loc.byte_start).or_insert(event);
            }
        });
    }
    by_loc.into_values().collect()
}

#[cfg(test)]
mod tests;
