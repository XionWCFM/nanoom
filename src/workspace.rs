use crate::config::{Config, DiscoveredWorkspace, PackageJson};
use crate::error::Result;
use globset::{Glob, GlobSetBuilder};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub dependencies: Vec<String>,
    /// Dependency name → declared range (`workspace:*`, `^1.0.0`, ...).
    pub dependency_specs: HashMap<String, String>,
    pub dependents: Vec<String>,
    /// Declared `version` from package.json, if any.
    pub package_json_version: Option<String>,
}

pub struct Workspace {
    projects: Vec<Project>,
    name_to_index: HashMap<String, usize>,
    path_to_index: HashMap<PathBuf, usize>,
}

impl Workspace {
    pub fn discover(config: &Config, cwd: &Path) -> Result<Self> {
        let workspaces = discover_workspaces(config, cwd)?;
        let mut projects = Vec::new();
        let mut name_to_index = HashMap::new();
        let mut path_to_index = HashMap::new();

        for (idx, ws) in workspaces.iter().enumerate() {
            if name_to_index.contains_key(&ws.name) {
                return Err(crate::error::Error::ConfigValidation(format!(
                    "Duplicate workspace name '{}'",
                    ws.name
                )));
            }
            let project = Project {
                name: ws.name.clone(),
                path: ws.path.clone(),
                dependencies: ws.dependencies.clone(),
                dependency_specs: ws.dependency_specs.clone(),
                dependents: Vec::new(),
                package_json_version: ws.package_json.version.clone(),
            };

            name_to_index.insert(ws.name.clone(), idx);
            path_to_index.insert(ws.path.clone(), idx);
            projects.push(project);
        }

        let mut workspace = Self {
            projects,
            name_to_index,
            path_to_index,
        };

        workspace.build_dependents();
        Ok(workspace)
    }

    fn build_dependents(&mut self) {
        // Workspace protocol (`workspace:*`, `link:`, `file:`) is an unambiguous
        // internal link. Plain semver ranges only count when they are satisfied
        // by the local package version — mirrors how pnpm resolves links.
        let versions: HashMap<&str, Option<&str>> = self
            .projects
            .iter()
            .map(|p| (p.name.as_str(), p.package_json_version.as_deref()))
            .collect();

        let name_to_index = self.name_to_index.clone();
        let project_names: Vec<String> = self.projects.iter().map(|p| p.name.clone()).collect();

        let mut dependents_to_add: HashMap<usize, Vec<String>> = HashMap::new();

        for (idx, project) in self.projects.iter().enumerate() {
            for dep in &project.dependencies {
                let Some(&dep_idx) = name_to_index.get(dep) else {
                    continue;
                };
                if dep_idx == idx {
                    continue;
                }
                let spec = project
                    .dependency_specs
                    .get(dep)
                    .map(String::as_str)
                    .unwrap_or("");
                let local_version = versions
                    .get(self.projects[dep_idx].name.as_str())
                    .copied()
                    .flatten();
                if is_internal_link(spec, local_version) {
                    dependents_to_add
                        .entry(dep_idx)
                        .or_default()
                        .push(project_names[idx].clone());
                }
            }
        }

        for (dep_idx, dependents) in dependents_to_add {
            self.projects[dep_idx].dependents.extend(dependents);
        }
    }

    pub fn get_project_by_name(&self, name: &str) -> Option<&Project> {
        self.name_to_index.get(name).map(|&idx| &self.projects[idx])
    }

    pub fn get_project_by_path(&self, path: &Path) -> Option<&Project> {
        self.path_to_index.get(path).map(|&idx| &self.projects[idx])
    }

    pub fn all_projects(&self) -> &[Project] {
        &self.projects
    }

    pub fn project_count(&self) -> usize {
        self.projects.len()
    }

    pub fn dependency_closure_paths(&self, name: &str, cwd: &Path) -> Vec<String> {
        let mut pending = vec![name.to_string()];
        let mut seen = HashSet::new();
        let mut paths = Vec::new();
        while let Some(current) = pending.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            let Some(project) = self.get_project_by_name(&current) else {
                continue;
            };
            paths.push(
                project
                    .path
                    .strip_prefix(cwd)
                    .unwrap_or(&project.path)
                    .to_string_lossy()
                    .into_owned(),
            );
            pending.extend(
                project
                    .dependencies
                    .iter()
                    .filter(|dependency| self.get_project_by_name(dependency).is_some())
                    .cloned(),
            );
        }
        paths.sort();
        paths.dedup();
        paths
    }
}

fn discover_workspaces(config: &Config, cwd: &Path) -> Result<Vec<DiscoveredWorkspace>> {
    let mut workspaces = Vec::new();
    for entry in WalkDir::new(cwd)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !matches!(entry.file_name().to_str(), Some(".git" | "node_modules")))
    {
        let entry = entry?;
        if entry.file_name() != "package.json" {
            continue;
        }
        let Some(path) = entry.path().parent() else {
            continue;
        };
        let relative = path.strip_prefix(cwd).unwrap_or(path);

        if workspace_path_is_included(config, relative) {
            workspaces.push(read_workspace(path, cwd)?);
        }
    }

    workspaces.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(workspaces)
}

pub fn missing_workspace_manifests(config: &Config, cwd: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["ls-files", "-z", "--", "*/package.json"])
        .output()?;
    if !output.status.success() {
        return Ok(vec![]);
    }

    let mut missing = Vec::new();
    for bytes in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let relative = PathBuf::from(String::from_utf8_lossy(bytes).into_owned());
        let Some(parent) = relative.parent() else {
            continue;
        };
        if workspace_path_is_included(config, parent) && !cwd.join(&relative).is_file() {
            missing.push(relative);
        }
    }
    missing.sort();
    Ok(missing)
}

pub fn is_workspace_manifest(config: &Config, path: &Path, cwd: &Path) -> bool {
    let relative = path.strip_prefix(cwd).unwrap_or(path);
    relative
        .file_name()
        .is_some_and(|name| name == "package.json")
        && relative
            .parent()
            .is_some_and(|parent| workspace_path_is_included(config, parent))
}

fn workspace_path_is_included(config: &Config, relative: &Path) -> bool {
    let include = config.workspace.include.is_empty()
        || config.workspace.include.iter().any(|pattern| {
            Glob::new(pattern)
                .map(|glob| glob.compile_matcher().is_match(relative))
                .unwrap_or(false)
        });
    let exclude = config.workspace.exclude.iter().any(|pattern| {
        Glob::new(pattern)
            .map(|glob| glob.compile_matcher().is_match(relative))
            .unwrap_or(false)
    });
    include && !exclude
}

fn read_workspace(path: &Path, _root: &Path) -> Result<DiscoveredWorkspace> {
    let package_json_path = path.join("package.json");
    let content = std::fs::read_to_string(&package_json_path)?;
    let package_json: PackageJson = serde_json::from_str(&content)?;

    let name = package_json.name.clone().unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    let mut dependency_specs = package_json.dependencies.clone();
    dependency_specs.extend(package_json.dev_dependencies.clone());
    dependency_specs.extend(package_json.peer_dependencies.clone());
    dependency_specs.extend(package_json.optional_dependencies.clone());

    let dependencies: Vec<String> = dependency_specs.keys().cloned().collect();

    Ok(DiscoveredWorkspace {
        name,
        path: path.to_path_buf(),
        package_json,
        dependencies,
        dependency_specs,
        dependents: Vec::new(),
    })
}

/// Decides whether a manifest entry refers to an in-workspace package.
///
/// - `workspace:*` / `workspace:^1.2.0` / `link:` / `file:` are always internal.
/// - Plain semver ranges are internal when satisfied by the local package
///   version (or when the range is a wildcard). Unresolvable ranges such as
///   `^2.0.0` against a local `1.x` package point at the registry instead.
fn is_internal_link(spec: &str, local_version: Option<&str>) -> bool {
    if spec.starts_with("workspace:") || spec.starts_with("link:") || spec.starts_with("file:") {
        return true;
    }

    let Some(local_version) = local_version else {
        // Without a declared version we cannot verify compatibility; treat the
        // edge conservatively so changes are never silently missed.
        return true;
    };

    crate::deps::is_satisfied(spec, local_version)
}

pub fn calculate_affected(
    workspace: &Workspace,
    changed_files: &[PathBuf],
    global_deps: &[String],
    cwd: &Path,
    include_dependents: bool,
) -> Vec<Project> {
    let mut affected_indices = HashSet::new();

    let global_files = matching_global_files(changed_files, global_deps, cwd);

    if !global_files.is_empty() {
        return workspace.projects.clone();
    }

    for file in changed_files {
        let relative = file.strip_prefix(cwd).unwrap_or(file);

        for (idx, project) in workspace.projects.iter().enumerate() {
            let project_relative = project.path.strip_prefix(cwd).unwrap_or(&project.path);
            if relative.starts_with(project_relative) {
                affected_indices.insert(idx);
                break;
            }
        }
    }

    if include_dependents {
        let mut queue: Vec<usize> = affected_indices.iter().copied().collect();
        while let Some(idx) = queue.pop() {
            let dependents = workspace.projects[idx].dependents.clone();
            for dep_name in dependents {
                if let Some(&dep_idx) = workspace.name_to_index.get(&dep_name) {
                    if affected_indices.insert(dep_idx) {
                        queue.push(dep_idx);
                    }
                }
            }
        }
    }

    // Deterministic order (workspace discovery order = sorted by path) so
    // generated matrices are stable across runs.
    let mut ordered: Vec<usize> = affected_indices.into_iter().collect();
    ordered.sort_unstable();
    ordered
        .into_iter()
        .map(|idx| workspace.projects[idx].clone())
        .collect()
}

pub(crate) fn matching_global_files(
    changed_files: &[PathBuf],
    global_deps: &[String],
    cwd: &Path,
) -> Vec<PathBuf> {
    let Some(matcher) = build_global_matcher(global_deps, cwd) else {
        return vec![];
    };
    changed_files
        .iter()
        .filter(|file| matcher.is_match(file.strip_prefix(cwd).unwrap_or(file)))
        .cloned()
        .collect()
}

fn build_global_matcher(patterns: &[String], _cwd: &Path) -> Option<globset::GlobSet> {
    if patterns.is_empty() {
        return None;
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        if let Ok(glob) = Glob::new(pattern) {
            builder.add(glob);
        }
    }
    builder.build().ok()
}

pub fn apply_rules(
    projects: Vec<Project>,
    rules: &[crate::config::Rule],
    _group_tasks: &[String],
) -> Vec<Project> {
    let mut result = Vec::new();

    for project in projects {
        let rule = rules.iter().find(|r| r.name == project.name);

        if let Some(rule) = rule {
            if rule.ignore {
                continue;
            }
        }

        result.push(project);
    }

    result
}

/// Orders projects so that dependencies come before their dependents.
/// Cycles are tolerated by appending remaining projects after sorted ones.
pub fn topological_sort(projects: &[Project]) -> Vec<Project> {
    let name_to_index: HashMap<&str, usize> = projects
        .iter()
        .enumerate()
        .map(|(idx, p)| (p.name.as_str(), idx))
        .collect();

    let mut in_degree = vec![0usize; projects.len()];
    let mut adjacents: Vec<Vec<usize>> = vec![Vec::new(); projects.len()];

    for (idx, project) in projects.iter().enumerate() {
        for dep in &project.dependencies {
            if let Some(&dep_idx) = name_to_index.get(dep.as_str()) {
                if dep_idx != idx {
                    adjacents[dep_idx].push(idx);
                    in_degree[idx] += 1;
                }
            }
        }
    }

    let mut queue: Vec<usize> = (0..projects.len())
        .filter(|&idx| in_degree[idx] == 0)
        .collect();
    queue.sort_by_key(|&idx| projects[idx].name.clone());
    let mut order = Vec::with_capacity(projects.len());

    while let Some(idx) = queue.first().copied() {
        queue.remove(0);
        order.push(idx);

        let mut next: Vec<usize> = Vec::new();
        for &adjacent in &adjacents[idx] {
            in_degree[adjacent] -= 1;
            if in_degree[adjacent] == 0 {
                next.push(adjacent);
            }
        }
        next.sort_by_key(|&idx| projects[idx].name.clone());
        queue.extend(next);
    }

    // Append any projects stuck in cycles to preserve completeness.
    for (idx, project) in projects.iter().enumerate() {
        if !order.contains(&idx) {
            order.push(idx);
            let _ = project;
        }
    }

    order.into_iter().map(|idx| projects[idx].clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, deps: &[&str]) -> Project {
        Project {
            name: name.to_string(),
            path: PathBuf::from(name),
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            dependency_specs: HashMap::new(),
            dependents: vec![],
            package_json_version: None,
        }
    }

    #[test]
    fn topological_sort_orders_dependencies_first() {
        let projects = vec![
            project("app", &["lib"]),
            project("lib", &[]),
            project("e2e", &["app"]),
        ];

        let sorted = topological_sort(&projects);
        let names: Vec<&str> = sorted.iter().map(|p| p.name.as_str()).collect();

        let lib_pos = names.iter().position(|n| *n == "lib").unwrap();
        let app_pos = names.iter().position(|n| *n == "app").unwrap();
        let e2e_pos = names.iter().position(|n| *n == "e2e").unwrap();

        assert!(lib_pos < app_pos);
        assert!(app_pos < e2e_pos);
    }

    #[test]
    fn topological_sort_empty() {
        let sorted = topological_sort(&[]);
        assert!(sorted.is_empty());
    }

    #[test]
    fn topological_sort_handles_cycles() {
        let projects = vec![project("a", &["b"]), project("b", &["a"])];

        let sorted = topological_sort(&projects);
        assert_eq!(sorted.len(), 2);
    }

    #[test]
    fn topological_sort_unknown_deps_ignored() {
        let projects = vec![project("app", &["external-dep"])];

        let sorted = topological_sort(&projects);
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].name, "app");
    }

    #[test]
    fn workspace_lookup_and_relative_path_are_stable() {
        let project = project("packages/app", &[]);
        let workspace = Workspace {
            projects: vec![project.clone()],
            name_to_index: HashMap::from([("packages/app".into(), 0)]),
            path_to_index: HashMap::from([(PathBuf::from("packages/app"), 0)]),
        };
        assert_eq!(
            workspace.get_project_by_name("packages/app").unwrap().name,
            "packages/app"
        );
        assert!(workspace.get_project_by_name("missing").is_none());
        assert_eq!(
            workspace
                .get_project_by_path(Path::new("packages/app"))
                .unwrap()
                .name,
            "packages/app"
        );
        assert_eq!(workspace.project_count(), 1);
        let discovered = DiscoveredWorkspace {
            name: project.name,
            path: PathBuf::from("/tmp/root/packages/app"),
            package_json: PackageJson {
                name: None,
                version: None,
                dependencies: HashMap::new(),
                dev_dependencies: HashMap::new(),
                peer_dependencies: HashMap::new(),
                optional_dependencies: HashMap::new(),
                workspaces: crate::config::WorkspacesField::default(),
            },
            dependencies: vec![],
            dependency_specs: HashMap::new(),
            dependents: vec![],
        };
        assert_eq!(
            discovered.relative_path(Path::new("/tmp/root")),
            PathBuf::from("packages/app")
        );
        assert_eq!(
            discovered.relative_path(Path::new("/other")),
            PathBuf::from("/tmp/root/packages/app")
        );
    }
}
