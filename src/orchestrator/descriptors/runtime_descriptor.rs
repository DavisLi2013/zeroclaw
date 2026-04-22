use anyhow::ensure;
use serde::{Deserialize, Serialize};

use crate::orchestrator::descriptors::manifest::DescriptorManifest;
use crate::tools::traits::ToolSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillRole {
    Manager,
    Executor,
    Supervisor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDescriptor {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub scope: String,
    pub role: SkillRole,
    pub model_binding_ref: String,
    pub version: String,
    pub body_ref: Option<String>,
}

impl SkillDescriptor {
    pub fn new(skill_id: impl Into<String>, role: SkillRole) -> Self {
        let skill_id = skill_id.into();
        Self {
            name: skill_id.clone(),
            skill_id,
            description: String::new(),
            tags: Vec::new(),
            scope: "session".to_string(),
            role,
            model_binding_ref: "default".to_string(),
            version: "0.1.0".to_string(),
            body_ref: None,
        }
    }

    pub fn from_skill(skill: &crate::skills::Skill, role: SkillRole) -> Self {
        Self {
            skill_id: skill.name.clone(),
            name: skill.name.clone(),
            description: skill.description.clone(),
            tags: skill.tags.clone(),
            scope: "session".to_string(),
            role,
            model_binding_ref: "default".to_string(),
            version: skill.version.clone(),
            body_ref: skill
                .location
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandDescriptor {
    pub command_id: String,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
    pub runtime_binding: String,
    pub execution_mode: String,
    pub permissions: Vec<String>,
}

impl CommandDescriptor {
    pub fn new(
        command_id: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        let command_id = command_id.into();
        Self {
            name: command_id.clone(),
            command_id,
            description: description.into(),
            input_schema,
            output_schema: None,
            runtime_binding: "builtin".to_string(),
            execution_mode: "in_proc".to_string(),
            permissions: Vec::new(),
        }
    }

    fn to_tool_surface_item(&self, descriptor_ref: &str) -> ToolSurfaceItem {
        ToolSurfaceItem {
            tool_id: self.command_id.clone(),
            source_type: ToolSourceType::Command,
            descriptor_ref: descriptor_ref.to_string(),
            spec: ToolSpec {
                name: self.name.clone(),
                description: self.description.clone(),
                parameters: self.input_schema.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookDescriptor {
    pub hook_id: String,
    pub event: String,
    pub target_plugin: String,
    pub order: i32,
    pub conditions: Vec<String>,
    pub failure_policy: String,
}

impl HookDescriptor {
    pub fn new(
        hook_id: impl Into<String>,
        event: impl Into<String>,
        target_plugin: impl Into<String>,
    ) -> Self {
        Self {
            hook_id: hook_id.into(),
            event: event.into(),
            target_plugin: target_plugin.into(),
            order: 0,
            conditions: Vec::new(),
            failure_policy: "continue".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDescriptor {
    pub agent_id: String,
    pub name: String,
    pub role: String,
    pub instruction_refs: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub handoff_policy: String,
}

impl AgentDescriptor {
    pub fn new(agent_id: impl Into<String>, role: impl Into<String>) -> Self {
        let agent_id = agent_id.into();
        Self {
            name: agent_id.clone(),
            agent_id,
            role: role.into(),
            instruction_refs: Vec::new(),
            allowed_tools: Vec::new(),
            handoff_policy: "manual".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerDescriptor {
    pub server_id: String,
    pub transport: String,
    pub endpoint_ref: Option<String>,
    pub auth_ref: Option<String>,
    pub exposure_policy: String,
    pub allowed_tools: Vec<String>,
    pub timeout_policy: String,
}

impl McpServerDescriptor {
    pub fn new(server_id: impl Into<String>, allowed_tools: Vec<String>) -> Self {
        Self {
            server_id: server_id.into(),
            transport: "stdio".to_string(),
            endpoint_ref: None,
            auth_ref: None,
            exposure_policy: "allow_list".to_string(),
            allowed_tools,
            timeout_policy: "default".to_string(),
        }
    }

    fn to_tool_surface_items(&self, descriptor_ref: &str) -> Vec<ToolSurfaceItem> {
        self.allowed_tools
            .iter()
            .map(|tool_name| ToolSurfaceItem {
                tool_id: format!("{}.{}", self.server_id, tool_name),
                source_type: ToolSourceType::McpServer,
                descriptor_ref: descriptor_ref.to_string(),
                spec: ToolSpec {
                    name: tool_name.clone(),
                    description: format!("MCP tool exposed by {}", self.server_id),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {}
                    }),
                },
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDescriptor {
    pub plugin_id: String,
    pub kind: String,
    pub provides: Vec<String>,
}

impl PluginDescriptor {
    pub fn new(plugin_id: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            kind: kind.into(),
            provides: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSourceType {
    Command,
    McpServer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSurfaceItem {
    pub tool_id: String,
    pub source_type: ToolSourceType,
    pub descriptor_ref: String,
    pub spec: ToolSpec,
}

impl ToolSurfaceItem {
    pub fn from_tool_spec(
        spec: ToolSpec,
        descriptor_ref: impl Into<String>,
        source_type: ToolSourceType,
    ) -> Self {
        Self {
            tool_id: spec.name.clone(),
            source_type,
            descriptor_ref: descriptor_ref.into(),
            spec,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DescriptorProjection {
    pub capability_pipeline: Vec<String>,
    pub skill_catalog: Vec<SkillDescriptor>,
    pub tool_surface: Vec<ToolSurfaceItem>,
    pub agent_catalog: Vec<AgentDescriptor>,
    pub hook_catalog: Vec<HookDescriptor>,
    pub plugin_catalog: Vec<PluginDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityRuntimeDescriptor {
    pub manifest: DescriptorManifest,
    pub skills: Vec<SkillDescriptor>,
    pub commands: Vec<CommandDescriptor>,
    pub hooks: Vec<HookDescriptor>,
    pub agents: Vec<AgentDescriptor>,
    pub mcp_servers: Vec<McpServerDescriptor>,
    pub plugins: Vec<PluginDescriptor>,
}

impl CapabilityRuntimeDescriptor {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.skills.is_empty() {
            return Ok(());
        }

        let has_manager = self
            .skills
            .iter()
            .any(|skill| skill.role == SkillRole::Manager);
        let has_executor = self
            .skills
            .iter()
            .any(|skill| skill.role == SkillRole::Executor);
        let has_supervisor = self
            .skills
            .iter()
            .any(|skill| skill.role == SkillRole::Supervisor);

        ensure!(has_manager, "capability skills must include a manager role");
        ensure!(has_executor, "capability skills must include an executor role");
        ensure!(
            has_supervisor,
            "capability skills must include a supervisor role"
        );
        Ok(())
    }

    pub fn project(&self) -> anyhow::Result<DescriptorProjection> {
        self.validate()?;

        let descriptor_ref = format!("{}@{}", self.manifest.descriptor_id, self.manifest.version);
        let mut tool_surface = self
            .commands
            .iter()
            .map(|command| command.to_tool_surface_item(&descriptor_ref))
            .collect::<Vec<_>>();
        for server in &self.mcp_servers {
            tool_surface.extend(server.to_tool_surface_items(&descriptor_ref));
        }

        Ok(DescriptorProjection {
            capability_pipeline: self
                .plugins
                .iter()
                .map(|plugin| plugin.plugin_id.clone())
                .collect(),
            skill_catalog: self.skills.clone(),
            tool_surface,
            agent_catalog: self.agents.clone(),
            hook_catalog: self.hooks.clone(),
            plugin_catalog: self.plugins.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_descriptor_requires_manager_executor_and_supervisor_roles() {
        let descriptor = CapabilityRuntimeDescriptor {
            manifest: DescriptorManifest::new("capability-a", "0.1.0"),
            skills: vec![SkillDescriptor::new("executor", SkillRole::Executor)],
            commands: vec![],
            hooks: vec![],
            agents: vec![],
            mcp_servers: vec![],
            plugins: vec![],
        };

        let error = descriptor.validate().unwrap_err();
        assert!(error.to_string().contains("manager"));
    }

    #[test]
    fn descriptor_projection_collects_skill_tool_agent_and_hook_views() {
        let descriptor = CapabilityRuntimeDescriptor {
            manifest: DescriptorManifest::new("capability-a", "0.1.0"),
            skills: vec![
                SkillDescriptor::new("manager", SkillRole::Manager),
                SkillDescriptor::new("executor", SkillRole::Executor),
                SkillDescriptor::new("supervisor", SkillRole::Supervisor),
            ],
            commands: vec![CommandDescriptor::new(
                "command-a",
                "Run command A",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "input": {"type": "string"}
                    }
                }),
            )],
            hooks: vec![HookDescriptor::new(
                "hook-a",
                "on_context_build_start",
                "plugin-a",
            )],
            agents: vec![AgentDescriptor::new("agent-a", "planner")],
            mcp_servers: vec![McpServerDescriptor::new(
                "mcp-a",
                vec!["fetch_page".to_string()],
            )],
            plugins: vec![PluginDescriptor::new("plugin-a", "observer")],
        };

        let projection = descriptor.project().unwrap();

        assert_eq!(projection.capability_pipeline, vec!["plugin-a"]);
        assert_eq!(projection.skill_catalog.len(), 3);
        assert_eq!(projection.tool_surface.len(), 2);
        assert_eq!(projection.tool_surface[0].tool_id, "command-a");
        assert_eq!(projection.tool_surface[1].tool_id, "mcp-a.fetch_page");
        assert_eq!(projection.agent_catalog.len(), 1);
        assert_eq!(projection.hook_catalog.len(), 1);
    }

    #[test]
    fn skill_descriptor_can_be_derived_from_existing_skill_metadata() {
        let skill = crate::skills::Skill {
            name: "triage".to_string(),
            description: "Investigate incidents".to_string(),
            version: "1.4.0".to_string(),
            author: Some("ops".to_string()),
            tags: vec!["incident".to_string(), "ops".to_string()],
            tools: vec![],
            prompts: vec!["Follow the checklist".to_string()],
            location: Some(std::path::PathBuf::from(
                "workspace/skills/triage/SKILL.md",
            )),
        };

        let descriptor = SkillDescriptor::from_skill(&skill, SkillRole::Supervisor);

        assert_eq!(descriptor.skill_id, "triage");
        assert_eq!(descriptor.description, "Investigate incidents");
        assert_eq!(descriptor.role, SkillRole::Supervisor);
        assert_eq!(
            descriptor.body_ref.as_deref(),
            Some("workspace/skills/triage/SKILL.md")
        );
    }

    #[test]
    fn tool_surface_item_can_wrap_existing_tool_specs() {
        let spec = ToolSpec {
            name: "fetch_url".to_string(),
            description: "Fetch a URL".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"}
                }
            }),
        };

        let item = ToolSurfaceItem::from_tool_spec(
            spec.clone(),
            "capability-a@0.1.0",
            ToolSourceType::Command,
        );

        assert_eq!(item.tool_id, "fetch_url");
        assert_eq!(item.descriptor_ref, "capability-a@0.1.0");
        assert_eq!(item.spec, spec);
    }
}
