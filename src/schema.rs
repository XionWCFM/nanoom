use crate::config::Config;
use crate::error::Result;
use schemars::schema_for;
use serde_json::Value;

pub fn generate() -> Result<Value> {
    let schema = schema_for!(Config);
    let mut value = serde_json::to_value(schema)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_schema_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.json");
        generate_to_file(&path).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["title"], "nanoom Configuration");
    }
}
