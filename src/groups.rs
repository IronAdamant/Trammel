//! MCP tool groups, following the suite-wide tool-groups convention.
//!
//! Every tool is in one group. `core` is always on. `locks` and `notes` are
//! the suite's shared names for features other tools also offer. Two sources
//! switch groups or single tools off, applied per key (`tools`, `disable`):
//!
//! 1. `TRAMMEL_TOOLS` / `TRAMMEL_DISABLE` (the operator). A bad name stops
//!    the server with exit code 2.
//! 2. An `agent-tools` block in the project's AGENTS.md (an agent):
//!    `trammel: tools <list>` / `trammel: disable <list>`. A bad line is
//!    ignored with a warning; it never stops the server.
//!
//! The switches shape only the MCP surface. With neither source in use,
//! nothing changes.

use std::collections::BTreeSet;

/// `(group, tools)`. The first group is `core`.
pub const GROUPS: &[(&str, &[&str])] = &[
    ("core", &["start_plan", "decompose", "get_plan", "resume", "complete_plan", "status"]),
    ("plan", &["explore", "list_plans", "update_plan_status", "merge_plans", "export_plan", "record_steps", "history", "estimate", "list_strategies"]),
    ("verify", &["verify_step", "verify_plan"]),
    ("recipes", &["save_recipe", "get_recipe", "list_recipes"]),
    ("constraints", &["add_constraint", "get_constraints", "deactivate_constraint", "resolve_failure", "failure_history"]),
    ("locks", &["claim_step", "release_step", "available_steps"]),
    ("notes", &["add_note"]),
    ("maintenance", &["prune_recipes", "prune_plans", "validate_recipes", "usage_stats"]),
];

pub const CORE: &str = "core";

/// This tool's name in an `agent-tools` block.
pub const BLOCK_NAME: &str = "trammel";

pub fn group_of(tool: &str) -> Option<&'static str> {
    GROUPS.iter().find(|(_, tools)| tools.contains(&tool)).map(|(group, _)| *group)
}

/// Where a key's value came from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Source {
    Env,
    File,
}

/// Why a tool is off, and the word to change.
#[derive(Clone, Debug, PartialEq)]
enum Off {
    NotSelected(Source),
    Disabled(String, Source),
}

/// Trammel's lines from the first `agent-tools` block of an AGENTS.md.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentLines {
    /// The file has an `agent-tools` block (for any tool).
    pub block: bool,
    pub tools: Vec<String>,
    pub disable: Vec<String>,
}

/// A fence line: its character, its length, and the info string after it.
fn fence_of(line: &str) -> Option<(char, usize, &str)> {
    let first = line.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let len = line.chars().take_while(|c| *c == first).count();
    (len >= 3).then(|| (first, len, line[len..].trim()))
}

/// Read the first top-level fenced block whose info string is `agent-tools`
/// and keep this tool's `tools` / `disable` lines. `#` starts a comment.
/// Fences inside other fences are content, so an example block shown in a
/// ````markdown fence does not count.
pub fn agent_lines(markdown: &str) -> AgentLines {
    let mut out = AgentLines::default();
    // The open fence, if any, and whether it is the agent-tools block.
    let mut open: Option<(char, usize, bool)> = None;
    for line in markdown.lines() {
        let trimmed = line.trim();
        match open {
            None => {
                if let Some((ch, len, info)) = fence_of(trimmed) {
                    let ours = info == "agent-tools";
                    open = Some((ch, len, ours));
                    if ours { out.block = true; }
                }
            }
            Some((ch, len, ours)) => {
                if let Some((c, l, info)) = fence_of(trimmed) {
                    if c == ch && l >= len && info.is_empty() {
                        if ours { break; }
                        open = None;
                        continue;
                    }
                }
                if !ours { continue; }
                let content = trimmed.split('#').next().unwrap_or("").trim();
                let Some((tool, rest)) = content.split_once(':') else { continue };
                if tool.trim().to_ascii_lowercase() != BLOCK_NAME { continue; }
                let rest = rest.trim();
                let (key, list) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                match key.to_ascii_lowercase().as_str() {
                    "tools" => out.tools.push(list.trim().to_string()),
                    "disable" => out.disable.push(list.trim().to_string()),
                    _ => {}
                }
            }
        }
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct Switches {
    /// Tools chosen by a `tools` key (core included), when one is in force.
    selected: Option<(BTreeSet<&'static str>, Source)>,
    /// Tools hidden by a `disable` key, with the token that hid them.
    disabled: Vec<(&'static str, String, Source)>,
    /// The project's AGENTS.md has an `agent-tools` block.
    block: bool,
    /// Ignored AGENTS.md lines, one sentence each.
    warnings: Vec<String>,
}

enum Problem {
    Unknown(String),
    Core(String),
}

impl Switches {
    /// From the environment only (no AGENTS.md).
    pub fn parse(tools: Option<&str>, disable: Option<&str>) -> Result<Switches, String> {
        Switches::resolve(tools, disable, &AgentLines::default())
    }

    /// Combine the env vars with the AGENTS.md lines, per key: a set env var
    /// wins over the file's line for the same key. Env errors are returned
    /// (the caller exits 2); file errors become warnings.
    pub fn resolve(env_tools: Option<&str>, env_disable: Option<&str>, file: &AgentLines) -> Result<Switches, String> {
        let mut s = Switches { block: file.block, ..Switches::default() };
        let env_tools = env_tools.filter(|v| !v.trim().is_empty());
        let env_disable = env_disable.filter(|v| !v.trim().is_empty());

        if let Some(raw) = env_tools {
            let picked = select(raw).map_err(|p| env_error("TRAMMEL_TOOLS", p))?;
            s.selected = Some((picked, Source::Env));
        } else {
            let mut picked: Option<BTreeSet<&'static str>> = None;
            for line in &file.tools {
                match select(line) {
                    Ok(tools) => picked.get_or_insert_with(BTreeSet::new).extend(tools),
                    Err(p) => s.warnings.push(file_warning(p)),
                }
            }
            s.selected = picked.map(|tools| (tools, Source::File));
        }

        if let Some(raw) = env_disable {
            for (tool, token) in hide(raw).map_err(|p| env_error("TRAMMEL_DISABLE", p))? {
                s.add_disabled(tool, token, Source::Env);
            }
        } else {
            for line in &file.disable {
                match hide(line) {
                    Ok(hits) => for (tool, token) in hits { s.add_disabled(tool, token, Source::File) },
                    Err(p) => s.warnings.push(file_warning(p)),
                }
            }
        }
        Ok(s)
    }

    fn add_disabled(&mut self, tool: &'static str, token: String, source: Source) {
        if !self.disabled.iter().any(|(t, _, _)| *t == tool) {
            self.disabled.push((tool, token, source));
        }
    }

    /// True when anything shapes the surface or should be reported: an env
    /// var, an `agent-tools` block, or a warning about one.
    pub fn active(&self) -> bool {
        self.selected.is_some() || !self.disabled.is_empty() || self.block || !self.warnings.is_empty()
    }

    fn off(&self, tool: &str) -> Option<Off> {
        if let Some((_, token, source)) = self.disabled.iter().find(|(t, _, _)| *t == tool) {
            return Some(Off::Disabled(token.clone(), *source));
        }
        match &self.selected {
            Some((chosen, source)) if !chosen.contains(tool) => Some(Off::NotSelected(*source)),
            _ => None,
        }
    }

    /// Whether `tools/list` shows `tool`. `default_listed` is the answer
    /// without these switches (the `TRAMMEL_MCP_SURFACE` listing).
    pub fn listed(&self, tool: &str, default_listed: bool) -> bool {
        if self.off(tool).is_some() { return false; }
        if self.selected.is_some() { return true; }
        default_listed
    }

    /// `Err` with the one-line message when a call to `tool` is switched off.
    pub fn check_call(&self, tool: &str) -> Result<(), String> {
        let group = group_of(tool).unwrap_or("");
        match self.off(tool) {
            None => Ok(()),
            Some(Off::Disabled(token, Source::Env)) => Err(format!("tool \"{tool}\" is disabled (group \"{group}\") by TRAMMEL_DISABLE; remove \"{token}\" from it to enable")),
            Some(Off::Disabled(token, Source::File)) => Err(format!("tool \"{tool}\" is disabled (group \"{group}\") by AGENTS.md agent-tools; remove \"{token}\" from the \"{BLOCK_NAME}: disable\" line to enable")),
            Some(Off::NotSelected(Source::Env)) => Err(format!("tool \"{tool}\" is disabled (group \"{group}\") by TRAMMEL_TOOLS; add \"{group}\" to it to enable")),
            Some(Off::NotSelected(Source::File)) => Err(format!("tool \"{tool}\" is disabled (group \"{group}\") by AGENTS.md agent-tools; add \"{group}\" to the \"{BLOCK_NAME}: tools\" line to enable")),
        }
    }

    /// One line naming the groups on and off, and how an agent changes them.
    pub fn summary(&self) -> Option<String> {
        if !self.active() { return None; }
        let (mut on, mut off) = (Vec::new(), Vec::new());
        for (group, tools) in GROUPS {
            let live = tools.iter().filter(|t| self.off(t).is_none()).count();
            match live {
                0 => off.push(group.to_string()),
                n if n == tools.len() => on.push(group.to_string()),
                n => on.push(format!("{group} ({n} of {})", tools.len())),
            }
        }
        let off = if off.is_empty() { String::new() } else { format!("; off: {}", off.join(", ")) };
        Some(format!("Tool groups on: {}{off}. Change them in AGENTS.md with an agent-tools line such as \"{BLOCK_NAME}: disable locks, notes\".", on.join(", ")))
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Lines appended to the `initialize` instructions: the summary, then
    /// any AGENTS.md warnings. None when nothing is switched.
    pub fn instruction_lines(&self) -> Option<String> {
        let mut lines = vec![self.summary()?];
        lines.extend(self.warnings.iter().cloned());
        Some(lines.join("\n"))
    }

    /// The tools `tools/list` shows for a surface, to tell when a file change
    /// changed the listing.
    pub fn listing(&self, surface_all: bool) -> Vec<&'static str> {
        all_tools().filter(|t| self.listed(t, surface_all || crate::tools::PRIMARY.contains(t))).collect()
    }
}

fn tokens(raw: &str) -> Vec<String> {
    raw.split(|c: char| c == ',' || c.is_whitespace()).map(|t| t.trim().to_ascii_lowercase()).filter(|t| !t.is_empty()).collect()
}

fn core_tools() -> impl Iterator<Item = &'static str> {
    GROUPS[0].1.iter().copied()
}

fn all_tools() -> impl Iterator<Item = &'static str> {
    GROUPS.iter().flat_map(|(_, tools)| tools.iter().copied())
}

fn lookup(token: &str) -> Option<Vec<&'static str>> {
    if let Some((_, tools)) = GROUPS.iter().find(|(group, _)| *group == token) {
        return Some(tools.to_vec());
    }
    all_tools().find(|t| *t == token).map(|t| vec![t])
}

/// A `tools` list: core plus the named groups and tools (`all` = everything).
fn select(raw: &str) -> Result<BTreeSet<&'static str>, Problem> {
    let mut chosen: BTreeSet<&'static str> = core_tools().collect();
    for token in tokens(raw) {
        if token == "all" {
            chosen.extend(all_tools());
            continue;
        }
        chosen.extend(lookup(&token).ok_or(Problem::Unknown(token))?);
    }
    Ok(chosen)
}

/// A `disable` list: the tools it hides, each with the token that named it.
fn hide(raw: &str) -> Result<Vec<(&'static str, String)>, Problem> {
    let mut out = Vec::new();
    for token in tokens(raw) {
        let hit = lookup(&token).ok_or_else(|| Problem::Unknown(token.clone()))?;
        if hit.iter().any(|t| group_of(t) == Some(CORE)) {
            return Err(Problem::Core(token));
        }
        out.extend(hit.into_iter().map(|t| (t, token.clone())));
    }
    Ok(out)
}

fn group_names() -> String {
    GROUPS.iter().map(|(g, _)| *g).collect::<Vec<_>>().join(", ")
}

fn env_error(var: &str, problem: Problem) -> String {
    let mut tools: Vec<&str> = all_tools().collect();
    tools.sort();
    let known = format!("groups: {}; tools: {}", group_names(), tools.join(", "));
    match problem {
        Problem::Unknown(token) => format!("{var}: unknown group \"{token}\"; {known}"),
        Problem::Core(token) => format!("{var}: \"{token}\" is in group \"core\", which is always on; {known}"),
    }
}

fn file_warning(problem: Problem) -> String {
    match problem {
        Problem::Unknown(token) => format!("AGENTS.md agent-tools: unknown group \"{token}\" for {BLOCK_NAME}; groups: {}", group_names()),
        Problem::Core(token) => format!("AGENTS.md agent-tools: \"{token}\" is in group \"core\", which is always on, for {BLOCK_NAME}; that line is ignored"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_in_exactly_one_group() {
        let mut grouped: Vec<&str> = all_tools().collect();
        grouped.sort();
        assert_eq!(grouped, crate::tools::all_names(), "groups must cover every tool once");
        assert_eq!(GROUPS[0].0, CORE);
        assert_eq!(group_of("claim_step"), Some("locks"));
        assert_eq!(group_of("add_note"), Some("notes"));
    }

    #[test]
    fn unset_or_empty_changes_nothing() {
        for (tools, disable) in [(None, None), (Some(""), Some("  "))] {
            let s = Switches::parse(tools, disable).unwrap();
            assert!(!s.active());
            assert!(!s.listed("claim_step", false) && s.listed("start_plan", true));
            assert!(s.check_call("claim_step").is_ok());
            assert_eq!(s.summary(), None);
            assert_eq!(s.instruction_lines(), None);
        }
    }

    #[test]
    fn names_are_case_and_space_blind_and_errors_name_the_variable() {
        let s = Switches::parse(Some(" Locks , VERIFY_step "), Some("RELEASE_STEP")).unwrap();
        assert!(s.listed("claim_step", false) && s.listed("verify_step", false) && s.listed("status", false));
        assert!(!s.listed("release_step", false) && !s.listed("get_recipe", true));
        let err = Switches::parse(None, Some("lokcs")).unwrap_err();
        assert!(err.starts_with("TRAMMEL_DISABLE: unknown group \"lokcs\"; groups: core, plan, verify, recipes, constraints, locks, notes, maintenance; tools: add_constraint, "), "{err}");
        assert!(Switches::parse(Some("everything"), None).unwrap_err().starts_with("TRAMMEL_TOOLS: unknown group \"everything\""));
        assert!(Switches::parse(None, Some("core")).unwrap_err().contains("always on"));
        assert!(Switches::parse(None, Some("status")).unwrap_err().contains("always on"));
        assert!(Switches::parse(Some("all"), None).unwrap().listed("prune_plans", false));
    }

    const FILE: &str = "# Project\n\n```sh\ntrammel: disable verify\n```\n\n```agent-tools\n# one agent works here\ntrammel: disable locks notes   # coordinationhub owns leases\nchisel: disable locks, history\nTRAMMEL: tools all\n```\n\n```agent-tools\ntrammel: disable recipes\n```\n";

    #[test]
    fn the_first_agent_tools_block_gives_this_tools_lines() {
        let lines = agent_lines(FILE);
        assert_eq!(lines, AgentLines { block: true, tools: vec!["all".into()], disable: vec!["locks notes".into()] });
        assert_eq!(agent_lines("# no block here\n"), AgentLines::default());
        assert_eq!(agent_lines("~~~agent-tools\nother: disable x\n~~~\n"), AgentLines { block: true, ..AgentLines::default() });
        // An example shown inside a longer fence is not a block.
        assert_eq!(agent_lines("````markdown\n```agent-tools\ntrammel: disable locks\n```\n````\n"), AgentLines::default());
        // This repo's AGENTS.md documents the block that way, so it switches nothing.
        let own = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/AGENTS.md")).unwrap();
        assert!(own.contains("```agent-tools"));
        assert_eq!(agent_lines(&own), AgentLines::default());
    }

    #[test]
    fn env_beats_the_file_per_key() {
        let file = agent_lines(FILE);
        let s = Switches::resolve(None, None, &file).unwrap();
        assert!(!s.listed("claim_step", true) && !s.listed("add_note", true) && s.listed("prune_plans", false));
        assert_eq!(s.check_call("claim_step").unwrap_err(), "tool \"claim_step\" is disabled (group \"locks\") by AGENTS.md agent-tools; remove \"locks\" from the \"trammel: disable\" line to enable");
        // TRAMMEL_DISABLE replaces the file's disable line; the file's tools line still applies.
        let s = Switches::resolve(None, Some("notes"), &file).unwrap();
        assert!(s.listed("claim_step", false) && !s.listed("add_note", true) && s.listed("prune_plans", false));
        // TRAMMEL_TOOLS replaces the file's tools line; the file's disable line still applies.
        let s = Switches::resolve(Some("verify"), None, &file).unwrap();
        assert!(!s.listed("prune_plans", true) && !s.listed("claim_step", true) && s.listed("verify_step", false));
    }

    #[test]
    fn file_mistakes_are_warnings_never_errors() {
        let file = agent_lines("```agent-tools\ntrammel: disable lokcs\ntrammel: disable core\ntrammel: disable notes\n```\n");
        let s = Switches::resolve(None, None, &file).unwrap();
        assert_eq!(s.warnings(), [
            "AGENTS.md agent-tools: unknown group \"lokcs\" for trammel; groups: core, plan, verify, recipes, constraints, locks, notes, maintenance".to_string(),
            "AGENTS.md agent-tools: \"core\" is in group \"core\", which is always on, for trammel; that line is ignored".to_string(),
        ]);
        // The good line still applies; the bad ones change nothing.
        assert!(!s.listed("add_note", true) && s.listed("status", true));
        let text = s.instruction_lines().unwrap();
        assert_eq!(text.lines().count(), 3);
        assert!(text.starts_with("Tool groups on: core, plan, verify, recipes, constraints, locks, maintenance; off: notes."));
    }
}
