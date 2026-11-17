//! Leon's shared MCP list (EXT-2, I14): one `.mcp.json`-shaped file in
//! Leon's config folder whose stdio servers reach every CLI's sessions.
//! Leon owns this file; it never writes the CLIs' own configs (P3).

use std::path::{Path, PathBuf};

use leon_core::ports::{PortError, SharedTools};
use leon_domain::McpServer;
use serde_json::{Map, Value, json};

/// The shared list, in `mcp.json` in Leon's config folder.
#[derive(Debug, Clone)]
pub struct SharedFile {
    path: PathBuf,
}

impl SharedFile {
    /// The list kept in the file at `path` (created on the first add).
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read(&self) -> Result<Value, PortError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| {
                PortError::new(
                    "reading the shared MCP list",
                    format!("{}: {e}", self.path.display()),
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({ "mcpServers": {} })),
            Err(e) => Err(PortError::new(
                "reading the shared MCP list",
                format!("{}: {e}", self.path.display()),
            )),
        }
    }

    /// Writes through a temporary file, so a crash never leaves half a file.
    fn write(&self, json: &Value) -> Result<(), PortError> {
        const C: &str = "saving the shared MCP list";
        let fail =
            |e: &dyn std::fmt::Display| PortError::new(C, format!("{}: {e}", self.path.display()));
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| fail(&e))?;
        }
        let text = serde_json::to_string_pretty(json).map_err(|e| fail(&e))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text + "\n").map_err(|e| fail(&e))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| fail(&e))
    }

    /// The `mcpServers` object of `json`, created if missing.
    fn servers(json: &mut Value) -> Result<&mut Map<String, Value>, PortError> {
        if !json["mcpServers"].is_object() {
            json["mcpServers"] = json!({});
        }
        json["mcpServers"].as_object_mut().ok_or_else(|| {
            PortError::new(
                "editing the shared MCP list",
                "the file is not a JSON object",
            )
        })
    }
}

impl SharedTools for SharedFile {
    fn path(&self) -> &Path {
        &self.path
    }

    fn list(&self) -> Result<Vec<McpServer>, PortError> {
        let json = self.read()?;
        let mut servers: Vec<McpServer> =
            json["mcpServers"].as_object().map_or_default(|servers| {
                servers
                    .iter()
                    .filter_map(|(name, entry)| McpServer::from_entry(name, entry))
                    .collect()
            });
        // The file's key order depends on serde_json's features: sort.
        servers.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(servers)
    }

    fn add(&self, server: &McpServer) -> Result<(), PortError> {
        let mut json = self.read()?;
        Self::servers(&mut json)?.insert(server.name.clone(), server.entry());
        self.write(&json)
    }

    fn remove(&self, name: &str) -> Result<(), PortError> {
        let mut json = self.read()?;
        Self::servers(&mut json)?.remove(name);
        self.write(&json)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use leon_core::ports::SharedTools;
    use leon_domain::McpServer;

    use super::SharedFile;

    fn server(name: &str) -> McpServer {
        McpServer {
            name: name.into(),
            command: PathBuf::from("npx"),
            args: vec!["-y".into(), format!("{name}-mcp")],
            env: vec![("TOKEN".into(), "x".into())],
            trusted: false,
        }
    }

    #[test]
    fn add_then_remove_keeps_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("leon").join("mcp.json");
        let file = SharedFile::new(path.clone());
        assert_eq!(file.list().unwrap(), Vec::new());
        file.add(&server("metabase")).unwrap();
        file.add(&server("files")).unwrap();
        assert_eq!(
            file.list().unwrap(),
            vec![server("files"), server("metabase")]
        );
        // Keys Leon does not know survive an edit.
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("{\n  \"mcpServers\"", "{\n  \"note\": 1,\n  \"mcpServers\"");
        std::fs::write(&path, text).unwrap();
        file.remove("files").unwrap();
        assert_eq!(file.list().unwrap(), vec![server("metabase")]);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("\"note\": 1")
        );
    }

    #[test]
    fn a_broken_file_is_an_error_not_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(SharedFile::new(path).list().is_err());
    }
}
