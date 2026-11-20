//! The planner's search, free of island content: integer facts (good counts,
//! finite source stock, coins, position), actions with inputs, kept tools,
//! outputs, a price and a cost, and a bounded uniform-cost search for the
//! cheapest sequence that reaches a target count of one good. It names no
//! good, trade or place; the island builds a [`Domain`] from its own data
//! (`super::domain`) and reads the plan back.
//!
//! Deterministic: the frontier is a `BTreeMap` keyed by `(cost, insertion)`,
//! actions are tried in the order the caller lists them (the island sorts
//! them by key), and every number is an integer.
use npc_framework::prelude::*;
use std::collections::BTreeMap;

/// What the search tracks. Ordered, so it can key the visited map.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Facts {
    /// Units of each good in the domain's good list.
    pub goods: Vec<u32>,
    /// Units left at each finite source (an offer or a patch).
    pub sources: Vec<u32>,
    pub coins: u32,
    pub x: i32,
    pub z: i32,
}

/// One applicable step: it needs `inputs` (consumed) and `requires` (kept,
/// a tool), the coins of `price` and one unit of `source` when it has one;
/// it adds `outputs`, costs `cost` plus travel and leaves the actor at `pos`.
pub struct Action<S> {
    pub step: S,
    pub inputs: Vec<(usize, u32)>,
    pub requires: Vec<(usize, u32)>,
    pub outputs: Vec<(usize, u32)>,
    pub source: Option<usize>,
    pub price: u32,
    pub cost: u32,
    pub pos: SimPos,
}

pub struct Domain<S> {
    pub initial: Facts,
    pub actions: Vec<Action<S>>,
    /// No action runs whose every output is already at its ceiling: the
    /// search never stocks a good beyond what the goal could use.
    pub ceiling: Vec<u32>,
    pub goal: usize,
    pub target: u32,
    /// Walking speed, mm per tick: travel costs distance / speed.
    pub speed: u32,
}

/// The search bounds: states expanded and steps in a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_expansions: u32,
    pub max_steps: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found<S> {
    pub steps: Vec<S>,
    pub expanded: u32,
    pub cost: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<S> {
    Found(Found<S>),
    /// A bound stopped the search before it proved anything.
    Limited {
        expanded: u32,
    },
    /// Nothing reachable within the domain makes the goal.
    Unavailable {
        expanded: u32,
    },
}

impl<S> Outcome<S> {
    pub fn expanded(&self) -> u32 {
        match self {
            Self::Found(found) => found.expanded,
            Self::Limited { expanded } | Self::Unavailable { expanded } => *expanded,
        }
    }
}

struct Node<S> {
    facts: Facts,
    steps: Vec<S>,
    cost: u32,
}

/// Conservative Manhattan travel estimate, integer and host-independent.
pub fn travel(from: (i32, i32), to: SimPos, speed: u32) -> u32 {
    from.0.abs_diff(to.x).saturating_add(from.1.abs_diff(to.z)) / speed.max(1)
}

pub fn search<S: Clone>(domain: &Domain<S>, limits: Limits) -> Outcome<S> {
    let mut frontier = BTreeMap::new();
    let mut visited = BTreeMap::new();
    let mut sequence = 0_u32;
    frontier.insert(
        (0, sequence),
        Node {
            facts: domain.initial.clone(),
            steps: Vec::new(),
            cost: 0,
        },
    );
    visited.insert(domain.initial.clone(), 0);
    let mut expanded = 0;
    let mut step_limited = false;
    while let Some((_, node)) = frontier.pop_first() {
        if node.facts.goods[domain.goal] >= domain.target {
            return Outcome::Found(Found {
                steps: node.steps,
                expanded,
                cost: node.cost,
            });
        }
        if expanded >= limits.max_expansions {
            return Outcome::Limited { expanded };
        }
        expanded += 1;
        if node.steps.len() >= limits.max_steps as usize {
            step_limited = true;
            continue;
        }
        if visited
            .get(&node.facts)
            .is_some_and(|cost| *cost < node.cost)
        {
            continue;
        }
        for action in &domain.actions {
            if node.facts.coins < action.price
                || action
                    .source
                    .is_some_and(|source| node.facts.sources[source] == 0)
                || action
                    .inputs
                    .iter()
                    .chain(&action.requires)
                    .any(|(index, count)| node.facts.goods[*index] < *count)
                || action
                    .outputs
                    .iter()
                    .all(|(index, _)| node.facts.goods[*index] >= domain.ceiling[*index])
            {
                continue;
            }
            let mut facts = node.facts.clone();
            for (index, count) in &action.inputs {
                facts.goods[*index] -= *count;
            }
            for (index, count) in &action.outputs {
                facts.goods[*index] = facts.goods[*index].saturating_add(*count);
            }
            if let Some(source) = action.source {
                facts.sources[source] -= 1;
            }
            facts.coins -= action.price;
            let cost = node.cost.saturating_add(action.cost).saturating_add(travel(
                (node.facts.x, node.facts.z),
                action.pos,
                domain.speed,
            ));
            facts.x = action.pos.x;
            facts.z = action.pos.z;
            if visited.get(&facts).is_some_and(|old| *old <= cost) {
                continue;
            }
            visited.insert(facts.clone(), cost);
            let mut steps = node.steps.clone();
            steps.push(action.step.clone());
            sequence = sequence.saturating_add(1);
            frontier.insert((cost, sequence), Node { facts, steps, cost });
        }
    }
    if step_limited {
        Outcome::Limited { expanded }
    } else {
        Outcome::Unavailable { expanded }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: Limits = Limits {
        max_expansions: 128,
        max_steps: 12,
    };

    fn action(
        step: &'static str,
        inputs: Vec<(usize, u32)>,
        outputs: Vec<(usize, u32)>,
    ) -> Action<&'static str> {
        Action {
            step,
            inputs,
            requires: Vec::new(),
            outputs,
            source: None,
            price: 0,
            cost: 1,
            pos: SimPos::default(),
        }
    }

    /// Buy a raw good (finite offer, a price), mill it, bake from two.
    fn example(stock: u32, coins: u32) -> Domain<&'static str> {
        let mut buy = action("buy raw", vec![], vec![(0, 1)]);
        buy.source = Some(0);
        buy.price = 3;
        let mut bake = action("bake", vec![(1, 2)], vec![(2, 4)]);
        bake.cost = 3;
        let mut mill = action("mill", vec![(0, 1)], vec![(1, 1)]);
        mill.cost = 2;
        Domain {
            initial: Facts {
                goods: vec![0, 0, 0],
                sources: vec![stock],
                coins,
                x: 0,
                z: 0,
            },
            ceiling: vec![2, 2, 1],
            goal: 2,
            target: 1,
            speed: 70,
            actions: vec![buy, mill, bake],
        }
    }

    #[test]
    fn forward_search_resolves_multistage_preconditions_and_is_deterministic() {
        let domain = example(2, 6);
        let Outcome::Found(a) = search(&domain, LIMITS) else {
            panic!("valid plan")
        };
        assert_eq!(a.steps.len(), 5);
        assert_eq!(a.steps.first(), Some(&"buy raw"));
        assert_eq!(a.steps.last(), Some(&"bake"));
        assert_eq!(Outcome::Found(a), search(&domain, LIMITS));
    }

    #[test]
    fn finite_stock_cash_and_search_budgets_are_real_preconditions() {
        assert!(matches!(
            search(&example(1, 6), LIMITS),
            Outcome::Unavailable { .. }
        ));
        assert!(matches!(
            search(&example(2, 5), LIMITS),
            Outcome::Unavailable { .. }
        ));
        let short = Limits {
            max_steps: 4,
            ..LIMITS
        };
        assert!(matches!(
            search(&example(2, 6), short),
            Outcome::Limited { .. }
        ));
        let bounded = Limits {
            max_expansions: 1,
            ..LIMITS
        };
        assert!(matches!(
            search(&example(2, 6), bounded),
            Outcome::Limited { .. }
        ));
    }

    /// A kept tool is a precondition that is not consumed, and the search
    /// buys it first only when the cheaper steps it unlocks pay for it.
    #[test]
    fn a_tool_is_bought_only_when_the_time_it_saves_pays_for_it() {
        // goods: 0 raw, 1 tool. Bare gathering costs 100, with the tool 40;
        // the tool costs 150 ticks of walking and trading.
        let domain = |target: u32| {
            let bare = Action {
                cost: 100,
                ..action("gather bare", vec![], vec![(0, 1)])
            };
            let tooled = Action {
                cost: 40,
                requires: vec![(1, 1)],
                ..action("gather with tool", vec![], vec![(0, 1)])
            };
            let buy = Action {
                cost: 150,
                price: 5,
                ..action("buy tool", vec![], vec![(1, 1)])
            };
            Domain {
                initial: Facts {
                    goods: vec![0, 0],
                    sources: vec![],
                    coins: 10,
                    x: 0,
                    z: 0,
                },
                ceiling: vec![target, 1],
                goal: 0,
                target,
                speed: 70,
                actions: vec![buy, bare, tooled],
            }
        };
        let Outcome::Found(one) = search(&domain(1), LIMITS) else {
            panic!("one unit")
        };
        assert_eq!(one.steps, ["gather bare"]);
        let Outcome::Found(four) = search(&domain(4), LIMITS) else {
            panic!("four units")
        };
        assert_eq!(four.steps.first(), Some(&"buy tool"));
        assert_eq!(four.cost, 150 + 4 * 40);
    }
}
