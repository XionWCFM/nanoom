use crate::config::Config;
use crate::error::{Error, Result};
use schemars::schema_for;
use serde_json::Value;

pub fn generate() -> Result<Value> {
    let schema = schema_for!(Config);
    let mut value =
        serde_json::to_value(schema).map_err(|e| Error::SchemaGeneration(e.to_string()))?;

    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "$schema".to_string(),
            Value::String("http://json-schema.org/draft-07/schema#".to_string()),
        );
        obj.insert(
            "title".to_string(),
            Value::String("nanoom Configuration".to_string()),
        );
        obj.insert(
            "description".to_string(),
            Value::String("Configuration for nanoom monorepo task runner".to_string()),
        );
    }

    Ok(value)
}

pub fn generate_to_file(path: &std::path::Path) -> Result<()> {
    let schema = generate()?;
    let content = serde_json::to_string_pretty(&schema)?;
    std::fs::write(path, content)?;
    Ok(())
}
