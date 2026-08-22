use crate::config::{
    Config, DiscoveredWorkspace, NxJson, PackageJson, PnpmWorkspaceYaml, TurboJson, WorkspacesField,
};
use crate::error::Result;
use globset::{Glob, GlobSetBuilder};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
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
}

fn discover_workspaces(config: &Config, cwd: &Path) -> Result<Vec<DiscoveredWorkspace>> {
    let mut workspaces = Vec::new();
    let mut seen_paths = HashSet::new();

    let pnpm_workspaces = discover_pnpm_workspaces(cwd)?;
    let turbo_workspaces = discover_turbo_workspaces(cwd)?;
    let nx_workspaces = discover_nx_workspaces(cwd)?;
    let yarn_workspaces = discover_yarn_workspaces(cwd)?;

    let all_discovered = [
        pnpm_workspaces,
        turbo_workspaces,
        nx_workspaces,
        yarn_workspaces,
    ]
    .concat();

    for ws in all_discovered {
        let relative = ws.path.strip_prefix(cwd).unwrap_or(&ws.path);

        let include = config.workspace.include.is_empty()
            || config.workspace.include.iter().any(|pattern| {
                Glob::new(pattern)
                    .map(|g| g.compile_matcher().is_match(relative))
                    .unwrap_or(false)
            });

        let exclude = config.workspace.exclude.iter().any(|pattern| {
            Glob::new(pattern)
                .map(|g| g.compile_matcher().is_match(relative))
                .unwrap_or(false)
        });

        if include && !exclude && seen_paths.insert(ws.path.clone()) {
            workspaces.push(ws);
        }
    }

    workspaces.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(workspaces)
}

fn discover_pnpm_workspaces(cwd: &Path) -> Result<Vec<DiscoveredWorkspace>> {
    let pnpm_yaml = cwd.join("pnpm-workspace.yaml");
    if !pnpm_yaml.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&pnpm_yaml)?;
    let pnpm_ws: PnpmWorkspaceYaml = serde_yaml::from_str(&content)?;

    let mut workspaces = Vec::new();
    for pattern in pnpm_ws.packages {
        let glob = Glob::new(&pattern)?;
        let matcher = glob.compile_matcher();

        for entry in WalkDir::new(cwd)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.join("package.json").exists() {
                let relative = path.strip_prefix(cwd).unwrap_or(path);
                if matcher.is_match(relative) {
                    workspaces.push(read_workspace(path, cwd)?);
                }
            }
        }
    }

    Ok(workspaces)
}

fn discover_turbo_workspaces(cwd: &Path) -> Result<Vec<DiscoveredWorkspace>> {
    let turbo_json = cwd.join("turbo.json");
    if !turbo_json.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&turbo_json)?;
    let _turbo: TurboJson = serde_json::from_str(&content)?;

    // Turbo normally delegates package membership to the package manager.
    // Reuse the root package.json workspaces instead of walking node_modules,
    // docs, fixtures, and nested unrelated packages.
    let workspaces = discover_yarn_workspaces(cwd)?;
    if !workspaces.is_empty() {
        return Ok(workspaces);
    }

    discover_direct_package_dirs(cwd)
}

fn discover_nx_workspaces(cwd: &Path) -> Result<Vec<DiscoveredWorkspace>> {
    let nx_json = cwd.join("nx.json");
    if !nx_json.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&nx_json)?;
    let nx: NxJson = serde_json::from_str(&content)?;

    if !nx.projects.is_empty() {
        let mut workspaces = Vec::new();
        for project_path in nx.projects.values() {
            let path = cwd.join(project_path);
            if path.join("package.json").exists() {
                workspaces.push(read_workspace(&path, cwd)?);
            }
        }
        return Ok(workspaces);
    }

    let workspaces = discover_yarn_workspaces(cwd)?;
    if !workspaces.is_empty() {
        return Ok(workspaces);
    }

    discover_direct_package_dirs(cwd)
}

fn discover_direct_package_dirs(cwd: &Path) -> Result<Vec<DiscoveredWorkspace>> {
    let mut workspaces = Vec::new();
    for container in ["packages", "apps", "libs"] {
        let root = cwd.join(container);
        if !root.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(root)? {
            let path = entry?.path();
            if path.is_dir() && path.join("package.json").exists() {
                workspaces.push(read_workspace(&path, cwd)?);
            }
        }
    }
    Ok(workspaces)
}

fn discover_yarn_workspaces(cwd: &Path) -> Result<Vec<DiscoveredWorkspace>> {
    let package_json = cwd.join("package.json");
    if !package_json.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&package_json)?;
    let root_pkg: PackageJson = serde_json::from_str(&content)?;

    let patterns: Vec<String> = match root_pkg.workspaces {
        WorkspacesField::Array(arr) => arr,
        WorkspacesField::Object(obj) => obj.packages.unwrap_or_default(),
    };

    if patterns.is_empty() {
        return Ok(vec![]);
    }

    let mut workspaces = Vec::new();
    for pattern in &patterns {
        let glob = Glob::new(pattern)?;
        let matcher = glob.compile_matcher();

        for entry in WalkDir::new(cwd)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.join("package.json").exists() {
                let relative = path.strip_prefix(cwd).unwrap_or(path);
                if matcher.is_match(relative) {
                    workspaces.push(read_workspace(path, cwd)?);
                }
            }
        }
    }

    Ok(workspaces)
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

    let global_matcher = build_global_matcher(global_deps, cwd);

    for file in changed_files {
        let relative = file.strip_prefix(cwd).unwrap_or(file);

        if let Some(matcher) = &global_matcher {
            if matcher.is_match(relative) {
                for idx in 0..workspace.projects.len() {
                    affected_indices.insert(idx);
                }
                return workspace.projects.clone();
            }
        }

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
}
