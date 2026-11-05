//! MCP tool groups and the `TRAMMEL_TOOLS` / `TRAMMEL_DISABLE` switches,
//! following the suite-wide tool-groups convention. Every tool is in one
//! group. `core` is always on. `locks` and `notes` are the suite's shared
//! names for features other tools also offer. The switches shape only the
//! MCP surface; with neither set, nothing changes.

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

pub fn group_of(tool: &str) -> Option<&'static str> {
    GROUPS.iter().find(|(_, tools)| tools.contains(&tool)).map(|(group, _)| *group)
}

/// Which variable turned a tool off, and the word to change in it.
#[derive(Clone, Debug, PartialEq)]
enum Off {
    /// Not named in `TRAMMEL_TOOLS`.
    NotSelected,
    /// Named in `TRAMMEL_DISABLE` by this token (a group or the tool).
    Disabled(String),
}

#[derive(Clone, Debug, Default)]
pub struct Switches {
    /// Tools chosen by `TRAMMEL_TOOLS` (core included), when it is set.
    selected: Option<BTreeSet<&'static str>>,
    /// Tools hidden by `TRAMMEL_DISABLE`, with the token that hid them.
    disabled: Vec<(&'static str, String)>,
}

impl Switches {
    pub fn from_env() -> Result<Switches, String> {
        Switches::parse(std::env::var("TRAMMEL_TOOLS").ok().as_deref(), std::env::var("TRAMMEL_DISABLE").ok().as_deref())
    }

    /// Parse the two variables. An empty value counts as unset. Any unknown
    /// name, or an attempt to turn off `core`, is an error naming the variable.
    pub fn parse(tools: Option<&str>, disable: Option<&str>) -> Result<Switches, String> {
        let mut switches = Switches::default();
        if let Some(raw) = tools.filter(|v| !v.trim().is_empty()) {
            let mut chosen: BTreeSet<&'static str> = core_tools().collect();
            for token in tokens(raw) {
                if token == "all" {
                    chosen.extend(all_tools());
                    continue;
                }
                chosen.extend(resolve("TRAMMEL_TOOLS", &token)?);
            }
            switches.selected = Some(chosen);
        }
        if let Some(raw) = disable.filter(|v| !v.trim().is_empty()) {
            for token in tokens(raw) {
                let hit = resolve("TRAMMEL_DISABLE", &token)?;
                if hit.iter().any(|t| group_of(t) == Some(CORE)) {
                    return Err(format!("TRAMMEL_DISABLE: \"{token}\" is in group \"core\", which is always on; {}", known()));
                }
                for tool in hit {
                    if !switches.disabled.iter().any(|(t, _)| *t == tool) {
                        switches.disabled.push((tool, token.clone()));
                    }
                }
            }
        }
        Ok(switches)
    }

    /// True when either variable is set.
    pub fn active(&self) -> bool {
        self.selected.is_some() || !self.disabled.is_empty()
    }

    fn off(&self, tool: &str) -> Option<Off> {
        if let Some((_, token)) = self.disabled.iter().find(|(t, _)| *t == tool) {
            return Some(Off::Disabled(token.clone()));
        }
        match &self.selected {
            Some(chosen) if !chosen.contains(tool) => Some(Off::NotSelected),
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
            Some(Off::Disabled(token)) => Err(format!("tool \"{tool}\" is disabled (group \"{group}\") by TRAMMEL_DISABLE; remove \"{token}\" from it to enable")),
            Some(Off::NotSelected) => Err(format!("tool \"{tool}\" is disabled (group \"{group}\") by TRAMMEL_TOOLS; add \"{group}\" to it to enable")),
        }
    }

    /// One line for `initialize` and `status` when a switch is set, e.g.
    /// `Tool groups on: core, plan, verify, recipes, constraints, maintenance; off: locks, notes.`
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
        Some(format!("Tool groups on: {}{off}.", on.join(", ")))
    }
}

fn tokens(raw: &str) -> Vec<String> {
    raw.split(',').map(|t| t.trim().to_ascii_lowercase()).filter(|t| !t.is_empty()).collect()
}

fn core_tools() -> impl Iterator<Item = &'static str> {
    GROUPS[0].1.iter().copied()
}

fn all_tools() -> impl Iterator<Item = &'static str> {
    GROUPS.iter().flat_map(|(_, tools)| tools.iter().copied())
}

fn resolve(var: &str, token: &str) -> Result<Vec<&'static str>, String> {
    if let Some((_, tools)) = GROUPS.iter().find(|(group, _)| *group == token) {
        return Ok(tools.to_vec());
    }
    if let Some(tool) = all_tools().find(|t| *t == token) {
        return Ok(vec![tool]);
    }
    Err(format!("{var}: unknown group \"{token}\"; {}", known()))
}

fn known() -> String {
    let groups: Vec<&str> = GROUPS.iter().map(|(g, _)| *g).collect();
    let mut tools: Vec<&str> = all_tools().collect();
    tools.sort();
    format!("groups: {}; tools: {}", groups.join(", "), tools.join(", "))
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
            assert!(s.listed("claim_step", false) == false && s.listed("start_plan", true));
            assert!(s.check_call("claim_step").is_ok());
            assert_eq!(s.summary(), None);
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
}
