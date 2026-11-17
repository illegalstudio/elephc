//! Purpose:
//! Alerts for `monitor --apm`: budgets on a route's latency, error rate,
//! throughput or queries, checked continuously while the collector runs, with
//! a hook when one starts or stops being broken.
//!
//! Called from:
//! - `apm::collect`, after every poll.
//! - The monitor page (`/api/alerts`) and the `--live` view.
//!
//! Key details:
//! - Same syntax as `--assert`: `<metric>:<route><op><value>`, a rule stating
//!   what healthy looks like (`p95:GET /products<=100`); `*` is every route.
//! - Checked over a sliding window (`--alert-window`, 60 s by default), so one
//!   slow request does not page anyone and a sustained regression does.
//! - A window with no traffic for the rule's route leaves its state as it was:
//!   no data is not a recovery, and not an incident either.

use super::{merge_io, merge_routes, summarize, total_of, Window};

/// What a rule measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Metric {
    /// Latency percentiles and mean, milliseconds.
    P50,
    P95,
    P99,
    Mean,
    /// Share of transactions answered 5xx (or failed runs), percent.
    ErrorPct,
    /// Share of transactions answered 4xx, percent.
    ClientErrorPct,
    /// Transactions per second.
    Rps,
    /// Database statements per transaction.
    Queries,
}

impl Metric {
    /// The metric a rule names (`p95`, `error_pct`, `rps`, ...), or `None`
    /// for a name no rule can measure.
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "p50" => Self::P50,
            "p95" => Self::P95,
            "p99" => Self::P99,
            "mean" => Self::Mean,
            "error_pct" => Self::ErrorPct,
            "client_error_pct" => Self::ClientErrorPct,
            "rps" => Self::Rps,
            "queries" => Self::Queries,
            _ => return None,
        })
    }

    /// The unit a value of this metric reads in.
    pub(crate) fn unit(self) -> &'static str {
        match self {
            Self::P50 | Self::P95 | Self::P99 | Self::Mean => "ms",
            Self::ErrorPct | Self::ClientErrorPct => "%",
            Self::Rps => "/s",
            Self::Queries => " q/req",
        }
    }
}

/// One budget.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Rule {
    /// The rule as written, for messages and hooks.
    pub spec: String,
    pub metric: Metric,
    /// The route it watches; `None` for `*`, every route together.
    pub route: Option<String>,
    pub op: String,
    pub threshold: f64,
}

/// Metric names `--alert` accepts, for the error message.
const METRICS: &str = "p50, p95, p99, mean (ms), error_pct, client_error_pct (%), rps, queries";

/// Parses `<metric>:<route><op><value>`.
pub(crate) fn parse_rule(spec: &str) -> Result<Rule, String> {
    let invalid = || format!("invalid --alert '{spec}': expected <metric>:<route><op><value>, e.g. p95:GET /products<=100");
    let (metric, rest) = spec.split_once(':').ok_or_else(invalid)?;
    // The LAST operator in the rule: route labels such as `<other>` contain
    // `<` and `>` themselves, so the first one found is not the comparison.
    let (position, op) = ["<=", ">=", "==", "<", ">"]
        .iter()
        .filter_map(|op| rest.rfind(op).map(|position| (position, *op)))
        .max_by(|a, b| a.0.cmp(&b.0).then(a.1.len().cmp(&b.1.len())))
        .ok_or_else(invalid)?;
    let route = rest[..position].trim().to_string();
    let threshold: f64 = rest[position + op.len()..].trim().parse().map_err(|_| invalid())?;
    if route.is_empty() || !threshold.is_finite() {
        return Err(invalid());
    }
    let (metric, op) = (metric.trim().to_string(), op.to_string());
    let metric = Metric::parse(&metric)
        .ok_or_else(|| format!("invalid --alert '{spec}': unknown metric '{metric}' (one of {METRICS})"))?;
    Ok(Rule {
        spec: spec.to_string(),
        metric,
        route: (route != "*").then_some(route),
        op,
        threshold,
    })
}

impl Rule {
    /// The rule's metric over `windows`, or `None` when its route had no
    /// transaction in them.
    pub(crate) fn measure(&self, windows: &[&Window]) -> Option<f64> {
        let span_ms = super::measured_ms(windows);
        let rows = merge_routes(windows.iter().copied());
        let io = merge_io(windows.iter().copied());
        let row = match &self.route {
            Some(route) => rows.into_iter().find(|row| &row.name == route),
            None => Some(total_of(&rows)),
        };
        let Some(row) = row.filter(|row| row.count > 0) else {
            // For throughput, no request IS the measurement: a floor such as
            // `rps:*>=1` exists to notice traffic stopping. Every other metric
            // has nothing to say about requests that did not happen.
            return (self.metric == Metric::Rps && span_ms > 0).then_some(0.0);
        };
        let summary = summarize(&row, span_ms, row.sum_ns);
        let ms = |ns: u64| ns as f64 / 1e6;
        Some(match self.metric {
            Metric::P50 => ms(summary.p50_ns),
            Metric::P95 => ms(summary.p95_ns),
            Metric::P99 => ms(summary.p99_ns),
            Metric::Mean => ms(summary.mean_ns),
            Metric::ErrorPct => summary.server_error_pct,
            Metric::ClientErrorPct => summary.client_error_pct,
            Metric::Rps => summary.rps,
            Metric::Queries => {
                let ops: u64 = match &self.route {
                    Some(route) => io.get(route).map_or(0, |io| io.db_ops),
                    None => io.values().map(|io| io.db_ops).sum(),
                };
                ops as f64 / row.count as f64
            }
        })
    }

    /// Whether `value` keeps the budget.
    pub(crate) fn holds(&self, value: f64) -> bool {
        match self.op.as_str() {
            "<=" => value <= self.threshold,
            ">=" => value >= self.threshold,
            "<" => value < self.threshold,
            ">" => value > self.threshold,
            _ => (value - self.threshold).abs() < f64::EPSILON,
        }
    }
}

/// A rule and where it stands.
#[derive(Debug, Clone)]
pub(crate) struct Alert {
    pub rule: Rule,
    /// Whether the budget is broken right now.
    pub firing: bool,
    /// The last measured value, `None` before the route had traffic.
    pub value: Option<f64>,
    /// When it last changed state, Unix milliseconds (0 = never).
    pub since_ms: u64,
}

/// A rule that started or stopped being broken.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Transition {
    pub spec: String,
    pub firing: bool,
    pub value: f64,
    pub unit: &'static str,
}

/// Every rule's state.
#[derive(Debug, Clone, Default)]
pub(crate) struct Alerts {
    pub alerts: Vec<Alert>,
}

impl Alerts {
    /// One alert per rule, each starting resolved with no measured value.
    pub(crate) fn new(rules: Vec<Rule>) -> Self {
        Self {
            alerts: rules
                .into_iter()
                .map(|rule| Alert { rule, firing: false, value: None, since_ms: 0 })
                .collect(),
        }
    }

    /// Re-measures every rule over `windows` and returns the state changes.
    pub(crate) fn check(&mut self, windows: &[&Window], now_ms: u64) -> Vec<Transition> {
        let mut changes = Vec::new();
        for alert in &mut self.alerts {
            let Some(value) = alert.rule.measure(windows) else {
                continue;
            };
            alert.value = Some(value);
            let firing = !alert.rule.holds(value);
            if firing != alert.firing {
                alert.firing = firing;
                alert.since_ms = now_ms;
                changes.push(Transition {
                    spec: alert.rule.spec.clone(),
                    firing,
                    value,
                    unit: alert.rule.metric.unit(),
                });
            }
        }
        changes
    }
}

/// Reports a transition on stderr and runs the `--alert-exec` hook, if any.
///
/// The hook runs through `sh -c` with the transition in its environment —
/// `ELEPHC_ALERT_STATE` (`firing` or `resolved`), `ELEPHC_ALERT_RULE`,
/// `ELEPHC_ALERT_VALUE` and `ELEPHC_ALERT_TARGET` — so a webhook is one `curl`
/// away and no notification service is built into the compiler. It is not
/// waited for: a slow hook must not hold up the poll. `quiet` sends its own
/// output to /dev/null, for a terminal the `--live` view redraws.
pub(crate) fn announce(target: &str, transition: &Transition, hook: Option<&str>, quiet: bool) {
    let state = if transition.firing { "firing" } else { "resolved" };
    let value = format!("{:.2}{}", transition.value, transition.unit);
    eprintln!(
        "elephc monitor: alert {state}: {} — now {value} over the alert window",
        transition.spec
    );
    let Some(hook) = hook else {
        return;
    };
    let mut command = std::process::Command::new("sh");
    command
        .arg("-c")
        .arg(hook)
        .env("ELEPHC_ALERT_STATE", state)
        .env("ELEPHC_ALERT_RULE", &transition.spec)
        .env("ELEPHC_ALERT_VALUE", &value)
        .env("ELEPHC_ALERT_TARGET", target)
        .stdin(std::process::Stdio::null());
    if quiet {
        command.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    }
    let spawned = command.spawn();
    match spawned {
        Ok(mut child) => {
            // Reaped on a thread of its own so it never lingers as a zombie.
            let _ = std::thread::Builder::new()
                .name("elephc-apm-alert-hook".to_string())
                .spawn(move || {
                    let _ = child.wait();
                });
        }
        Err(error) => eprintln!("elephc monitor: cannot run the --alert-exec hook: {error}"),
    }
}
