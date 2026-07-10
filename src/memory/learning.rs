use crate::core::{Learning, LearningLevel};

#[derive(Debug, Clone, Default)]
pub struct LearningBuckets {
    pub experiments: Vec<Learning>,
    pub decisions: Vec<Learning>,
    pub playbook: Vec<Learning>,
}

pub fn bucket_learning(items: Vec<Learning>) -> LearningBuckets {
    let mut buckets = LearningBuckets::default();
    for item in items {
        match item.level {
            LearningLevel::SingleObservation => buckets.experiments.push(item),
            LearningLevel::StableDecision => buckets.decisions.push(item),
            LearningLevel::PlaybookRule => buckets.playbook.push(item),
        }
    }
    buckets
}

pub fn render_learning(item: &Learning) -> String {
    format!(
        "- Summary: {}\n- Evidence: {}\n- Action: {}\n- Sources: {}\n",
        item.summary,
        item.evidence,
        item.recommended_action,
        item.source_experiment_ids.join(", ")
    )
}
