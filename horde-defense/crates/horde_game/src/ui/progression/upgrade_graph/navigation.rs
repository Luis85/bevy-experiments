use super::model::Node;
use bevy_egui::egui;

/// The node nearest to `from` in screen direction `dir` (a unit vector), or
/// `None` at the edge of the layout. Sideways distance costs more than reach.
pub(in crate::ui::progression) fn nearest_in_direction(
    nodes: &[Node],
    from: usize,
    dir: egui::Vec2,
) -> Option<usize> {
    let origin = nodes.get(from)?.position;
    nodes
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != from)
        .filter_map(|(index, node)| {
            let delta = node.position - origin;
            let along = delta.dot(dir);
            if along <= 1.0 {
                return None;
            }
            let across = (delta - dir * along).length();
            Some((index, along + across * 2.5))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}
