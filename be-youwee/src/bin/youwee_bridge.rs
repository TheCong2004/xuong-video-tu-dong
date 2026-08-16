use std::process::ExitCode;

const MAX_QUERY_CHARS: usize = 500;
const MAX_SEARCH_RESULTS: u32 = 50;

#[tokio::main]
async fn main() -> ExitCode {
  match run(std::env::args().skip(1).collect()).await {
    Ok(payload) => match serde_json::to_string(&payload) {
      Ok(json) => {
        println!("{json}");
        ExitCode::SUCCESS
      },
      Err(error) => fail(format!("failed to serialize response: {error}")),
    },
    Err(error) => fail(error),
  }
}

async fn run(args: Vec<String>) -> Result<serde_json::Value, String> {
  match args.first().map(String::as_str) {
    Some("health") if args.len() == 1 => Ok(serde_json::json!({
        "status": "ready",
        "service": "youwee",
        "version": env!("CARGO_PKG_VERSION"),
    })),
    Some("search") if (2..=3).contains(&args.len()) => {
      let query = args[1].trim();
      if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
        return Err("query must contain between 1 and 500 characters".to_string());
      }
      let limit = args.get(2).map(|value| value.parse::<u32>()).transpose().map_err(|_| "limit must be an integer".to_string())?.unwrap_or(10);
      if !(1..=MAX_SEARCH_RESULTS).contains(&limit) {
        return Err("limit must be between 1 and 50".to_string());
      }

      let result = app_lib::commands::search_youtube_videos(query.to_string(), Some(limit), None, None).await?;
      serde_json::to_value(result).map_err(|error| format!("failed to serialize Youwee result: {error}"))
    },
    _ => Err("usage: youwee-bridge health | search <query> [limit]".to_string()),
  }
}

fn fail(message: String) -> ExitCode {
  eprintln!("{}", serde_json::json!({"error": message}));
  ExitCode::FAILURE
}
