use crate::affected::WorkspaceEntry;
use crate::config::DistributionConfig;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

type TimingKey = (String, String, String, Option<usize>, String, String);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimingHistory {
    #[serde(default)]
    pub samples: Vec<TimingSample>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<TimingBatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimingBatch {
    pub assignment_id: String,
    pub predicted_duration_ms: u64,
    pub checkout_path_count: usize,
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
    pub checkout_path_count: usize,
    pub prediction_sources: PredictionSources,
    pub reason: String,
    pub checkout: crate::affected::CheckoutPlan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing_environment: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictionSources {
    pub exact: usize,
    pub group: usize,
    pub cold: usize,
    pub sample_count: usize,
}

#[derive(Debug, Clone, Copy)]
enum PredictionSource {
    Exact,
    Group,
    Cold,
}

#[derive(Debug, Clone, Copy)]
struct Prediction {
    duration_ms: u64,
    source: PredictionSource,
    sample_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedTier {
    pub name: String,
    pub max_affected_percent: f64,
    pub concurrency: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing_environment: Option<String>,
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
        group_fallback: Prediction,
    ) -> Prediction {
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
            Prediction {
                duration_ms: median(&mut exact),
                source: PredictionSource::Exact,
                sample_count: exact.len(),
            }
        }
    }

    fn group_fallback(&self, group: &str, runner: &str, environment: &str) -> Prediction {
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
            Prediction {
                duration_ms: 1,
                source: PredictionSource::Cold,
                sample_count: 0,
            }
        } else {
            Prediction {
                duration_ms: median(&mut values),
                source: PredictionSource::Group,
                sample_count: values.len(),
            }
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
    let labels = tier.runner_labels.clone();
    let derived = labels.as_ref().map(|values| {
        let mut sorted = values.clone();
        sorted.sort();
        sorted.join("/")
    });
    SelectedTier {
        name: name.into(),
        max_affected_percent: tier.max_affected_percent,
        concurrency: tier.concurrency,
        runner_labels: tier.runner_labels.clone(),
        timing_environment: tier.timing_environment.clone().or(derived),
    }
}

pub fn assign(
    group: &str, items: &[WorkspaceEntry], concurrency: usize, history: &TimingHistory, runner: &str, environment: &str,
) -> Vec<Assignment> { assign_with_config(group, items, concurrency, history, runner, environment, None, None) }

pub fn assign_with_config(
    group: &str,
    items: &[WorkspaceEntry],
    concurrency: usize,
    history: &TimingHistory,
    runner: &str,
    environment: &str,
    runner_labels: Option<Vec<String>>,
    timing_environment: Option<String>,
) -> Vec<Assignment> {
    if items.is_empty() {
        return vec![];
    }
    let fallback = history.group_fallback(group, runner, environment);
    let mut weighted: Vec<(WorkspaceEntry, Prediction, String)> = items
        .iter()
        .cloned()
        .map(|item| {
            let prediction = history.prediction(&item, runner, environment, fallback);
            let id = work_item_id(&item);
            (item, prediction, id)
        })
        .collect();
    weighted.sort_by(|a, b| {
        b.1.duration_ms
            .cmp(&a.1.duration_ms)
            .then_with(|| a.2.cmp(&b.2))
    });

    let count = concurrency.min(weighted.len());
    let mut buckets: Vec<Assignment> = (1..=count)
        .map(|index| Assignment {
            assignment_id: format!("{group}-{index}"),
            items: vec![],
            predicted_duration_ms: 0,
            checkout_path_count: 0,
            prediction_sources: PredictionSources::default(),
            reason: "minimized predicted runtime makespan, then total sparse checkout paths".into(),
            checkout: crate::affected::checkout_plan(Vec::new()),
            runner_labels: runner_labels.clone(),
            timing_environment: timing_environment.clone(),
        })
        .collect();
    for (item, prediction, _) in weighted {
        let current_checkout_total: usize = buckets
            .iter()
            .map(|bucket| bucket.checkout_path_count)
            .sum();
        let index = buckets
            .iter()
            .enumerate()
            .min_by_key(|(candidate_index, bucket)| {
                let target_load = bucket.predicted_duration_ms + prediction.duration_ms;
                let makespan = buckets
                    .iter()
                    .enumerate()
                    .map(|(index, existing)| {
                        if index == *candidate_index {
                            target_load
                        } else {
                            existing.predicted_duration_ms
                        }
                    })
                    .max()
                    .unwrap_or(target_load);
                let target_checkout_count = bucket
                    .items
                    .iter()
                    .flat_map(|existing| existing.checkout_paths.iter())
                    .chain(item.checkout_paths.iter())
                    .collect::<HashSet<_>>()
                    .len();
                let checkout_total =
                    current_checkout_total - bucket.checkout_path_count + target_checkout_count;
                (makespan, checkout_total, target_load, &bucket.assignment_id)
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        buckets[index].predicted_duration_ms += prediction.duration_ms;
        buckets[index].checkout = crate::affected::checkout_plan(
            buckets[index]
                .items
                .iter()
                .flat_map(|item| item.checkout_paths.iter().cloned())
                .chain(item.checkout_paths.iter().cloned()),
        );
        buckets[index].checkout_path_count = buckets[index]
            .checkout
            .sparse_checkout
            .lines()
            .filter(|path| !path.is_empty())
            .count();
        match prediction.source {
            PredictionSource::Exact => buckets[index].prediction_sources.exact += 1,
            PredictionSource::Group => buckets[index].prediction_sources.group += 1,
            PredictionSource::Cold => buckets[index].prediction_sources.cold += 1,
        }
        buckets[index].prediction_sources.sample_count += prediction.sample_count;
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
    TimingHistory {
        samples,
        batch: None,
    }
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
            checkout_paths: vec![format!("packages/{name}")],
        }
    }

    fn sample(name: &str, duration_ms: u64) -> TimingSample {
        TimingSample {
            group: "ci".into(),
            workspace: name.into(),
            task: "test".into(),
            shard: None,
            runner: "yarn".into(),
            environment: "linux-x64".into(),
            duration_ms,
        }
    }

    #[test]
    fn tier_boundaries_are_inclusive() {
        let config = DistributionConfig {
            small: DistributionTier {
                runner_labels: None, timing_environment: None,
                max_affected_percent: 25.0,
                concurrency: 2,
            },
            medium: DistributionTier {
                runner_labels: None, timing_environment: None,
                max_affected_percent: 60.0,
                concurrency: 4,
            },
            full: DistributionTier {
                runner_labels: None, timing_environment: None,
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
            batch: None,
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
        assert_eq!(result[0].prediction_sources.cold, 2);
    }

    #[test]
    fn runtime_makespan_wins_over_checkout_affinity() {
        let mut a = item("a");
        a.checkout_paths = vec!["shared".into()];
        let b = item("b");
        let mut c = item("c");
        c.checkout_paths = vec!["shared".into()];
        let history = TimingHistory {
            samples: vec![sample("a", 10), sample("b", 9), sample("c", 1)],
            batch: None,
        };

        let result = assign("ci", &[a, b, c], 2, &history, "yarn", "linux-x64");

        let bucket_with_c = result
            .iter()
            .find(|bucket| bucket.items.iter().any(|entry| entry.name == "c"))
            .unwrap();
        assert!(bucket_with_c.items.iter().any(|entry| entry.name == "b"));
        assert_eq!(
            result
                .iter()
                .map(|bucket| bucket.predicted_duration_ms)
                .max(),
            Some(10)
        );
    }

    #[test]
    fn equal_makespan_minimizes_total_sparse_checkout_paths() {
        let mut a = item("a");
        a.checkout_paths = vec!["shared".into()];
        let b = item("b");
        let mut c = item("c");
        c.checkout_paths = vec!["shared".into()];
        let history = TimingHistory {
            samples: vec![sample("a", 10), sample("b", 10), sample("c", 1)],
            batch: None,
        };

        let result = assign("ci", &[a, b, c], 2, &history, "yarn", "linux-x64");

        let bucket_with_c = result
            .iter()
            .find(|bucket| bucket.items.iter().any(|entry| entry.name == "c"))
            .unwrap();
        assert!(bucket_with_c.items.iter().any(|entry| entry.name == "a"));
        assert_eq!(
            result
                .iter()
                .map(|bucket| bucket.checkout_path_count)
                .sum::<usize>(),
            2
        );
        assert_eq!(
            result
                .iter()
                .map(|bucket| bucket.prediction_sources.exact)
                .sum::<usize>(),
            3
        );
        assert_eq!(
            result
                .iter()
                .map(|bucket| bucket.prediction_sources.sample_count)
                .sum::<usize>(),
            3
        );

        let oracle = (1_u8..7)
            .map(|mask| {
                let mut loads = [0_u64; 2];
                let mut paths = [HashSet::new(), HashSet::new()];
                for (index, (duration, checkout)) in
                    [(10, "shared"), (10, "packages/b"), (1, "shared")]
                        .into_iter()
                        .enumerate()
                {
                    let bucket = usize::from(mask & (1 << index) != 0);
                    loads[bucket] += duration;
                    paths[bucket].insert(checkout);
                }
                (
                    *loads.iter().max().unwrap(),
                    paths.iter().map(HashSet::len).sum::<usize>(),
                )
            })
            .min()
            .unwrap();
        assert_eq!(
            (
                result
                    .iter()
                    .map(|bucket| bucket.predicted_duration_ms)
                    .max()
                    .unwrap(),
                result
                    .iter()
                    .map(|bucket| bucket.checkout_path_count)
                    .sum::<usize>()
            ),
            oracle
        );
    }

    #[test]
    fn large_cold_schedule_is_deterministic() {
        let items: Vec<_> = (0..128)
            .flat_map(|index| {
                ["build", "test", "typecheck"].map(move |task| {
                    let mut entry = item(&format!("next-app-{index:03}"));
                    entry.task = task.into();
                    entry
                })
            })
            .collect();
        let first = assign(
            "ci",
            &items,
            24,
            &TimingHistory::default(),
            "yarn",
            "linux-x64",
        );
        let second = assign(
            "ci",
            &items,
            24,
            &TimingHistory::default(),
            "yarn",
            "linux-x64",
        );
        assert_eq!(first, second);
        assert_eq!(first.len(), 24);
        assert_eq!(
            first.iter().map(|bucket| bucket.items.len()).sum::<usize>(),
            384
        );
        assert!(
            first
                .iter()
                .map(|bucket| bucket.checkout_path_count)
                .sum::<usize>()
                <= 384
        );
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
        let merged = merge_histories([TimingHistory {
            samples,
            batch: None,
        }]);
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
