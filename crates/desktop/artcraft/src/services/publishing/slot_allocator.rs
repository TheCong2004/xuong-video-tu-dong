use chrono::{Datelike, FixedOffset, NaiveDateTime, NaiveTime, TimeZone, Utc};
use sqlite_tasks::connection::TaskDbConnection;

/// Computes next available slot datetime for a page given its default slot list (e.g. ["08:30", "10:00", "17:00", "22:00"]).
/// Checks existing scheduled publications for that page to avoid slot collision.
pub fn default_reels_slots(count: usize) -> Vec<String> {
  match count {
    3 => vec!["11:30".into(), "19:30".into(), "20:30".into()],
    4 => vec!["11:30".into(), "12:15".into(), "19:30".into(), "20:30".into()],
    _ => vec!["11:30".into(), "12:15".into(), "19:30".into(), "20:15".into(), "21:00".into()],
  }
}

pub async fn allocate_next_available_slot(db: &TaskDbConnection, page_id: &str, default_slots_json: &str) -> i64 {
  let parsed: Vec<String> = serde_json::from_str(default_slots_json).unwrap_or_default();
  let slots = if parsed.is_empty() { default_reels_slots(3) } else { parsed };

  // Scheduler timestamps are stored as UTC, while user-facing windows are
  // fixed Asia/Ho_Chi_Minh (UTC+07:00) and have no DST transitions.
  let timezone = FixedOffset::east_opt(7 * 3600).expect("valid Vietnam offset");
  let now = Utc::now().with_timezone(&timezone);
  let mut target_date = now.date_naive();

  // Query existing scheduled publications for this page in the future
  let scheduled_rows: Vec<(i64,)> = sqlx::query_as("SELECT scheduled_at FROM job_publications WHERE page_id = $1 AND scheduled_at IS NOT NULL AND status IN ('SCHEDULED', 'READY_TO_POST', 'POSTING', 'POSTED')").bind(page_id).fetch_all(db.get_pool()).await.unwrap_or_default();

  let scheduled_timestamps: Vec<i64> = scheduled_rows.into_iter().map(|(t,)| t).collect();

  // Try today and the next 14 days
  for _ in 0..14 {
    for slot_str in &slots {
      let parts: Vec<&str> = slot_str.split(':').collect();
      if parts.len() != 2 {
        continue;
      }
      let hour: u32 = parts[0].trim().parse().unwrap_or(8);
      let min: u32 = parts[1].trim().parse().unwrap_or(0);

      if let Some(naive_time) = NaiveTime::from_hms_opt(hour, min, 0) {
        let naive_dt = NaiveDateTime::new(target_date, naive_time);
        if let Some(local_dt) = timezone.from_local_datetime(&naive_dt).single() {
          let hour_ok = (hour == 11 && min >= 30) || (hour == 12 && min == 15) || (hour >= 19 && hour <= 20) || (hour == 21 && min == 0);
          let ts = local_dt.timestamp();

          // Must be in the future (at least 2 minutes from now)
          if hour_ok && ts > now.timestamp() + 120 {
            // Check collision (allow +/- 5 minutes margin)
            let is_taken = scheduled_timestamps.iter().any(|&existing_ts| (existing_ts - ts).abs() < 300);

            if !is_taken {
              return ts;
            }
          }
        }
      }
    }
    // Move to next day
    target_date = target_date.succ_opt().unwrap_or(target_date);
  }

  // Fallback: 1 hour from now
  now.timestamp() + 3600
}

#[cfg(test)]
mod tests {
  use super::default_reels_slots;

  #[test]
  fn reels_defaults_fit_requested_counts_and_windows() {
    for count in [3, 4, 5] {
      let slots = default_reels_slots(count);
      assert_eq!(slots.len(), count);
      assert!(slots.iter().all(|slot| {
        let (h, m) = slot.split_once(':').and_then(|(h, m)| Some((h.parse::<u32>().ok()?, m.parse::<u32>().ok()?))).unwrap();
        (h == 11 && m >= 30) || (h == 12 && m <= 30) || (h >= 19 && h < 21) || (h == 21 && m == 0)
      }));
    }
  }
}
