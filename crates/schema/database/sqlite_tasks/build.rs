use std::env;
use std::path::PathBuf;

pub fn main() {
  // Development builds should not require a user-specific compile-time
  // SQLite file when the workspace already carries SQLx's offline query
  // metadata.  The runtime connection still creates/migrates its own
  // database (see `connection.rs`); this only affects `query!` expansion.
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let workspace_sqlx = manifest_dir.ancestors().nth(4).map(|root| root.join(".sqlx")).filter(|path| path.is_dir());

  if let Some(sqlx_dir) = workspace_sqlx {
    println!("cargo:rerun-if-changed={}", sqlx_dir.display());
    if env::var_os("SQLX_OFFLINE").is_none() {
      println!("cargo:rustc-env=SQLX_OFFLINE=true");
      println!("cargo:warning=SQLx offline metadata detected; using workspace .sqlx query cache");
    }
  }

  // NB: This should help JetBrains' RustRover from highlighting failing query macros,
  // but we do not want to interfere with the following:
  //
  //    (1) User development builds
  //    (2) Tooling to cache offline queries
  //    (3) CI builds
  let family = env::var("CARGO_CFG_TARGET_FAMILY").ok();

  match family.as_deref() {
    Some("unix") => unix_temp_database_pathing(),
    Some("windows") => windows_temp_database_pathing(),
    _ => {
      println!("cargo:warning=Unsupported target family, using Unix temp database pathing.");
      unix_temp_database_pathing();
    },
  }
}

fn unix_temp_database_pathing() {
  println!("cargo:rustc-env=DATABASE_URL=sqlite:/tmp/tasks.sqlite");
}

fn windows_temp_database_pathing() {
  if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
    let path = PathBuf::from(local_app_data);
    let path = path.join("Temp\\tasks.sqlite");
    let path = path.to_str().expect("path should be valid").to_string();
    println!("cargo:warning=LocalAppData path: {}", path);
    println!("cargo:rustc-env=DATABASE_URL=sqlite:{}", path);
  } else {
    panic!("LOCALAPPDATA environment variable not set");
  }
}
