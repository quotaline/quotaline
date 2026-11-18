//! Parses Claude transcript events and estimates their token cost.

use jiff::Timestamp;
use serde::Deserialize;
use std::path::{Component, Path};

mod lines;
pub use lines::{PartialLine, consume_chunk};
mod state;
pub use state::{LocalTranscriptUsage, TranscriptAnalytics, TranscriptOffset, TranscriptState};

pub const ANALYTICS_METRICS: [&str; 10] = [
    "inputTokens",
    "cachedInputTokens",
    "outputTokens",
    "cacheWriteTokens",
    "costUSD",
    "messages",
    "sessions",
    "toolCalls",
    "projectCost",
    "projectMessages",
];

pub fn pricing_warnings(models: &[String]) -> Vec<String> {
    models
        .iter()
        .map(|model| format!("Cost for {model} is estimated; the pricing table has no entry"))
        .collect()
}

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    uuid: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    #[serde(rename = "sessionId")]
    session: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "costUSD")]
    cost: Option<f64>,
    cwd: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
    content: Option<Vec<Block>>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cache_creation_input_tokens: Option<i64>,
    cache_read_input_tokens: Option<i64>,
    cache_creation: Option<CacheCreation>,
    speed: Option<String>,
}

#[derive(Deserialize)]
struct CacheCreation {
    ephemeral_1h_input_tokens: Option<i64>,
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Record {
    pub id: String,
    pub timestamp: i64,
    pub day: String,
    pub session: String,
    pub model: String,
    pub project: Option<String>,
    pub input: i64,
    pub output: i64,
    pub cache_write: i64,
    pub cache_read: i64,
    pub cache_write_one_hour: Option<i64>,
    pub speed: Option<String>,
    pub tool_calls: i64,
    pub reported_cost: Option<f64>,
    pub cost: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub write: f64,
    pub read: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TokenCounts {
    pub input: i64,
    pub output: i64,
    pub cache_write: i64,
    pub cache_read: i64,
    pub cache_write_one_hour: i64,
}

pub fn price(model: &str, prompt_tokens: i64) -> Option<Price> {
    let model = model.to_ascii_lowercase();
    let entries = [
        ("claude-fable-5-1", 10.0, 50.0, 12.5, 0.25),
        ("claude-mythos-5-1", 10.0, 50.0, 12.5, 0.25),
        ("claude-fable-5", 10.0, 50.0, 12.5, 1.0),
        ("claude-mythos-5", 10.0, 50.0, 12.5, 1.0),
        ("claude-opus-5-5", 4.0, 20.0, 5.0, 0.2),
        ("claude-opus-5", 5.0, 25.0, 6.25, 0.5),
        ("claude-opus-4-8", 5.0, 25.0, 6.25, 0.5),
        ("claude-opus-4-7", 5.0, 25.0, 6.25, 0.5),
        ("claude-opus-4-6", 5.0, 25.0, 6.25, 0.5),
        ("claude-opus-4-5", 5.0, 25.0, 6.25, 0.5),
        ("claude-sonnet-5-5", 2.0, 10.0, 2.5, 0.2),
        ("claude-sonnet-5", 2.0, 10.0, 2.5, 0.2),
        ("claude-sonnet-4-6", 3.0, 15.0, 3.75, 0.3),
        ("claude-sonnet-4-5", 3.0, 15.0, 3.75, 0.3),
        ("claude-haiku-4-5", 1.0, 5.0, 1.25, 0.1),
    ];
    if model.starts_with("claude-haiku-5-5") {
        return Some(if prompt_tokens > 100_000 {
            Price {
                input: 0.5,
                output: 2.5,
                write: 0.625,
                read: 0.05,
            }
        } else {
            Price {
                input: 0.1,
                output: 0.5,
                write: 0.125,
                read: 0.01,
            }
        });
    }
    let family = model.strip_prefix("claude-").and_then(|name| name.split('-').next());
    entries
        .iter()
        .find(|(prefix, _, _, _, _)| model.starts_with(prefix))
        .or_else(|| {
            family.and_then(|family| {
                entries
                    .iter()
                    .filter(|(prefix, _, _, _, _)| prefix.split('-').nth(1) == Some(family))
                    .max_by_key(|(prefix, _, _, _, _)| prefix)
            })
        })
        .map(|(_, input, output, write, read)| Price {
            input: *input,
            output: *output,
            write: *write,
            read: *read,
        })
}

pub fn cost(model: &str, usage: TokenCounts, speed: Option<&str>) -> Option<f64> {
    let model = model.to_ascii_lowercase();
    let prompt_tokens = usage
        .input
        .saturating_add(usage.cache_write)
        .saturating_add(usage.cache_read);
    let price = price(&model, prompt_tokens)?;
    let premium =
        if speed == Some("fast") && (model.starts_with("claude-opus-5") || model.starts_with("claude-opus-4-8")) {
            2.0
        } else {
            1.0
        };
    Some(
        ((usage.input as f64 * price.input)
            + (usage.output as f64 * price.output)
            + (usage.cache_write as f64 * price.write)
            + (usage.cache_read as f64 * price.read)
            + (usage.cache_write_one_hour as f64 * (2.0 * price.input - price.write)))
            / 1_000_000.0
            * premium,
    )
}

fn project_path(path: String) -> String {
    if path.starts_with('/') {
        path_clean::clean(&path).to_string_lossy().into_owned()
    } else {
        path
    }
}

pub fn encoded_project_name(name: &str) -> String {
    format!("directory:{name}")
}

pub fn project_name(transcript: &Path, root: &Path) -> String {
    if let Ok(relative) = transcript.strip_prefix(root) {
        let mut components = relative.components();
        if let (Some(Component::Normal(directory)), Some(_)) = (components.next(), components.next()) {
            return encoded_project_name(&directory.to_string_lossy());
        }
    }
    transcript
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn parse(line: &[u8]) -> Option<Record> {
    let line: Line = serde_json::from_slice(line).ok()?;
    if line.kind.as_deref() != Some("assistant") {
        return None;
    }
    let message = line.message?;
    let usage = message.usage?;
    let model = message.model?;
    let date = line.timestamp.as_deref()?.parse::<Timestamp>().ok()?;
    let input = usage.input_tokens.unwrap_or(0).max(0);
    let output = usage.output_tokens.unwrap_or(0).max(0);
    let cache_write = usage.cache_creation_input_tokens.unwrap_or(0).max(0);
    let cache_read = usage.cache_read_input_tokens.unwrap_or(0).max(0);
    let one_hour = usage
        .cache_creation
        .and_then(|creation| creation.ephemeral_1h_input_tokens)
        .map(|tokens| tokens.clamp(0, cache_write));
    let estimate = cost(
        &model,
        TokenCounts {
            input,
            output,
            cache_write,
            cache_read,
            cache_write_one_hour: one_hour.unwrap_or(0),
        },
        usage.speed.as_deref(),
    );
    let reported_cost = line.cost.filter(|cost| *cost >= 0.0);
    Some(Record {
        id: format!(
            "{}:{}",
            message.id.or(line.uuid).unwrap_or_default(),
            line.request_id.unwrap_or_default()
        ),
        timestamp: date.as_second(),
        day: date.strftime("%Y-%m-%d").to_string(),
        session: line.session.unwrap_or_default(),
        model,
        project: line.cwd.map(project_path),
        input,
        output,
        cache_write,
        cache_read,
        cache_write_one_hour: one_hour,
        speed: usage.speed,
        tool_calls: message
            .content
            .unwrap_or_default()
            .iter()
            .filter(|block| block.kind.as_deref() == Some("tool_use"))
            .count() as i64,
        reported_cost,
        cost: reported_cost.or(estimate),
    })
}

#[cfg(test)]
#[path = "../tests/unit/claude_transcripts/tests.rs"]
mod tests;
