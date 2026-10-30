use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const DEFAULT_CONTEXT_BUDGET_TOKENS: u32 = 12_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextReferenceKind {
    TaskContract,
    BlueprintEntity,
    ProjectFact,
    DependencySummary,
    Instruction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextReference {
    pub id: String,
    pub kind: ContextReferenceKind,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextOmission {
    pub reference_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContext {
    pub task_key: String,
    pub contract_hash: String,
    pub markdown: String,
    pub estimated_tokens: u32,
    pub budget_tokens: u32,
    pub included: Vec<ContextReference>,
    pub omitted: Vec<ContextOmission>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskReferenceContent {
    pub reference_id: String,
    pub title: String,
    pub content: String,
    pub content_hash: String,
}

pub fn estimate_tokens(text: &str) -> u32 {
    let (ascii, non_ascii) = text
        .chars()
        .fold((0u32, 0u32), |(ascii, non_ascii), character| {
            if character.is_ascii() {
                (ascii + 1, non_ascii)
            } else {
                (ascii, non_ascii + 1)
            }
        });
    ascii.div_ceil(4) + non_ascii
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimate_counts_ascii_and_cjk_conservatively() {
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("冷启动"), 3);
        assert_eq!(estimate_tokens("abc冷"), 2);
    }
}
