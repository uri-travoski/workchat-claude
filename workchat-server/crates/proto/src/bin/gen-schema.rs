//! Writes the API JSON Schema. Usage: `gen-schema [output-path]` (default `schema/api.schema.json`).
use std::path::PathBuf;

fn main() {
    let out: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("schema/api.schema.json"));
    let mut schema = serde_json::to_value(proto::api_schema()).expect("schema");
    schema["title"] = "WorkChat API".into();
    schema["x-api-version"] = proto::API_VERSION.into();
    schema["x-min-client-version"] = proto::MIN_CLIENT_VERSION.into();
    if let Some(dir) = out.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = serde_json::to_string_pretty(&schema).expect("json") + "\n";
    std::fs::write(&out, text).expect("write schema");
    println!("wrote {}", out.display());
}
