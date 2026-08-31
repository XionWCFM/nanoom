use crate::affected::WorkspaceEntry;
use crate::config::DistributionConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

type TimingKey = (String, String, String, Option<usize>, String, String);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimingHistory {
    #[serde(default)]
    pub samples: Vec<TimingSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimingSample {
    pub group: String,
    pub workspace: String,
    pub task: String,
    #[serde(default)]
    pub shard: Option<usize>,
    pub runner: String,
    pub environment: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub assignment_id: String,
    pub items: Vec<WorkspaceEntry>,
    pub predicted_duration_ms: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedTier {
    pub name: String,
    pub max_affected_percent: f64,
    pub concurrency: usize,
}

impl TimingHistory {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        serde_json::from_str(&content).map_err(|error| error.to_string())
    }

    fn prediction(
        &self,
        item: &WorkspaceEntry,
        runner: &str,
        environment: &str,
        group_fallback: u64,
    ) -> u64 {
        let mut exact: Vec<u64> = self
            .samples
            .iter()
            .rev()
            .filter(|sample| {
                sample.group == item.group
                    && sample.workspace == item.name
                    && sample.task == item.task
                    && sample.shard == item.shard
                    && sample.runner == runner
                    && sample.environment == environment
                    && sample.duration_ms > 0
            })
            .take(7)
            .map(|sample| sample.duration_ms)
            .collect();
        if exact.is_empty() {
            group_fallback
        } else {
            median(&mut exact)
        }
    }

    fn group_fallback(&self, group: &str, runner: &str, environment: &str) -> u64 {
        let mut values: Vec<u64> = self
            .samples
            .iter()
            .filter(|sample| {
                sample.group == group
                    && sample.runner == runner
                    && sample.environment == environment
                    && sample.duration_ms > 0
            })
            .map(|sample| sample.duration_ms)
            .collect();
        if values.is_empty() {
            1
        } else {
            median(&mut values)
        }
    }
}

fn median(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    let upper = values.len() / 2;
    if values.len().is_multiple_of(2) {
        values[upper - 1] + (values[upper] - values[upper - 1]) / 2
    } else {
        values[upper]
    }
}

pub fn select_tier(config: &DistributionConfig, affected_percent: f64) -> SelectedTier {
    let (name, tier) = if affected_percent <= config.small.max_affected_percent {
        ("small", &config.small)
    } else if affected_percent <= config.medium.max_affected_percent {
        ("medium", &config.medium)
    } else {
        ("full", &config.full)
    };
    SelectedTier {
        name: name.into(),
        max_affected_percent: tier.max_affected_percent,
        concurrency: tier.concurrency,
    }
}

pub fn assign(
    group: &str,
    items: &[WorkspaceEntry],
    concurrency: usize,
    history: &TimingHistory,
    runner: &str,
    environment: &str,
) -> Vec<Assignment> {
    if items.is_empty() {
        return vec![];
    }
    let fallback = history.group_fallback(group, runner, environment);
    let mut weighted: Vec<(WorkspaceEntry, u64, String)> = items
        .iter()
        .cloned()
        .map(|item| {
            let prediction = history.prediction(&item, runner, environment, fallback);
            let id = work_item_id(&item);
            (item, prediction, id)
        })
        .collect();
    weighted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));

    let count = concurrency.min(weighted.len());
    let mut buckets: Vec<Assignment> = (1..=count)
        .map(|index| Assignment {
            assignment_id: format!("{group}-{index}"),
            items: vec![],
            predicted_duration_ms: 0,
            reason: "longest predicted item assigned to the least-loaded bucket".into(),
        })
        .collect();
    for (item, prediction, _) in weighted {
        let index = buckets
            .iter()
            .enumerate()
            .min_by_key(|(_, bucket)| (&bucket.predicted_duration_ms, &bucket.assignment_id))
            .map(|(index, _)| index)
            .unwrap_or(0);
        buckets[index].predicted_duration_ms += prediction;
        buckets[index].items.push(item);
    }
    buckets
}

pub fn merge_histories(histories: impl IntoIterator<Item = TimingHistory>) -> TimingHistory {
    let mut by_key: HashMap<TimingKey, Vec<TimingSample>> = HashMap::new();
    for sample in histories.into_iter().flat_map(|history| history.samples) {
        if sample.duration_ms == 0 {
            continue;
        }
        let key = (
            sample.group.clone(),
            sample.workspace.clone(),
            sample.task.clone(),
            sample.shard,
            sample.runner.clone(),
            sample.environment.clone(),
        );
        let samples = by_key.entry(key).or_default();
        samples.push(sample);
        if samples.len() > 7 {
            samples.remove(0);
        }
    }
    let mut samples: Vec<TimingSample> = by_key.into_values().flatten().collect();
    samples.sort_by(|a, b| {
        (
            &a.group,
            &a.workspace,
            &a.task,
            a.shard,
            &a.runner,
            &a.environment,
        )
            .cmp(&(
                &b.group,
                &b.workspace,
                &b.task,
                b.shard,
                &b.runner,
                &b.environment,
            ))
    });
    TimingHistory { samples }
}

fn work_item_id(item: &WorkspaceEntry) -> String {
    format!(
        "{}:{}:{}:{}",
        item.group,
        item.name,
        item.task,
        item.shard.unwrap_or(0)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DistributionConfig, DistributionTier};

    fn item(name: &str) -> WorkspaceEntry {
        WorkspaceEntry {
            group: "ci".into(),
            name: name.into(),
            path: format!("packages/{name}"),
            task: "test".into(),
            shard: None,
            total_shards: None,
        }
    }

    #[test]
    fn tier_boundaries_are_inclusive() {
        let config = DistributionConfig {
            small: DistributionTier {
                max_affected_percent: 25.0,
                concurrency: 2,
            },
            medium: DistributionTier {
                max_affected_percent: 60.0,
                concurrency: 4,
            },
            full: DistributionTier {
                max_affected_percent: 100.0,
                concurrency: 8,
            },
        };
        assert_eq!(select_tier(&config, 0.0).name, "small");
        assert_eq!(select_tier(&config, 25.0).name, "small");
        assert_eq!(select_tier(&config, 25.1).name, "medium");
        assert_eq!(select_tier(&config, 60.0).name, "medium");
        assert_eq!(select_tier(&config, 60.1).name, "full");
        assert_eq!(select_tier(&config, 100.0).name, "full");
    }

    #[test]
    fn lpt_uses_recent_median_and_is_deterministic() {
        let history = TimingHistory {
            samples: [10, 1000, 11, 9, 12, 10, 8, 7]
                .into_iter()
                .map(|duration_ms| TimingSample {
                    group: "ci".into(),
                    workspace: "a".into(),
                    task: "test".into(),
                    shard: None,
                    runner: "yarn".into(),
                    environment: "linux-x64".into(),
                    duration_ms,
                })
                .collect(),
        };
        let result = assign(
            "ci",
            &[item("a"), item("b"), item("c")],
            2,
            &history,
            "yarn",
            "linux-x64",
        );
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].items[0].name, "a");
        assert_eq!(result.iter().flat_map(|bucket| &bucket.items).count(), 3);
        assert_eq!(
            result,
            assign(
                "ci",
                &[item("a"), item("b"), item("c")],
                2,
                &history,
                "yarn",
                "linux-x64"
            )
        );
    }

    #[test]
    fn cold_start_and_shards_have_stable_identity() {
        let mut first = item("a");
        first.shard = Some(1);
        first.total_shards = Some(2);
        let mut second = first.clone();
        second.shard = Some(2);
        let result = assign(
            "ci",
            &[second, first],
            1,
            &TimingHistory::default(),
            "nx",
            "linux-x64",
        );
        assert_eq!(result[0].predicted_duration_ms, 2);
        assert_eq!(result[0].items[0].shard, Some(1));
        assert_eq!(result[0].items[1].shard, Some(2));
    }

    #[test]
    fn merge_keeps_only_recent_successful_samples() {
        let samples = (0..9)
            .map(|duration_ms| TimingSample {
                group: "ci".into(),
                workspace: "a".into(),
                task: "test".into(),
                shard: None,
                runner: "nx".into(),
                environment: "linux-x64".into(),
                duration_ms,
            })
            .collect();
        let merged = merge_histories([TimingHistory { samples }]);
        assert_eq!(merged.samples.len(), 7);
        assert_eq!(merged.samples.first().unwrap().duration_ms, 2);
        assert_eq!(merged.samples.last().unwrap().duration_ms, 8);
    }

    #[test]
    fn corrupt_history_is_rejected_for_caller_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(TimingHistory::load(&path).is_err());
    }
}
