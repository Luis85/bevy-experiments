//! Pure presentation model: ownership groups plus actual nested call paths.
use npc_perf::{PerfDomain, PerfTreeReport, TraceId, TraceNode, clip};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Key {
    Domain(PerfDomain),
    Group(PerfDomain, String),
    Trace(TraceId),
    Reference(TraceId, TraceId),
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub key: Key,
    pub depth: usize,
    pub label: String,
    pub full_name: String,
    pub own_us: Option<f64>,
    pub share: f64,
    pub expandable: bool,
    pub over: bool,
}
pub(crate) fn prefix(mechanic: &str) -> &str {
    mechanic.split('.').next().unwrap_or(mechanic)
}
pub(crate) fn rows(report: &PerfTreeReport, expanded: &BTreeSet<Key>, limit: usize) -> Vec<Row> {
    let index: BTreeMap<_, _> = report.nodes.iter().map(|n| (n.id, n)).collect();
    let mut children: BTreeMap<TraceId, Vec<&TraceNode>> = BTreeMap::new();
    let mut roots: BTreeMap<(PerfDomain, String), Vec<&TraceNode>> = BTreeMap::new();
    for node in &report.nodes {
        if node.frame_wrapper {
            continue;
        }
        if let Some(parent) = node
            .parent
            .and_then(|id| index.get(&id))
            .filter(|p| !p.frame_wrapper)
        {
            children.entry(parent.id).or_default().push(node);
            if parent.domain == node.domain && prefix(&parent.mechanic) == prefix(&node.mechanic) {
                continue;
            }
        }
        roots
            .entry((node.domain, prefix(&node.mechanic).to_owned()))
            .or_default()
            .push(node);
    }
    for list in children.values_mut().chain(roots.values_mut()) {
        list.sort_by(|a, b| {
            b.self_mean_us
                .total_cmp(&a.self_mean_us)
                .then_with(|| a.name.cmp(&b.name))
                .then(a.id.cmp(&b.id))
        });
    }
    let total = report.domains.iter().map(|d| d.work.mean_us).sum::<f64>();
    let share = |cost: f64| {
        if total > 0.0 {
            100.0 * cost / total
        } else {
            0.0
        }
    };
    let mut out = Vec::new();
    for domain in PerfDomain::ALL {
        let measured = report.domains.iter().find(|d| d.domain == domain);
        if domain == PerfDomain::Unclassified && measured.is_none() {
            continue;
        }
        let cost = measured.map_or(0.0, |d| d.work.mean_us);
        let groups: Vec<_> = report
            .groups
            .iter()
            .filter(|g| g.domain == domain && !g.mechanic.contains('.'))
            .collect();
        let key = Key::Domain(domain);
        out.push(Row {
            key: key.clone(),
            depth: 0,
            label: domain.label().into(),
            full_name: domain.label().into(),
            own_us: measured.map(|_| cost),
            share: share(cost),
            expandable: !groups.is_empty(),
            over: groups.iter().any(|g| over(report, &g.mechanic)),
        });
        if !expanded.contains(&key) {
            continue;
        }
        let mut groups = groups;
        groups.sort_by(|a, b| {
            b.work
                .mean_us
                .total_cmp(&a.work.mean_us)
                .then(a.mechanic.cmp(&b.mechanic))
        });
        for group in groups {
            let key = Key::Group(domain, group.mechanic.clone());
            let list = roots.get(&(domain, group.mechanic.clone()));
            out.push(Row {
                key: key.clone(),
                depth: 1,
                label: group.mechanic.clone(),
                full_name: group.mechanic.clone(),
                own_us: Some(group.work.mean_us),
                share: share(group.work.mean_us),
                expandable: list.is_some_and(|l| !l.is_empty()),
                over: over(report, &group.mechanic),
            });
            if expanded.contains(&key)
                && let Some(list) = list
            {
                for node in list {
                    append_trace(&mut out, node, 2, expanded, &children, total, limit);
                }
            }
        }
    }
    out.truncate(limit);
    out
}
fn over(report: &PerfTreeReport, mechanic: &str) -> bool {
    report.budgets.iter().any(|b| {
        b.over_budget && (b.mechanic == mechanic || b.mechanic.starts_with(&format!("{mechanic}.")))
    })
}
fn append_trace(
    out: &mut Vec<Row>,
    node: &TraceNode,
    depth: usize,
    expanded: &BTreeSet<Key>,
    children: &BTreeMap<TraceId, Vec<&TraceNode>>,
    total: f64,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    let key = Key::Trace(node.id);
    let list = children.get(&node.id);
    out.push(Row {
        key: key.clone(),
        depth,
        label: format!(
            "{}{}",
            clip(&node.name, 40),
            match node.kind {
                npc_perf::TraceKind::Deferred => " [deferred]",
                npc_perf::TraceKind::Conditions => " [conditions]",
                _ => "",
            }
        ),
        full_name: node.name.clone(),
        own_us: Some(node.self_mean_us),
        share: if total > 0.0 {
            100.0 * node.self_mean_us / total
        } else {
            0.0
        },
        expandable: list.is_some_and(|l| !l.is_empty()),
        over: false,
    });
    if expanded.contains(&key)
        && let Some(list) = list
    {
        for child in list {
            if out.len() >= limit {
                break;
            }
            if child.domain != node.domain || prefix(&child.mechanic) != prefix(&node.mechanic) {
                out.push(Row {
                    key: Key::Reference(node.id, child.id),
                    depth: depth + 1,
                    label: format!("-> {} / {}", child.domain.label(), clip(&child.name, 26)),
                    full_name: child.name.clone(),
                    own_us: None,
                    share: 0.0,
                    expandable: false,
                    over: false,
                });
            } else {
                append_trace(out, child, depth + 1, expanded, children, total, limit);
            }
        }
    }
}
pub(crate) fn select_trace(report: &PerfTreeReport, id: TraceId, expanded: &mut BTreeSet<Key>) {
    let Some(node) = report.nodes.iter().find(|n| n.id == id) else {
        return;
    };
    expanded.insert(Key::Domain(node.domain));
    expanded.insert(Key::Group(node.domain, prefix(&node.mechanic).into()));
    let mut current = node;
    while let Some(parent) = current
        .parent
        .and_then(|p| report.nodes.iter().find(|n| n.id == p))
    {
        if parent.frame_wrapper
            || parent.domain != node.domain
            || prefix(&parent.mechanic) != prefix(&node.mechanic)
        {
            break;
        }
        expanded.insert(Key::Trace(parent.id));
        current = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_defaults_and_expansion_reveal_measured_systems() {
        let c = npc_perf::PerfCollector::new(16);
        c.record_for_test("npc_needs::decay", 50);
        c.end_frame();
        let report = c.tree_report(None);
        let mut expanded = BTreeSet::new();
        let compact = rows(&report, &expanded, 100);
        assert_eq!(compact.len(), 5);
        assert!(
            compact
                .iter()
                .any(|r| r.label == "Simulation" && r.own_us == Some(50.0))
        );
        expanded.insert(Key::Domain(PerfDomain::Simulation));
        expanded.insert(Key::Group(PerfDomain::Simulation, "needs".into()));
        assert!(
            rows(&report, &expanded, 100)
                .iter()
                .any(|r| r.full_name == "npc_needs::decay")
        );
        assert_eq!(rows(&report, &expanded, 2).len(), 2);
    }
}
