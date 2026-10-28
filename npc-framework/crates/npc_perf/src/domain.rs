//! Configurable diagnostic ownership, independent of simulation/mechanic identities.
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PerfDomain {
    Simulation,
    Presentation,
    UserInterface,
    Rendering,
    Platform,
    #[default]
    Unclassified,
}

impl PerfDomain {
    pub const ALL: [Self; 6] = [
        Self::Simulation,
        Self::Presentation,
        Self::UserInterface,
        Self::Rendering,
        Self::Platform,
        Self::Unclassified,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Simulation => "Simulation",
            Self::Presentation => "World presentation",
            Self::UserInterface => "User interface",
            Self::Rendering => "Rendering (CPU)",
            Self::Platform => "Engine / host",
            Self::Unclassified => "Unclassified",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "simulation" | "sim" => Some(Self::Simulation),
            "presentation" | "world" => Some(Self::Presentation),
            "user_interface" | "ui" => Some(Self::UserInterface),
            "rendering" | "render" => Some(Self::Rendering),
            "platform" | "engine" | "host" => Some(Self::Platform),
            "unclassified" => Some(Self::Unclassified),
            _ => None,
        }
    }
}

/// Longest matching system prefix wins, followed by longest mechanic prefix.
/// Hosts can classify their own namespaces without coupling this crate to content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainTags {
    systems: Vec<(String, PerfDomain)>,
    mechanics: Vec<(String, PerfDomain)>,
}
impl Default for DomainTags {
    fn default() -> Self {
        use PerfDomain::*;
        let mut tags = Self {
            systems: Vec::new(),
            mechanics: Vec::new(),
        };
        for prefix in [
            "npc_kernel",
            "npc_ai",
            "npc_needs",
            "npc_jobs",
            "npc_locomotion",
            "npc_nav",
            "npc_perception",
            "npc_rules",
            "npc_interaction",
            "npc_progression",
            "npc_crafting",
            "npc_command",
            "npc_vehicles",
            "npc_agent_io",
            "npc_framework::glue",
        ] {
            tags.system(prefix, Simulation);
        }
        tags.system("npc_view_helpers", Presentation);
        tags.system("npc_agent_io::scheduler", Platform);
        tags.system("npc_view_helpers::perf_runtime", Platform)
            .system("npc_view_helpers::timing", Platform);
        for module in ["perf_panel", "journal_panel", "command_ui", "driver_ui"] {
            tags.system(format!("npc_view_helpers::{module}"), UserInterface);
        }
        for prefix in ["npc_tools", "npc_perf", "bevy_"] {
            tags.system(prefix, Platform);
        }
        for prefix in ["bevy_ui", "bevy_text", "bevy_input_focus", "bevy_a11y"] {
            tags.system(prefix, UserInterface);
        }
        for prefix in [
            "bevy_render",
            "bevy_pbr",
            "bevy_core_pipeline",
            "bevy_sprite_render",
            "bevy_ui_render",
            "bevy_gizmos_render",
            "bevy_anti_alias",
            "bevy_post_process",
            "bevy_extract",
            "bevy_material",
            "bevy_shader",
        ] {
            tags.system(prefix, Rendering);
        }
        for prefix in ["bevy_animation", "bevy_transform"] {
            tags.system(prefix, Presentation);
        }
        for prefix in [
            "inputs",
            "needs",
            "clock",
            "think",
            "actions",
            "spatial",
            "locomotion",
            "nav",
            "perception",
            "fog",
            "rules",
            "agent_io",
            "interaction",
            "progression",
            "crafting",
            "command",
            "vehicles",
            "effects",
            "journal",
            "driver",
            "simulation",
        ] {
            tags.mechanic(prefix, Simulation);
        }
        tags.mechanic("view", Presentation);
        for prefix in [
            "view.overlay",
            "view.panels",
            "view.perf_panel",
            "view.journal_panel",
            "view.explorer_panel",
            "view.explorer_modal",
            "view.field_clipboard",
            "view.command_ui",
            "view.command",
            "view.driver_ui",
        ] {
            tags.mechanic(prefix, UserInterface);
        }
        for prefix in ["frame", "engine", "debug", "remote", "perf"] {
            tags.mechanic(prefix, Platform);
        }
        tags.mechanic("render", Rendering);
        tags
    }
}
impl DomainTags {
    pub fn system(&mut self, prefix: impl Into<String>, domain: PerfDomain) -> &mut Self {
        Self::put(&mut self.systems, prefix.into(), domain);
        self
    }
    pub fn mechanic(&mut self, prefix: impl Into<String>, domain: PerfDomain) -> &mut Self {
        Self::put(&mut self.mechanics, prefix.into(), domain);
        self
    }
    fn put(entries: &mut Vec<(String, PerfDomain)>, prefix: String, domain: PerfDomain) {
        if let Some(entry) = entries.iter_mut().find(|(p, _)| *p == prefix) {
            entry.1 = domain;
        } else {
            entries.push((prefix, domain));
        }
    }
    pub fn classify(&self, system: Option<&str>, mechanic: &str) -> Option<PerfDomain> {
        fn longest(entries: &[(String, PerfDomain)], value: &str) -> Option<PerfDomain> {
            entries
                .iter()
                .filter(|(p, _)| value.starts_with(p))
                .max_by_key(|(p, _)| p.len())
                .map(|(_, d)| *d)
        }
        system
            .and_then(|name| longest(&self.systems, name.trim_start_matches(['<', '&', ' '])))
            .or_else(|| longest(&self.mechanics, mechanic))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_is_explicit_longest_and_unknown_is_not_hidden() {
        let mut tags = DomainTags::default();
        assert_eq!(
            tags.classify(Some("npc_perception::scan"), "perception"),
            Some(PerfDomain::Simulation)
        );
        assert_eq!(
            tags.classify(Some("bevy_ui::layout"), "engine"),
            Some(PerfDomain::UserInterface)
        );
        assert_eq!(
            tags.classify(Some("bevy_pbr::prepare"), "render"),
            Some(PerfDomain::Rendering)
        );
        assert_eq!(tags.classify(Some("my_game::custom"), "untagged"), None);
        tags.system("my_game", PerfDomain::Simulation)
            .system("my_game::hud", PerfDomain::UserInterface);
        assert_eq!(
            tags.classify(Some("my_game::hud::text"), "view"),
            Some(PerfDomain::UserInterface)
        );
        assert_eq!(
            tags.classify(Some("a_content::panel"), "view.explorer_panel"),
            Some(PerfDomain::UserInterface)
        );
    }
}
