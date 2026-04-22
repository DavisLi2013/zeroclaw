pub mod manifest;
pub mod runtime_descriptor;

pub use manifest::DescriptorManifest;
pub use runtime_descriptor::{
    AgentDescriptor, CapabilityRuntimeDescriptor, CommandDescriptor, DescriptorProjection,
    HookDescriptor, McpServerDescriptor, PluginDescriptor, SkillDescriptor, SkillRole,
    ToolSourceType, ToolSurfaceItem,
};
