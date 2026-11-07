//! Purpose:
//! Parses `elephc test --coverage`'s runtime site counters and renders line
//! coverage as text, LCOV or Cobertura XML against the suite's debug metadata.
//!
//! Called from:
//! - `crate::test_command::run_inner` after a coverage-enabled suite exits.
//!
//! Key details:
//! - The debug runtime prints `elephc-debug-site: <id> <count>` at exit; this
//!   module filters those lines from the program's stderr and aggregates counts.
//! - Metadata sites are one logical guard per (function, source, line). Hits on a
//!   line are summed across its sites, so split functions do not under-report.
//! - The synthesized `.elephc-test.lfc` entry is excluded: it always runs and
//!   says nothing about the user's tests.
//! - LCOV uses `DA:<line>,<hits>`; Cobertura uses one `<class>` per source file
//!   with `<line number="..." hits="..."/>`. Branch data is not emitted because
//!   the compiler records one guard per line, not branch points.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Output format for the coverage report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoverageFormat {
    /// Human-readable text summary.
    Text,
    /// LCOV `.info`/`.lcov` line records.
    Lcov,
    /// Cobertura XML.
    Cobertura,
    /// Self-contained HTML report with per-file line/branch visualization.
    Html,
}

impl CoverageFormat {
    /// Parses an explicit `--coverage-format` value.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "lcov" | "info" => Some(Self::Lcov),
            "cobertura" | "xml" => Some(Self::Cobertura),
            "html" | "htm" => Some(Self::Html),
            _ => None,
        }
    }

    /// Infers a format from a `--coverage-out` path, defaulting to text.
    pub(crate) fn from_path(path: &str) -> Self {
        match Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
        {
            "lcov" | "info" => Self::Lcov,
            "xml" => Self::Cobertura,
            "html" | "htm" => Self::Html,
            _ => Self::Text,
        }
    }
}

/// One source file's line-hit counts, sorted by line.
pub(crate) struct SourceCoverage {
    /// Source id as recorded by the compiler.
    pub(crate) source_id: u64,
    /// Source path as recorded by the compiler.
    pub(crate) path: String,
    /// `(line, hits)` pairs, sorted ascending.
    pub(crate) lines: Vec<(u64, u64)>,
}

/// One source-level branch point and one hit count per outgoing edge.
pub(crate) struct BranchCoverage {
    /// Source owning the branch.
    pub(crate) source_id: u64,
    /// Source line of the branch.
    pub(crate) line: u64,
    /// Hits per edge, in source order: then/else for a conditional, case
    /// labels plus default for a switch.
    pub(crate) edges: Vec<u64>,
}

impl BranchCoverage {
    /// Returns the number of outgoing edges.
    fn total(&self) -> usize {
        self.edges.len()
    }

    /// Returns the number of edges taken at least once.
    fn covered(&self) -> usize {
        self.edges.iter().filter(|hits| **hits > 0).count()
    }

    /// Returns whether the branch point itself was reached.
    fn reached(&self) -> bool {
        self.covered() > 0
    }
}

/// One logical or switch path group and its terminal-route hit counts.
pub(crate) struct PathCoverage {
    /// Source owning the path group.
    pub(crate) source_id: u64,
    /// Source line of the logical condition or switch.
    pub(crate) line: u64,
    /// `logical` or `switch`.
    pub(crate) kind: String,
    /// Whether path planning hit its bounded full-prefix cap.
    pub(crate) truncated: bool,
    /// Terminal routes in metadata order.
    pub(crate) paths: Vec<CoveredPath>,
}

/// One terminal path route and its hit count.
pub(crate) struct CoveredPath {
    /// Display label: terminal condition outcome/position, `caseN` or `default`.
    pub(crate) label: String,
    /// Site-counter hits for the route's terminal edge.
    pub(crate) hits: u64,
}

impl PathCoverage {
    /// Returns the number of routes in this group.
    fn total(&self) -> usize {
        self.paths.len()
    }

    /// Returns the number of routes taken at least once.
    fn covered(&self) -> usize {
        self.paths.iter().filter(|path| path.hits > 0).count()
    }
}

/// Aggregated line, branch and path coverage for one suite.
pub(crate) struct CoverageReport {
    /// User sources with at least one executed guard site.
    pub(crate) sources: Vec<SourceCoverage>,
    /// Source-level conditional branches, in metadata order.
    pub(crate) branches: Vec<BranchCoverage>,
    /// Logical/switch path groups, in metadata order.
    pub(crate) paths: Vec<PathCoverage>,
}

impl CoverageReport {
    /// Returns the number of executable lines in the report.
    pub(crate) fn total(&self) -> usize {
        self.sources.iter().map(|source| source.lines.len()).sum()
    }

    /// Returns the number of lines with at least one hit.
    pub(crate) fn covered(&self) -> usize {
        self.sources
            .iter()
            .map(|source| source.lines.iter().filter(|(_, hits)| *hits > 0).count())
            .sum()
    }

    /// Returns the covered fraction, or `0.0` for an empty report.
    pub(crate) fn ratio(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.covered() as f64 / total as f64
        }
    }

    /// Returns the number of branch edges in the report.
    pub(crate) fn branch_total(&self) -> usize {
        self.branches.iter().map(BranchCoverage::total).sum()
    }

    /// Returns the number of branch edges that were taken at least once.
    pub(crate) fn branch_covered(&self) -> usize {
        self.branches.iter().map(BranchCoverage::covered).sum()
    }

    /// Returns the covered branch-edge fraction, or `0.0` when there are none.
    pub(crate) fn branch_ratio(&self) -> f64 {
        let total = self.branch_total();
        if total == 0 {
            0.0
        } else {
            self.branch_covered() as f64 / total as f64
        }
    }

    /// Returns the branches recorded for one source.
    pub(crate) fn branches_for(&self, source_id: u64) -> impl Iterator<Item = &BranchCoverage> {
        self.branches
            .iter()
            .filter(move |branch| branch.source_id == source_id)
    }

    /// Returns the number of logical/switch paths in the report.
    pub(crate) fn path_total(&self) -> usize {
        self.paths.iter().map(PathCoverage::total).sum()
    }

    /// Returns the number of paths taken at least once.
    pub(crate) fn path_covered(&self) -> usize {
        self.paths.iter().map(PathCoverage::covered).sum()
    }

    /// Returns the covered path fraction, or `0.0` when there are none.
    pub(crate) fn path_ratio(&self) -> f64 {
        let total = self.path_total();
        if total == 0 {
            0.0
        } else {
            self.path_covered() as f64 / total as f64
        }
    }

    /// Returns the path groups recorded for one source.
    pub(crate) fn paths_for(&self, source_id: u64) -> impl Iterator<Item = &PathCoverage> {
        self.paths
            .iter()
            .filter(move |path| path.source_id == source_id)
    }
}

/// Parses site-counter lines from the suite's stderr.
///
/// Returns the per-site counts and every other stderr line unchanged, so program
/// diagnostics and test failures remain visible.
pub(crate) fn parse_site_counts(stderr: &str) -> (HashMap<u32, u64>, Vec<String>) {
    let mut counts = HashMap::new();
    let mut passthrough = Vec::new();
    for line in stderr.lines() {
        if let Some(rest) = line.strip_prefix("elephc-debug-site: ") {
            let mut parts = rest.split_whitespace();
            if let (Some(id), Some(count)) = (parts.next(), parts.next()) {
                if let (Ok(id), Ok(count)) = (id.parse::<u32>(), count.parse::<u64>()) {
                    *counts.entry(id).or_insert(0) += count;
                }
            }
            continue;
        }
        if line.starts_with("elephc-debug-sites: ") {
            continue;
        }
        passthrough.push(line.to_string());
    }
    (counts, passthrough)
}

/// Aggregates the metadata sites and executed counters into one report.
///
/// Returns an error when the metadata has no executable user sites, which means
/// the suite was not built with debug guards.
pub(crate) fn collect(
    metadata: &serde_json::Value,
    counts: &HashMap<u32, u64>,
) -> Result<CoverageReport, String> {
    let sources: HashMap<u64, String> = metadata
        .get("sources")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|source| {
            Some((
                source.get("id")?.as_u64()?,
                source.get("path")?.as_str()?.to_string(),
            ))
        })
        .collect();
    let sites = metadata
        .get("sites")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "debug metadata has no sites table".to_string())?;
    let mut per_source: BTreeMap<u64, BTreeMap<u64, u64>> = BTreeMap::new();
    for site in sites {
        let Some(id) = site.get("id").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        let Some(source_id) = site.get("source_id").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        let Some(line) = site.get("line").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        let hits = counts.get(&(id as u32)).copied().unwrap_or(0);
        *per_source
            .entry(source_id)
            .or_default()
            .entry(line)
            .or_insert(0) += hits;
    }
    let mut sources_out: Vec<SourceCoverage> = per_source
        .into_iter()
        .filter_map(|(source_id, lines)| {
            let path = sources.get(&source_id)?.clone();
            if path.ends_with(".elephc-test.lfc") {
                return None;
            }
            Some(SourceCoverage {
                source_id,
                path,
                lines: lines.into_iter().collect(),
            })
        })
        .collect();
    sources_out.sort_by(|left, right| left.path.cmp(&right.path));
    if sources_out.is_empty() {
        return Err("debug metadata has no executable sites".to_string());
    }
    let user_source_ids: std::collections::HashSet<u64> =
        sources_out.iter().map(|source| source.source_id).collect();
    let mut branches = Vec::new();
    if let Some(branch_table) = metadata
        .get("branches")
        .and_then(serde_json::Value::as_array)
    {
        for branch in branch_table {
            let Some(source_id) = branch.get("source_id").and_then(serde_json::Value::as_u64)
            else {
                continue;
            };
            if !user_source_ids.contains(&source_id) {
                continue;
            }
            let Some(line) = branch.get("line").and_then(serde_json::Value::as_u64) else {
                continue;
            };
            let edges: Vec<u64> = if let Some(edges) =
                branch.get("edges").and_then(serde_json::Value::as_array)
            {
                edges
                    .iter()
                    .map(|site| {
                        site.as_u64()
                            .and_then(|site| counts.get(&(site as u32)).copied())
                            .unwrap_or(0)
                    })
                    .collect()
            } else {
                // v1 conditional metadata predates the general `edges` array.
                let then_site = branch
                    .get("then_site")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let else_site = branch
                    .get("else_site")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                vec![
                    counts.get(&(then_site as u32)).copied().unwrap_or(0),
                    counts.get(&(else_site as u32)).copied().unwrap_or(0),
                ]
            };
            branches.push(BranchCoverage {
                source_id,
                line,
                edges,
            });
        }
    }
    let mut path_groups = Vec::new();
    if let Some(path_table) = metadata
        .get("paths")
        .and_then(serde_json::Value::as_array)
    {
        for group in path_table {
            let Some(source_id) = group.get("source_id").and_then(serde_json::Value::as_u64)
            else {
                continue;
            };
            if !user_source_ids.contains(&source_id) {
                continue;
            }
            let Some(line) = group.get("line").and_then(serde_json::Value::as_u64) else {
                continue;
            };
            let kind = group
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("logical")
                .to_string();
            let truncated = group
                .get("truncated")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let paths: Vec<CoveredPath> = group
                .get("paths")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|path| {
                    let site = path.get("site")?.as_u64()?;
                    Some(CoveredPath {
                        label: path
                            .get("label")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("path")
                            .to_string(),
                        hits: counts.get(&(site as u32)).copied().unwrap_or(0),
                    })
                })
                .collect();
            if !paths.is_empty() {
                path_groups.push(PathCoverage {
                    source_id,
                    line,
                    kind,
                    truncated,
                    paths,
                });
            }
        }
    }
    Ok(CoverageReport {
        sources: sources_out,
        branches,
        paths: path_groups,
    })
}

/// Renders one report in the requested format.
pub(crate) fn render(report: &CoverageReport, format: CoverageFormat) -> String {
    match format {
        CoverageFormat::Text => render_text(report),
        CoverageFormat::Lcov => render_lcov(report),
        CoverageFormat::Cobertura => render_cobertura(report),
        CoverageFormat::Html => render_html(report),
    }
}

/// Renders the human-readable per-source summary and uncovered lines.
fn render_text(report: &CoverageReport) -> String {
    let total = report.total();
    let covered = report.covered();
    let mut out = format!(
        "Coverage: {covered}/{total} lines ({:.1}%)\n",
        report.ratio() * 100.0
    );
    if report.branch_total() > 0 {
        let _ = writeln!(
            out,
            "Branches: {}/{} ({:.1}%)",
            report.branch_covered(),
            report.branch_total(),
            report.branch_ratio() * 100.0
        );
    }
    if report.path_total() > 0 {
        let _ = writeln!(
            out,
            "Paths: {}/{} ({:.1}%)",
            report.path_covered(),
            report.path_total(),
            report.path_ratio() * 100.0
        );
    }
    for source in &report.sources {
        let source_covered = source.lines.iter().filter(|(_, hits)| *hits > 0).count();
        let source_total = source.lines.len();
        let percent = if source_total == 0 {
            0.0
        } else {
            source_covered as f64 * 100.0 / source_total as f64
        };
        let _ = writeln!(
            out,
            "{}: {source_covered}/{source_total} ({percent:.1}%)",
            source.path
        );
        let uncovered: Vec<String> = source
            .lines
            .iter()
            .filter(|(_, hits)| *hits == 0)
            .map(|(line, _)| line.to_string())
            .take(64)
            .collect();
        if !uncovered.is_empty() {
            let _ = writeln!(out, "  uncovered: {}", uncovered.join(", "));
        }
        let branches: Vec<&BranchCoverage> =
            report.branches_for(source.source_id).collect();
        if !branches.is_empty() {
            let edge_total = branch_edges_total(&branches);
            let edge_covered = branch_edges_covered(&branches);
            let _ = writeln!(
                out,
                "  branches: {edge_covered}/{edge_total} ({:.1}%)",
                if edge_total == 0 {
                    0.0
                } else {
                    edge_covered as f64 * 100.0 / edge_total as f64
                }
            );
        }
        let paths: Vec<&PathCoverage> = report.paths_for(source.source_id).collect();
        if !paths.is_empty() {
            let path_total = paths.iter().map(|path| path.total()).sum::<usize>();
            let path_covered = paths.iter().map(|path| path.covered()).sum::<usize>();
            let _ = writeln!(
                out,
                "  paths: {path_covered}/{path_total} ({:.1}%)",
                if path_total == 0 {
                    0.0
                } else {
                    path_covered as f64 * 100.0 / path_total as f64
                }
            );
            let truncated_lines: Vec<String> = paths
                .iter()
                .filter(|path| path.truncated)
                .map(|path| path.line.to_string())
                .collect();
            if !truncated_lines.is_empty() {
                let _ = writeln!(
                    out,
                    "  truncated paths: {} (full-prefix cap reached)",
                    truncated_lines.join(", ")
                );
            }
            let uncovered: Vec<&str> = paths
                .iter()
                .flat_map(|path| &path.paths)
                .filter(|path| path.hits == 0)
                .map(|path| path.label.as_str())
                .take(16)
                .collect();
            if !uncovered.is_empty() {
                let _ = writeln!(out, "  uncovered paths: {}", uncovered.join(", "));
            }
        }
    }
    out
}

/// Renders LCOV line records, one record per source file.
fn render_lcov(report: &CoverageReport) -> String {
    let mut out = String::new();
    for source in &report.sources {
        let _ = writeln!(out, "SF:{}", source.path);
        for (line, hits) in &source.lines {
            let _ = writeln!(out, "DA:{line},{hits}");
        }
        let branches: Vec<&BranchCoverage> = report.branches_for(source.source_id).collect();
        if !branches.is_empty() {
            let mut per_line: BTreeMap<u64, Vec<&BranchCoverage>> = BTreeMap::new();
            for branch in &branches {
                per_line.entry(branch.line).or_default().push(*branch);
            }
            for (line, line_branches) in &per_line {
                for (index, branch) in line_branches.iter().enumerate() {
                    let reached = branch.reached();
                    for (edge_index, hits) in branch.edges.iter().enumerate() {
                        let taken = if reached {
                            hits.to_string()
                        } else {
                            "-".to_string()
                        };
                        let _ = writeln!(
                            out,
                            "BRDA:{line},{index},{edge_index},{taken}"
                        );
                    }
                }
            }
            let _ = writeln!(out, "BRF:{}", branch_edges_total(&branches));
            let _ = writeln!(out, "BRH:{}", branch_edges_covered(&branches));
        }
        let paths: Vec<&PathCoverage> = report.paths_for(source.source_id).collect();
        if !paths.is_empty() {
            let mut index = 0;
            for group in &paths {
                for path in &group.paths {
                    let _ = writeln!(out, "PATH:{},{index},{}", group.line, path.hits);
                    index += 1;
                }
            }
            let path_total = paths.iter().map(|path| path.total()).sum::<usize>();
            let path_covered = paths.iter().map(|path| path.covered()).sum::<usize>();
            let _ = writeln!(out, "PTF:{path_total}");
            let _ = writeln!(out, "PTH:{path_covered}");
        }
        let covered = source.lines.iter().filter(|(_, hits)| *hits > 0).count();
        let _ = writeln!(out, "LF:{}", source.lines.len());
        let _ = writeln!(out, "LH:{covered}");
        out.push_str("end_of_record\n");
    }
    out
}

/// Counts the branch edges taken at least once.
fn branch_edges_covered(branches: &[&BranchCoverage]) -> usize {
    branches.iter().map(|branch| branch.covered()).sum()
}

/// Counts every branch edge in the given branch points.
fn branch_edges_total(branches: &[&BranchCoverage]) -> usize {
    branches.iter().map(|branch| branch.total()).sum()
}

/// Counts the paths taken at least once in one source or line.
fn path_covered(paths: &[&PathCoverage]) -> usize {
    paths.iter().map(|path| path.covered()).sum()
}

/// Counts every path in one source or line.
fn path_total(paths: &[&PathCoverage]) -> usize {
    paths.iter().map(|path| path.total()).sum()
}

/// Renders one Cobertura XML document with one `<class>` per source file.
fn render_cobertura(report: &CoverageReport) -> String {
    let root = common_root(report);
    // An empty common root (all-relative sources) is not valid Cobertura input;
    // `"."` keeps the document usable while `filename` stays relative.
    let source_root = if root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        root.clone()
    };
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let total = report.total();
    let covered = report.covered();
    let total_paths = report.path_total();
    let covered_paths = report.path_covered();
    let coverage_path_attrs = if total_paths > 0 {
        format!(
            " paths-covered=\"{covered_paths}\" paths-valid=\"{total_paths}\" path-rate=\"{:.4}\"",
            report.path_ratio()
        )
    } else {
        String::new()
    };
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" ?>\n");
    out.push_str("<!DOCTYPE coverage SYSTEM \"http://cobertura.sourceforge.net/xml/coverage-04.dtd\">\n");
    let _ = writeln!(
        out,
        "<coverage line-rate=\"{:.4}\" branch-rate=\"{:.4}\" lines-covered=\"{covered}\" lines-valid=\"{total}\" branches-covered=\"{}\" branches-valid=\"{}\"{coverage_path_attrs} complexity=\"0\" version=\"elephc\" timestamp=\"{timestamp}\">",
        report.ratio(),
        report.branch_ratio(),
        report.branch_covered(),
        report.branch_total()
    );
    out.push_str("  <sources>\n");
    let _ = writeln!(
        out,
        "    <source>{}</source>",
        escape_xml(&source_root.to_string_lossy())
    );
    out.push_str("  </sources>\n");
    out.push_str("  <packages>\n");
    let _ = writeln!(
        out,
        "    <package name=\"\" line-rate=\"{:.4}\" branch-rate=\"{:.4}\"{coverage_path_attrs} complexity=\"0\">",
        report.ratio(),
        report.branch_ratio()
    );
    out.push_str("      <classes>\n");
    for source in &report.sources {
        let source_covered = source.lines.iter().filter(|(_, hits)| *hits > 0).count();
        let source_total = source.lines.len();
        let source_rate = if source_total == 0 {
            0.0
        } else {
            source_covered as f64 / source_total as f64
        };
        let source_branches: Vec<&BranchCoverage> =
            report.branches_for(source.source_id).collect();
        let source_branch_total = branch_edges_total(&source_branches);
        let source_branch_covered = branch_edges_covered(&source_branches);
        let source_branch_rate = if source_branch_total == 0 {
            0.0
        } else {
            source_branch_covered as f64 / source_branch_total as f64
        };
        let source_paths: Vec<&PathCoverage> = report.paths_for(source.source_id).collect();
        let source_path_total = path_total(&source_paths);
        let source_path_covered = path_covered(&source_paths);
        let source_path_rate = if source_path_total == 0 {
            0.0
        } else {
            source_path_covered as f64 / source_path_total as f64
        };
        let class_path_attrs = if source_path_total > 0 {
            format!(" path-rate=\"{source_path_rate:.4}\"")
        } else {
            String::new()
        };
        let filename = relative_filename(&source.path, &root);
        let name = Path::new(&source.path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(&source.path);
        let _ = writeln!(
            out,
            "        <class name=\"{}\" filename=\"{}\" line-rate=\"{source_rate:.4}\" branch-rate=\"{source_branch_rate:.4}\"{class_path_attrs} complexity=\"0\">",
            escape_xml(name),
            escape_xml(&filename)
        );
        out.push_str("          <methods/>\n");
        out.push_str("          <lines>\n");
        let mut branches_per_line: BTreeMap<u64, Vec<&BranchCoverage>> = BTreeMap::new();
        for branch in &source_branches {
            branches_per_line.entry(branch.line).or_default().push(*branch);
        }
        let mut paths_per_line: BTreeMap<u64, Vec<&PathCoverage>> = BTreeMap::new();
        for path in &source_paths {
            paths_per_line.entry(path.line).or_default().push(*path);
        }
        for (line, hits) in &source.lines {
            let line_branches = branches_per_line.get(line);
            let line_paths = paths_per_line.get(line);
            if line_branches.is_some() || line_paths.is_some() {
                let edge_total = line_branches.map(|branches| branch_edges_total(branches)).unwrap_or(0);
                let edge_covered = line_branches.map(|branches| branch_edges_covered(branches)).unwrap_or(0);
                let line_path_total = line_paths.map(|paths| path_total(paths)).unwrap_or(0);
                let line_path_covered = line_paths.map(|paths| path_covered(paths)).unwrap_or(0);
                let mut attrs = String::new();
                if line_branches.is_some() {
                    let _ = write!(
                        attrs,
                        " branch=\"true\" condition-coverage=\"{}% ({edge_covered}/{edge_total})\"",
                        percent(edge_covered, edge_total)
                    );
                }
                if line_paths.is_some() {
                    let _ = write!(
                        attrs,
                        " path-coverage=\"{}% ({line_path_covered}/{line_path_total})\"",
                        percent(line_path_covered, line_path_total)
                    );
                }
                let _ = writeln!(
                    out,
                    "            <line number=\"{line}\" hits=\"{hits}\"{attrs}>"
                );
                out.push_str("              <conditions>\n");
                let mut condition_number = 0;
                if let Some(line_branches) = line_branches {
                    for branch in line_branches {
                        let _ = writeln!(
                            out,
                            "                <condition number=\"{condition_number}\" type=\"jump\" coverage=\"{}%\"/>",
                            percent(branch.covered(), branch.total())
                        );
                        condition_number += 1;
                    }
                }
                if let Some(line_paths) = line_paths {
                    for group in line_paths {
                        let _ = writeln!(
                            out,
                            "                <condition number=\"{condition_number}\" type=\"path\" coverage=\"{}%\"/>",
                            percent(group.covered(), group.total())
                        );
                        condition_number += 1;
                    }
                }
                out.push_str("              </conditions>\n");
                out.push_str("            </line>\n");
            } else {
                let _ = writeln!(
                    out,
                    "            <line number=\"{line}\" hits=\"{hits}\"/>"
                );
            }
        }
        out.push_str("          </lines>\n");
        out.push_str("        </class>\n");
    }
    out.push_str("      </classes>\n");
    out.push_str("    </package>\n");
    out.push_str("  </packages>\n");
    out.push_str("</coverage>\n");
    out
}

/// Returns the deepest directory shared by every reported source.
fn common_root(report: &CoverageReport) -> PathBuf {
    let mut root: Option<PathBuf> = None;
    for source in &report.sources {
        let parent = Path::new(&source.path)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        root = Some(match root {
            None => parent,
            Some(current) => common_prefix(&current, &parent),
        });
    }
    root.unwrap_or_default()
}

/// Returns the component-wise common prefix of two paths.
fn common_prefix(left: &Path, right: &Path) -> PathBuf {
    let mut prefix = PathBuf::new();
    for (left, right) in left.components().zip(right.components()) {
        if left != right {
            break;
        }
        prefix.push(left);
    }
    prefix
}

/// Returns `path` relative to `root` when possible, else the original path.
fn relative_filename(path: &str, root: &Path) -> String {
    Path::new(path)
        .strip_prefix(root)
        .unwrap_or_else(|_| Path::new(path))
        .to_string_lossy()
        .into_owned()
}

/// Renders a self-contained HTML report with per-file line/branch rows.
fn render_html(report: &CoverageReport) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let line_total = report.total();
    let line_covered = report.covered();
    let branch_total = report.branch_total();
    let branch_covered = report.branch_covered();
    let total_paths = report.path_total();
    let covered_paths = report.path_covered();
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<title>elephc coverage report</title>\n");
    out.push_str(
        "<style>\n         :root { color-scheme: light dark; }\n         body { font-family: system-ui, -apple-system, Segoe UI, sans-serif; margin: 2rem auto; max-width: 1200px; padding: 0 1rem; line-height: 1.4; }\n         h1 { margin-bottom: .25rem; }\n         .summary { margin: .5rem 0 1rem; }\n         ul.files { margin-bottom: 2rem; }\n         section { margin-top: 2rem; }\n         table.coverage { border-collapse: collapse; width: 100%; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .9rem; }\n         th, td { border-bottom: 1px solid rgba(128,128,128,.25); padding: .15rem .5rem; vertical-align: top; text-align: left; }\n         th { position: sticky; top: 0; background: Canvas; }\n         tr.covered { background: rgba(0, 160, 0, .12); }\n         tr.uncovered { background: rgba(200, 0, 0, .12); }\n         tr.neutral { color: GrayText; }\n         td.line, td.hits { text-align: right; white-space: nowrap; width: 5ch; }\n         td.branches { white-space: nowrap; }\n         td.source code { white-space: pre-wrap; }\n         .edge.hit { color: #0a7a0a; font-weight: 600; }\n         .edge.miss { color: #b00020; font-weight: 600; }\n         .branch { display: inline-block; margin-right: .4rem; }\n         .warning { color: #b00020; }\n         </style>\n",
    );
    out.push_str("</head>\n<body>\n");
    out.push_str("<h1>elephc coverage report</h1>\n");
    let path_summary = if total_paths > 0 {
        format!(
            "Path coverage: <strong>{covered_paths}/{total_paths} ({:.1}%)</strong><br>\n",
            report.path_ratio() * 100.0
        )
    } else {
        String::new()
    };
    let _ = writeln!(
        out,
        "<p class=\"summary\">Line coverage: <strong>{line_covered}/{line_total} ({:.1}%)</strong><br>\n\
         Branch coverage: <strong>{branch_covered}/{branch_total} ({:.1}%)</strong><br>\n\
         {path_summary}\
         <small>generated from the debug site counters at unix time {timestamp}</small></p>",
        report.ratio() * 100.0,
        report.branch_ratio() * 100.0
    );
    out.push_str("<h2>Files</h2>\n<ul class=\"files\">\n");
    for (index, source) in report.sources.iter().enumerate() {
        let source_line_covered = source.lines.iter().filter(|(_, hits)| *hits > 0).count();
        let source_line_total = source.lines.len();
        let source_branches: Vec<&BranchCoverage> =
            report.branches_for(source.source_id).collect();
        let source_branch_total = branch_edges_total(&source_branches);
        let source_branch_covered = branch_edges_covered(&source_branches);
        let source_paths: Vec<&PathCoverage> = report.paths_for(source.source_id).collect();
        let source_path_total = path_total(&source_paths);
        let source_path_covered = path_covered(&source_paths);
        let path_suffix = if source_path_total > 0 {
            format!(", paths {source_path_covered}/{source_path_total}")
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "<li><a href=\"#file-{index}\">{}</a> — lines {source_line_covered}/{source_line_total}, branches {source_branch_covered}/{source_branch_total}{path_suffix}</li>",
            escape_html(&source.path)
        );
    }
    out.push_str("</ul>\n");

    for (index, source) in report.sources.iter().enumerate() {
        let text = std::fs::read_to_string(&source.path).ok();
        let text_lines: Vec<&str> = text
            .as_deref()
            .map(|text| text.lines().collect())
            .unwrap_or_default();
        let hits_by_line: BTreeMap<u64, u64> = source.lines.iter().copied().collect();
        let source_branches: Vec<&BranchCoverage> =
            report.branches_for(source.source_id).collect();
        let mut branches_by_line: BTreeMap<u64, Vec<&BranchCoverage>> = BTreeMap::new();
        for branch in &source_branches {
            branches_by_line.entry(branch.line).or_default().push(*branch);
        }
        let source_line_covered = source.lines.iter().filter(|(_, hits)| *hits > 0).count();
        let source_line_total = source.lines.len();
        let source_branch_total = branch_edges_total(&source_branches);
        let source_branch_covered = branch_edges_covered(&source_branches);
        let source_paths: Vec<&PathCoverage> = report.paths_for(source.source_id).collect();
        let source_path_total = path_total(&source_paths);
        let source_path_covered = path_covered(&source_paths);
        let path_summary = if source_path_total > 0 {
            format!(" · Paths: <strong>{source_path_covered}/{source_path_total}</strong>")
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "<section id=\"file-{index}\">\n<h2>{}</h2>",
            escape_html(&source.path)
        );
        let _ = writeln!(
            out,
            "<p class=\"summary\">Lines: <strong>{source_line_covered}/{source_line_total}</strong> · Branches: <strong>{source_branch_covered}/{source_branch_total}</strong>{path_summary}</p>"
        );
        if text.is_none() {
            out.push_str(
                "<p class=\"warning\">Source file could not be read; only coverage metadata is shown.</p>\n",
            );
        }
        let max_line = text_lines
            .len()
            .max(hits_by_line.keys().next_back().copied().unwrap_or(0) as usize)
            .max(branches_by_line.keys().next_back().copied().unwrap_or(0) as usize)
            .max(1) as u64;
        out.push_str(
            "<table class=\"coverage\">\n<thead><tr><th>Line</th><th>Hits</th><th>Branches</th><th>Source</th></tr></thead>\n<tbody>\n",
        );
        for line in 1..=max_line {
            let hits = hits_by_line.get(&line).copied();
            let line_branches = branches_by_line.get(&line);
            let class = match hits {
                Some(0) => "uncovered",
                Some(_) => "covered",
                None if line_branches.is_some() => {
                    let taken = line_branches
                        .map(|branches| branch_edges_covered(branches) > 0)
                        .unwrap_or(false);
                    if taken {
                        "covered"
                    } else {
                        "uncovered"
                    }
                }
                None => "neutral",
            };
            let hits_text = hits.map(|hits| hits.to_string()).unwrap_or_default();
            let branch_text = line_branches
                .map(|branches| render_branch_cells(branches))
                .unwrap_or_default();
            let source_text = text_lines
                .get(line.saturating_sub(1) as usize)
                .copied()
                .unwrap_or("");
            let _ = writeln!(
                out,
                "<tr class=\"{class}\"><td class=\"line\">{line}</td><td class=\"hits\">{hits_text}</td><td class=\"branches\">{branch_text}</td><td class=\"source\"><code>{}</code></td></tr>",
                escape_html(source_text)
            );
        }
        out.push_str("</tbody>\n</table>\n");
        if !source_paths.is_empty() {
            out.push_str("<h3>Path coverage</h3>\n<table class=\"paths\">\n<thead><tr><th>Line</th><th>Kind</th><th>Covered</th><th>Routes</th></tr></thead>\n<tbody>\n");
            for group in &source_paths {
                let truncated = if group.truncated {
                    " <span class=\"badge\">truncated</span>"
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "<tr><td>{}</td><td>{}</td><td>{}/{} ({:.1}%){truncated}</td><td>",
                    group.line,
                    escape_html(&group.kind),
                    group.covered(),
                    group.total(),
                    if group.total() == 0 {
                        0.0
                    } else {
                        group.covered() as f64 * 100.0 / group.total() as f64
                    }
                );
                for path in &group.paths {
                    let class = if path.hits > 0 { "hit" } else { "miss" };
                    let _ = write!(
                        out,
                        "<span class=\"path {class}\">{}</span>",
                        escape_html(&format!("{} ({})", path.label, path.hits))
                    );
                }
                out.push_str("</td></tr>\n");
            }
            out.push_str("</tbody>\n</table>\n");
        }
        out.push_str("</section>\n");
    }
    out.push_str("</body>\n</html>\n");
    out
}

/// Renders the then/else edge badges for one source line.
fn render_branch_cells(branches: &[&BranchCoverage]) -> String {
    let mut out = String::new();
    for (index, branch) in branches.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(&format!("<span class=\"branch\" title=\"branch {index}\">"));
        if branch.edges.len() == 2 {
            let then_class = if branch.edges[0] > 0 { "hit" } else { "miss" };
            let else_class = if branch.edges[1] > 0 { "hit" } else { "miss" };
            let _ = write!(
                out,
                "<span class=\"edge {then_class}\">T {}</span> <span class=\"edge {else_class}\">E {}</span>",
                branch.edges[0], branch.edges[1]
            );
        } else {
            for (edge_index, hits) in branch.edges.iter().enumerate() {
                let class = if *hits > 0 { "hit" } else { "miss" };
                let _ = write!(
                    out,
                    "<span class=\"edge {class}\">E{edge_index} {hits}</span>"
                );
                if edge_index + 1 < branch.edges.len() {
                    out.push(' ');
                }
            }
        }
        out.push_str("</span>");
    }
    out
}

/// Escapes one value for HTML text or attribute content.
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Returns an integer percentage for a covered/total pair.
fn percent(covered: usize, total: usize) -> usize {
    if total == 0 {
        0
    } else {
        covered * 100 / total
    }
}

/// Escapes one value for Cobertura XML text or attribute content.
fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies profiler lines are consumed while ordinary stderr is preserved.
    #[test]
    fn splits_site_counts_from_program_stderr() {
        let stderr = "warning: user line\n\
                      elephc-debug-site: 3 2\n\
                      elephc-debug-site: 7 1\n\
                      elephc-debug-sites: total=3 sites=2 frame=0x1\n\
                      fail: assertion\n";
        let (counts, passthrough) = parse_site_counts(stderr);
        assert_eq!(counts.get(&3), Some(&2));
        assert_eq!(counts.get(&7), Some(&1));
        assert_eq!(passthrough, ["warning: user line", "fail: assertion"]);
    }

    /// Builds the metadata fixture used by the render tests.
    fn metadata() -> serde_json::Value {
        serde_json::json!({
            "sources": [
                {"id": 0, "path": "main.php"},
                {"id": 1, "path": "lib.php"}
            ],
            "sites": [
                {"id": 1, "source_id": 0, "line": 2},
                {"id": 2, "source_id": 0, "line": 2},
                {"id": 3, "source_id": 0, "line": 3},
                {"id": 4, "source_id": 1, "line": 9}
            ],
            "branches": [
                {"source_id": 0, "line": 2, "column": 1, "then_site": 20, "else_site": 21},
                {"source_id": 0, "line": 2, "column": 8, "then_site": 22, "else_site": 23}
            ]
        })
    }

    /// Verifies split sites on one line aggregate into one text coverage row.
    #[test]
    fn renders_text_line_coverage() {
        let counts = HashMap::from([(1, 1), (2, 1), (4, 1), (20, 1), (21, 1), (22, 1)]);
        let report = collect(&metadata(), &counts).expect("coverage report");
        let text = render(&report, CoverageFormat::Text);
        assert!(text.contains("Coverage: 2/3 lines (66.7%)"), "{text}");
        assert!(text.contains("Branches: 3/4 (75.0%)"), "{text}");
        assert!(text.contains("main.php: 1/2 (50.0%)"), "{text}");
        assert!(text.contains("branches: 3/4 (75.0%)"), "{text}");
        assert!(text.contains("uncovered: 3"), "{text}");
        assert!(text.contains("lib.php: 1/1 (100.0%)"), "{text}");
    }

    /// Verifies LCOV records sum hits per line and carry the expected totals.
    #[test]
    fn renders_lcov_line_records() {
        let counts = HashMap::from([(1, 1), (2, 1), (4, 1), (20, 1), (21, 1), (22, 1)]);
        let report = collect(&metadata(), &counts).expect("coverage report");
        let lcov = render(&report, CoverageFormat::Lcov);
        assert!(lcov.contains("SF:main.php\n"), "{lcov}");
        assert!(lcov.contains("DA:2,2\n"), "{lcov}");
        assert!(lcov.contains("DA:3,0\n"), "{lcov}");
        assert!(lcov.contains("BRDA:2,0,0,1\n"), "{lcov}");
        assert!(lcov.contains("BRDA:2,0,1,1\n"), "{lcov}");
        assert!(lcov.contains("BRDA:2,1,0,1\n"), "{lcov}");
        assert!(lcov.contains("BRDA:2,1,1,0\n"), "{lcov}");
        assert!(lcov.contains("BRF:4\nBRH:3\n"), "{lcov}");
        assert!(lcov.contains("LF:2\nLH:1\nend_of_record\n"), "{lcov}");
        assert!(lcov.contains("SF:lib.php\n"), "{lcov}");
        assert!(lcov.contains("LF:1\nLH:1\nend_of_record\n"), "{lcov}");
        assert!(!lcov.contains("BRDA:9"), "lib.php has no branches: {lcov}");
    }

    /// Verifies a switch branch point records one edge per case plus default,
    /// rather than collapsing to a two-edge conditional.
    #[test]
    fn renders_switch_branch_edges() {
        let metadata = serde_json::json!({
            "sources": [{"id": 0, "path": "main.php"}],
            "sites": [{"id": 1, "source_id": 0, "line": 2}],
            "branches": [
                {"source_id": 0, "line": 2, "kind": "switch", "edges": [10, 11, 12]}
            ]
        });
        let counts = HashMap::from([(1, 1), (10, 1)]);
        let report = collect(&metadata, &counts).expect("coverage report");
        assert_eq!(report.branch_total(), 3);
        assert_eq!(report.branch_covered(), 1);
        let text = render(&report, CoverageFormat::Text);
        assert!(text.contains("Branches: 1/3 (33.3%)"), "{text}");
        assert!(text.contains("branches: 1/3 (33.3%)"), "{text}");
        let lcov = render(&report, CoverageFormat::Lcov);
        assert!(lcov.contains("BRDA:2,0,0,1\n"), "{lcov}");
        assert!(lcov.contains("BRDA:2,0,1,0\n"), "{lcov}");
        assert!(lcov.contains("BRDA:2,0,2,0\n"), "{lcov}");
        assert!(lcov.contains("BRF:3\nBRH:1\n"), "{lcov}");
        let xml = render(&report, CoverageFormat::Cobertura);
        assert!(
            xml.contains("condition-coverage=\"33% (1/3)\""),
            "{xml}"
        );
        assert!(
            xml.contains("<condition number=\"0\" type=\"jump\" coverage=\"33%\"/>"),
            "{xml}"
        );
        let html = render(&report, CoverageFormat::Html);
        assert!(
            html.contains("E0 1") && html.contains("E1 0") && html.contains("E2 0"),
            "{html}"
        );
    }

    /// Verifies an unreached switch branch point reports every edge as `-` in
    /// LCOV while still counting toward BRF.
    #[test]
    fn renders_unreached_switch_edges_as_dashes() {
        let metadata = serde_json::json!({
            "sources": [{"id": 0, "path": "main.php"}],
            "sites": [{"id": 1, "source_id": 0, "line": 2}],
            "branches": [
                {"source_id": 0, "line": 2, "kind": "switch", "edges": [10, 11]}
            ]
        });
        let report = collect(&metadata, &HashMap::new()).expect("coverage report");
        let lcov = render(&report, CoverageFormat::Lcov);
        assert!(lcov.contains("BRDA:2,0,0,-\n"), "{lcov}");
        assert!(lcov.contains("BRDA:2,0,1,-\n"), "{lcov}");
        assert!(lcov.contains("BRF:2\nBRH:0\n"), "{lcov}");
    }

    /// Verifies logical and switch paths render their covered/total counts in
    /// text, LCOV, Cobertura and HTML without changing branch counts.
    #[test]
    fn renders_path_coverage_in_all_formats() {
        let metadata = serde_json::json!({
            "sources": [{"id": 0, "path": "main.php"}],
            "sites": [
                {"id": 1, "source_id": 0, "line": 2},
                {"id": 2, "source_id": 0, "line": 4}
            ],
            "branches": [
                {"source_id": 0, "line": 2, "kind": "switch", "edges": [10, 11, 12]}
            ],
            "paths": [
                {"function_id": 0, "source_id": 0, "line": 2, "column": 1, "kind": "switch", "truncated": false,
                 "paths": [
                    {"site": 10, "label": "case0"},
                    {"site": 11, "label": "case1"},
                    {"site": 12, "label": "default"}
                 ]},
                {"function_id": 0, "source_id": 0, "line": 4, "column": 1, "kind": "logical", "truncated": true,
                 "paths": [
                    {"site": 20, "label": "false@4:1"},
                    {"site": 21, "label": "true@4:9"},
                    {"site": 22, "label": "false@4:9"}
                 ]}
            ]
        });
        let counts = HashMap::from([(1, 1), (2, 1), (10, 1), (20, 1), (22, 1)]);
        let report = collect(&metadata, &counts).expect("coverage report");
        assert_eq!(report.branch_total(), 3, "branch total unchanged");
        assert_eq!(report.branch_covered(), 1, "branch covered unchanged");
        assert_eq!(report.path_total(), 6);
        assert_eq!(report.path_covered(), 3);

        let text = render(&report, CoverageFormat::Text);
        assert!(text.contains("Branches: 1/3 (33.3%)"), "{text}");
        assert!(text.contains("Paths: 3/6 (50.0%)"), "{text}");
        assert!(text.contains("paths: 3/6 (50.0%)"), "{text}");
        assert!(
            text.contains("uncovered paths: case1, default, true@4:9"),
            "{text}"
        );

        let lcov = render(&report, CoverageFormat::Lcov);
        assert!(lcov.contains("BRDA:2,0,0,1\n"), "{lcov}");
        assert!(lcov.contains("PATH:2,0,1\n"), "{lcov}");
        assert!(lcov.contains("PATH:2,1,0\n"), "{lcov}");
        assert!(lcov.contains("PATH:2,2,0\n"), "{lcov}");
        assert!(lcov.contains("PATH:4,3,1\n"), "{lcov}");
        assert!(lcov.contains("PATH:4,4,0\n"), "{lcov}");
        assert!(lcov.contains("PATH:4,5,1\n"), "{lcov}");
        assert!(lcov.contains("PTF:6\nPTH:3\n"), "{lcov}");

        let xml = render(&report, CoverageFormat::Cobertura);
        assert!(xml.contains("path-rate=\"0.5000\""), "{xml}");
        assert!(
            xml.contains("line number=\"2\" hits=\"1\" branch=\"true\" condition-coverage=\"33% (1/3)\" path-coverage=\"33% (1/3)\""),
            "{xml}"
        );
        assert!(
            xml.contains("line number=\"4\" hits=\"1\" path-coverage=\"66% (2/3)\""),
            "{xml}"
        );
        assert!(
            xml.contains("<condition number=\"1\" type=\"path\" coverage=\"33%\"/>"),
            "{xml}"
        );

        let html = render(&report, CoverageFormat::Html);
        assert!(html.contains("Path coverage: <strong>3/6 (50.0%)</strong>"), "{html}");
        assert!(html.contains("paths 3/6"), "{html}");
        assert!(html.contains("class=\"path hit\""), "{html}");
        assert!(html.contains("class=\"path miss\""), "{html}");
        assert!(html.contains("false@4:1"), "{html}");
        assert!(html.contains("case0"), "{html}");
        assert!(
            html.contains("logical</td><td>2/3 (66.7%) <span class=\"badge\">truncated</span></td>"),
            "{html}"
        );
    }

    /// Verifies Cobertura XML carries document totals, one class per source and
    /// per-line hit counts.
    #[test]
    fn renders_cobertura_xml() {
        let counts = HashMap::from([(1, 1), (2, 1), (4, 1), (20, 1), (21, 1), (22, 1)]);
        let report = collect(&metadata(), &counts).expect("coverage report");
        let xml = render(&report, CoverageFormat::Cobertura);
        assert!(xml.starts_with("<?xml version=\"1.0\" ?>"), "{xml}");
        assert!(
            xml.contains("<coverage line-rate=\"0.6667\" branch-rate=\"0.7500\" lines-covered=\"2\" lines-valid=\"3\" branches-covered=\"3\" branches-valid=\"4\""),
            "{xml}"
        );
        assert!(
            xml.contains("<class name=\"main\" filename=\"main.php\" line-rate=\"0.5000\" branch-rate=\"0.7500\""),
            "{xml}"
        );
        assert!(
            xml.contains("<line number=\"2\" hits=\"2\" branch=\"true\" condition-coverage=\"75% (3/4)\">"),
            "{xml}"
        );
        assert!(xml.contains("<condition number=\"0\" type=\"jump\" coverage=\"100%\"/>"), "{xml}");
        assert!(xml.contains("<condition number=\"1\" type=\"jump\" coverage=\"50%\"/>"), "{xml}");
        assert!(xml.contains("<line number=\"3\" hits=\"0\"/>"), "{xml}");
        assert!(xml.contains("<class name=\"lib\" filename=\"lib.php\" line-rate=\"1.0000\" branch-rate=\"0.0000\""), "{xml}");
    }

    /// Verifies an all-relative report still names a usable Cobertura source root.
    #[test]
    fn renders_cobertura_relative_source_root() {
        let counts = HashMap::from([(1, 1)]);
        let report = collect(&metadata(), &counts).expect("coverage report");
        let xml = render(&report, CoverageFormat::Cobertura);
        assert!(xml.contains("<source>.</source>"), "{xml}");
        assert!(xml.contains("filename=\"main.php\""), "{xml}");
    }

    /// Verifies the HTML report renders per-file rows, escaping source text and
    /// marking covered/uncovered lines and branch edges.
    #[test]
    fn renders_html_report() {
        let dir = std::env::temp_dir().join(format!(
            "elephc-coverage-html-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp coverage dir");
        let main = dir.join("main.php");
        std::fs::write(
            &main,
            "<?php\n$x = 1;\nif ($x > 0) {\n    echo $x;\n} else {\n    echo 0;\n}\n",
        )
        .expect("write coverage source");
        let metadata = serde_json::json!({
            "sources": [{"id": 0, "path": main.to_str().unwrap()}],
            "sites": [
                {"id": 1, "source_id": 0, "line": 2},
                {"id": 2, "source_id": 0, "line": 3},
                {"id": 3, "source_id": 0, "line": 4},
                {"id": 4, "source_id": 0, "line": 6}
            ],
            "branches": [
                {"source_id": 0, "line": 3, "column": 1, "then_site": 20, "else_site": 21}
            ]
        });
        let counts = HashMap::from([(1, 1), (2, 1), (3, 1), (20, 1)]);
        let report = collect(&metadata, &counts).expect("coverage report");
        let html = render(&report, CoverageFormat::Html);
        assert!(html.starts_with("<!doctype html>"), "{html}");
        assert!(html.contains("<title>elephc coverage report</title>"), "{html}");
        assert!(html.contains("Line coverage:"), "{html}");
        assert!(html.contains("Branch coverage:"), "{html}");
        assert!(html.contains("class=\"covered\""), "{html}");
        assert!(html.contains("class=\"uncovered\""), "{html}");
        assert!(html.contains("if ($x &gt; 0) {"), "{html}");
        assert!(html.contains("class=\"edge hit\""), "{html}");
        assert!(html.contains("class=\"edge miss\""), "{html}");
        assert!(html.contains("T 1"), "{html}");
        assert!(html.contains("E 0"), "{html}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Verifies Cobertura escapes XML metacharacters in source names.
    #[test]
    fn escapes_cobertura_xml() {
        let metadata = serde_json::json!({
            "sources": [{"id": 0, "path": "a&b.php"}],
            "sites": [{"id": 1, "source_id": 0, "line": 1}]
        });
        let report = collect(&metadata, &HashMap::from([(1, 1)])).expect("coverage report");
        let xml = render(&report, CoverageFormat::Cobertura);
        assert!(xml.contains("filename=\"a&amp;b.php\""), "{xml}");
    }

    /// Verifies explicit format parsing and path inference.
    #[test]
    fn parses_and_infers_coverage_formats() {
        assert_eq!(CoverageFormat::parse("text"), Some(CoverageFormat::Text));
        assert_eq!(CoverageFormat::parse("lcov"), Some(CoverageFormat::Lcov));
        assert_eq!(CoverageFormat::parse("cobertura"), Some(CoverageFormat::Cobertura));
        assert_eq!(CoverageFormat::parse("html"), Some(CoverageFormat::Html));
        assert_eq!(CoverageFormat::parse("htm"), Some(CoverageFormat::Html));
        assert_eq!(CoverageFormat::parse("markdown"), None);
        assert_eq!(CoverageFormat::from_path("report.lcov"), CoverageFormat::Lcov);
        assert_eq!(CoverageFormat::from_path("report.info"), CoverageFormat::Lcov);
        assert_eq!(CoverageFormat::from_path("report.xml"), CoverageFormat::Cobertura);
        assert_eq!(CoverageFormat::from_path("report.html"), CoverageFormat::Html);
        assert_eq!(CoverageFormat::from_path("report.htm"), CoverageFormat::Html);
        assert_eq!(CoverageFormat::from_path("report.txt"), CoverageFormat::Text);
    }
}
