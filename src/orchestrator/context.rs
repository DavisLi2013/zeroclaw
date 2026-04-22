use crate::orchestrator::contracts::ContextBuildReason;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextSourceType {
    SystemPrompt,
    RuntimeDirective,
    Memory,
    UserMessage,
    SkillCatalog,
    SkillBody,
    ToolResult,
    AgentResult,
    HookContribution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextTarget {
    SystemPrompt,
    UserMessage,
    ToolMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextItem {
    pub item_id: String,
    pub source_type: ContextSourceType,
    pub target: ContextTarget,
    pub loading_reason: String,
    pub budget_weight: usize,
    pub priority: u32,
    pub content: String,
}

impl ContextItem {
    pub fn new(
        item_id: impl Into<String>,
        source_type: ContextSourceType,
        target: ContextTarget,
        loading_reason: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            item_id: item_id.into(),
            source_type,
            target,
            loading_reason: loading_reason.into(),
            budget_weight: 1,
            priority: 0,
            content: content.into(),
        }
    }

    pub fn with_budget_weight(mut self, budget_weight: usize) -> Self {
        self.budget_weight = budget_weight.max(1);
        self
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedContextItem {
    pub item: ContextItem,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextPackage {
    pub system_prompt: Option<String>,
    pub user_message: Option<String>,
    pub tool_messages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBuildResult {
    pub build_reason: ContextBuildReason,
    pub selected_context_items: Vec<ContextItem>,
    pub rejected_context_items: Vec<RejectedContextItem>,
    pub context_package: ContextPackage,
}

pub struct ContextBuilder {
    build_reason: ContextBuildReason,
    max_budget: usize,
    items: Vec<ContextItem>,
}

impl ContextBuilder {
    pub fn new(build_reason: ContextBuildReason) -> Self {
        Self::with_budget(build_reason, usize::MAX)
    }

    pub fn with_budget(build_reason: ContextBuildReason, max_budget: usize) -> Self {
        Self {
            build_reason,
            max_budget,
            items: Vec::new(),
        }
    }

    pub fn push(&mut self, item: ContextItem) {
        self.items.push(item);
    }

    pub fn build(self) -> ContextBuildResult {
        let mut indexed_items: Vec<(usize, ContextItem)> = self.items.into_iter().enumerate().collect();
        indexed_items.sort_by(|(left_index, left_item), (right_index, right_item)| {
            right_item
                .priority
                .cmp(&left_item.priority)
                .then_with(|| left_index.cmp(right_index))
        });

        let mut selected_context_items = Vec::new();
        let mut rejected_context_items = Vec::new();
        let mut used_budget = 0usize;

        for (_, item) in indexed_items {
            if item.content.trim().is_empty() {
                rejected_context_items.push(RejectedContextItem {
                    item,
                    reason: "empty_content".into(),
                });
                continue;
            }

            let next_budget = used_budget.saturating_add(item.budget_weight);
            if next_budget > self.max_budget {
                rejected_context_items.push(RejectedContextItem {
                    item,
                    reason: "budget_exhausted".into(),
                });
                continue;
            }

            used_budget = next_budget;
            selected_context_items.push(item);
        }

        let context_package = ContextPackage {
            system_prompt: render_target(&selected_context_items, ContextTarget::SystemPrompt),
            user_message: render_target(&selected_context_items, ContextTarget::UserMessage),
            tool_messages: render_target_items(&selected_context_items, ContextTarget::ToolMessage),
        };

        ContextBuildResult {
            build_reason: self.build_reason,
            selected_context_items,
            rejected_context_items,
            context_package,
        }
    }
}

fn render_target(items: &[ContextItem], target: ContextTarget) -> Option<String> {
    let rendered = items
        .iter()
        .filter(|item| item.target == target)
        .map(|item| item.content.trim())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

fn render_target_items(items: &[ContextItem], target: ContextTarget) -> Vec<String> {
    items
        .iter()
        .filter(|item| item.target == target)
        .map(|item| item.content.trim())
        .filter(|content| !content.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_builder_renders_system_and_user_targets() {
        let mut builder = ContextBuilder::new(ContextBuildReason::TurnGeneration);
        builder.push(
            ContextItem::new(
                "system",
                ContextSourceType::SystemPrompt,
                ContextTarget::SystemPrompt,
                "bootstrap",
                "system-body",
            )
            .with_priority(100),
        );
        builder.push(
            ContextItem::new(
                "memory",
                ContextSourceType::Memory,
                ContextTarget::UserMessage,
                "recall",
                "memory-body",
            )
            .with_priority(50),
        );
        builder.push(
            ContextItem::new(
                "user",
                ContextSourceType::UserMessage,
                ContextTarget::UserMessage,
                "request",
                "user-body",
            )
            .with_priority(10),
        );

        let result = builder.build();

        assert_eq!(result.selected_context_items.len(), 3);
        assert_eq!(
            result.context_package.system_prompt.as_deref(),
            Some("system-body")
        );
        assert_eq!(
            result.context_package.user_message.as_deref(),
            Some("memory-body\n\nuser-body")
        );
    }

    #[test]
    fn context_builder_rejects_items_when_budget_is_exhausted() {
        let mut builder = ContextBuilder::with_budget(ContextBuildReason::TurnGeneration, 2);
        builder.push(
            ContextItem::new(
                "system",
                ContextSourceType::SystemPrompt,
                ContextTarget::SystemPrompt,
                "bootstrap",
                "system-body",
            )
            .with_priority(100)
            .with_budget_weight(1),
        );
        builder.push(
            ContextItem::new(
                "memory",
                ContextSourceType::Memory,
                ContextTarget::UserMessage,
                "recall",
                "memory-body",
            )
            .with_priority(50)
            .with_budget_weight(1),
        );
        builder.push(
            ContextItem::new(
                "tool",
                ContextSourceType::ToolResult,
                ContextTarget::UserMessage,
                "tool_result_reentry",
                "tool-body",
            )
            .with_priority(1)
            .with_budget_weight(1),
        );

        let result = builder.build();

        assert_eq!(result.selected_context_items.len(), 2);
        assert_eq!(result.rejected_context_items.len(), 1);
        assert_eq!(result.rejected_context_items[0].item.item_id, "tool");
    }

    #[test]
    fn context_builder_keeps_tool_messages_as_separate_entries() {
        let mut builder = ContextBuilder::new(ContextBuildReason::ToolResultReentry);
        builder.push(
            ContextItem::new(
                "tool-1",
                ContextSourceType::ToolResult,
                ContextTarget::ToolMessage,
                "tool_result_reentry",
                "{\"tool_call_id\":\"1\",\"content\":\"first\"}",
            )
            .with_priority(10),
        );
        builder.push(
            ContextItem::new(
                "tool-2",
                ContextSourceType::ToolResult,
                ContextTarget::ToolMessage,
                "tool_result_reentry",
                "{\"tool_call_id\":\"2\",\"content\":\"second\"}",
            )
            .with_priority(5),
        );

        let result = builder.build();

        assert_eq!(result.context_package.tool_messages.len(), 2);
        assert_eq!(
            result.context_package.tool_messages,
            vec![
                "{\"tool_call_id\":\"1\",\"content\":\"first\"}",
                "{\"tool_call_id\":\"2\",\"content\":\"second\"}",
            ]
        );
    }
}
