//! The AI explorer's support code: the prompt templates the game shows the dev,
//! random names, agent folders, the prepared agent workspace (`AGENTS.md`,
//! Claude Code config, `.mcp.json`), app settings and the window-free host
//! that builds or resumes an explorer world ([`host`]; the notes themselves
//! are `npc_framework::agent_io::notes`).

pub mod clipboard;
pub mod home;
pub mod host;
pub mod names;
pub mod prompt;
pub mod settings;
pub mod workspace;
