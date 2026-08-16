use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Deserialize;
use vynaro_detect::Ffmpeg;
use vynaro_domain::ExportStrategy;
use vynaro_intake::{build_plans, PlanOptions};

const MAX_REQUEST_BYTES: u64 = 256 * 1024;
const MAX_SOURCES: usize = 500;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanRequest {
  sources: Vec<String>,
  strategy: ExportStrategy,
  #[serde(default = "default_base_name")]
  base_name: String,
  series_context: Option<String>,
  #[serde(default = "default_episode_template")]
  episode_template: String,
}

#[derive(Debug, Deserialize)]
struct ProbeRequest {
  path: String,
}

fn default_base_name() -> String {
  "output".to_owned()
}

fn default_episode_template() -> String {
  "Episode {n}".to_owned()
}

#[tokio::main]
async fn main() -> ExitCode {
  match run(std::env::args().skip(1).collect()).await {
    Ok(payload) => match serde_json::to_string(&payload) {
      Ok(json) => {
        println!("{json}");
        ExitCode::SUCCESS
      },
      Err(_) => fail("failed to serialize Vynaro response"),
    },
    Err(message) => fail(&message),
  }
}

async fn run(args: Vec<String>) -> Result<serde_json::Value, String> {
  match args.as_slice() {
    [command] if command == "health" => {
      let media_available = Ffmpeg::is_available();
      Ok(serde_json::json!({
        "status": if media_available { "ready" } else { "degraded" },
        "service": "vynaro",
        "version": env!("CARGO_PKG_VERSION"),
        "capabilities": ["video-plan", "probe"],
        "ffmpegAvailable": media_available,
      }))
    },
    [command] if command == "plan" => plan_from_stdin(),
    [command] if command == "probe" => probe_from_stdin().await,
    _ => Err("usage: vynaro-bridge health | plan | probe".to_owned()),
  }
}

async fn probe_from_stdin() -> Result<serde_json::Value, String> {
  let input = read_stdin()?;
  let request: ProbeRequest = serde_json::from_str(&input).map_err(|_| "invalid probe request".to_owned())?;
  let path = PathBuf::from(request.path.trim());
  if request.path.trim().is_empty() || !path.is_file() {
    return Err("input video was not found".to_owned());
  }
  let ffmpeg = Ffmpeg::discover().map_err(|error| error.to_string())?;
  let probe = ffmpeg.probe(&path).await.map_err(|error| error.to_string())?;
  Ok(serde_json::json!({"probe": probe}))
}

fn read_stdin() -> Result<String, String> {
  let mut input = String::new();
  std::io::stdin().take(MAX_REQUEST_BYTES + 1).read_to_string(&mut input).map_err(|_| "failed to read request".to_owned())?;
  if input.len() as u64 > MAX_REQUEST_BYTES {
    return Err("request is too large".to_owned());
  }
  Ok(input)
}

fn plan_from_stdin() -> Result<serde_json::Value, String> {
  let input = read_stdin()?;
  let request: PlanRequest = serde_json::from_str(&input).map_err(|_| "invalid plan request".to_owned())?;
  if request.sources.is_empty() || request.sources.len() > MAX_SOURCES {
    return Err("sources must contain between 1 and 500 paths".to_owned());
  }
  if request.sources.iter().any(|source| source.trim().is_empty()) {
    return Err("source paths must not be empty".to_owned());
  }

  let sources: Vec<PathBuf> = request.sources.into_iter().map(PathBuf::from).collect();
  let options = PlanOptions { base_name: request.base_name, series_context: request.series_context, episode_template: request.episode_template };
  let plans = build_plans(&sources, request.strategy, &options).map_err(|error| error.to_string())?;
  Ok(serde_json::json!({"plans": plans}))
}

fn fail(message: &str) -> ExitCode {
  eprintln!("{}", serde_json::json!({"error": message}));
  ExitCode::FAILURE
}
