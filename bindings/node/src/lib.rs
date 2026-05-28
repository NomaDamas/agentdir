#![deny(clippy::all)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use tokio::sync::Mutex;

use agentdir::error::AgentdirError;
use agentdir::types::{
    MappingDirection, MaterializeStrategy, SourcePath, VirtualPath as RustVirtualPath,
};
use agentdir::workspace::Workspace as RustWorkspace;

fn to_napi_err(e: AgentdirError) -> napi::Error {
    match e {
        AgentdirError::Io(io) => {
            napi::Error::new(napi::Status::GenericFailure, format!("IO error: {io}"))
        }
        AgentdirError::EntryNotFound(msg) => {
            napi::Error::new(napi::Status::GenericFailure, format!("Not found: {msg}"))
        }
        AgentdirError::EntryExists(msg) => {
            napi::Error::new(napi::Status::InvalidArg, format!("Already exists: {msg}"))
        }
        AgentdirError::InvalidPath(msg) => {
            napi::Error::new(napi::Status::InvalidArg, format!("Invalid path: {msg}"))
        }
        other => napi::Error::new(napi::Status::GenericFailure, other.to_string()),
    }
}

fn make_vp(s: &str) -> napi::Result<RustVirtualPath> {
    RustVirtualPath::new(s).map_err(to_napi_err)
}

fn parse_strategy(s: &str) -> napi::Result<MaterializeStrategy> {
    match s {
        "reflink" => Ok(MaterializeStrategy::Reflink),
        "symlink" => Ok(MaterializeStrategy::Symlink),
        "hardlink" => Ok(MaterializeStrategy::Hardlink),
        "virtual" => Ok(MaterializeStrategy::Virtual),
        other => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unknown strategy '{other}'; expected reflink, symlink, hardlink, or virtual"),
        )),
    }
}

#[napi(object)]
pub struct MapSummary {
    pub entries_added: u32,
    pub reflinked: u32,
    pub copied: u32,
    pub symlinked: u32,
    pub hardlinked: u32,
    pub dirs_created: u32,
    pub errors: u32,
}

#[napi(object)]
pub struct BatchMapSummary {
    pub entries_added: u32,
    pub reflinked: u32,
    pub copied: u32,
    pub symlinked: u32,
    pub hardlinked: u32,
    pub dirs_created: u32,
    pub errors: Vec<Vec<String>>,
}

#[napi(object)]
pub struct UnmapSummary {
    pub entries_removed: u32,
}

#[napi(object)]
pub struct StatResult {
    pub virtual_path: String,
    pub source_path: String,
    pub size_bytes: i64,
    pub mtime_ns: i64,
    pub entry_type: String,
    pub materialized: bool,
}

#[napi(object)]
pub struct RefreshSummary {
    pub added: u32,
    pub refreshed: u32,
    pub removed: u32,
    pub errors: u32,
}

#[napi(object)]
pub struct StatusResult {
    pub total_entries: u32,
    pub source_roots: u32,
    pub materialized_root: String,
    pub last_updated_epoch_secs: i64,
}

#[napi(js_name = "Workspace")]
pub struct JsWorkspace {
    inner: Arc<Mutex<RustWorkspace>>,
}

#[napi]
impl JsWorkspace {
    /// Initialize a new workspace at the given path.
    ///
    /// @param path - Root directory for the workspace.
    /// @param strategy - Materialization strategy: "reflink" (default), "symlink", "hardlink", or "virtual".
    #[napi(factory)]
    pub fn init(path: String, strategy: Option<String>) -> napi::Result<Self> {
        let strat = parse_strategy(strategy.as_deref().unwrap_or("reflink"))?;
        let ws =
            RustWorkspace::init_with_strategy(PathBuf::from(&path), strat).map_err(to_napi_err)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(ws)),
        })
    }

    /// Open an existing workspace.
    ///
    /// @param path - Root directory of an existing workspace.
    #[napi(factory)]
    pub fn open(path: String) -> napi::Result<Self> {
        let ws = RustWorkspace::open(PathBuf::from(&path)).map_err(to_napi_err)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(ws)),
        })
    }

    /// Map a source directory to a virtual mount point.
    ///
    /// @param source - Source directory path on disk.
    /// @param mount - Virtual mount point (e.g. "/docs").
    /// @returns Summary of the mapping operation.
    #[napi]
    pub async fn map(&self, source: String, mount: String) -> napi::Result<MapSummary> {
        let sp = SourcePath::new(PathBuf::from(&source));
        let vp = make_vp(&mount)?;
        let mut ws = self.inner.lock().await;
        let summary = ws.map(sp, vp).await.map_err(to_napi_err)?;
        Ok(MapSummary {
            entries_added: summary.entries_added as u32,
            reflinked: summary.reflinked as u32,
            copied: summary.copied as u32,
            symlinked: summary.symlinked as u32,
            hardlinked: summary.hardlinked as u32,
            dirs_created: summary.dirs_created as u32,
            errors: summary.errors as u32,
        })
    }

    /// Remove a mapping at the given mount point.
    ///
    /// @param mount - Virtual mount point to unmap.
    /// @returns Summary with entries_removed count.
    #[napi]
    pub async fn unmap(&self, mount: String) -> napi::Result<UnmapSummary> {
        let vp = make_vp(&mount)?;
        let mut ws = self.inner.lock().await;
        let summary = ws.unmap(&vp).map_err(to_napi_err)?;
        Ok(UnmapSummary {
            entries_removed: summary.entries_removed as u32,
        })
    }

    /// Move a virtual entry from one path to another.
    #[napi(js_name = "mv")]
    pub async fn mv(&self, from: String, to: String) -> napi::Result<()> {
        let fp = make_vp(&from)?;
        let tp = make_vp(&to)?;
        let mut ws = self.inner.lock().await;
        ws.mv(&fp, &tp).map_err(to_napi_err)
    }

    /// Copy a virtual entry from one path to another.
    #[napi(js_name = "cp")]
    pub async fn cp(&self, from: String, to: String) -> napi::Result<()> {
        let fp = make_vp(&from)?;
        let tp = make_vp(&to)?;
        let mut ws = self.inner.lock().await;
        ws.cp(&fp, &tp).map_err(to_napi_err)
    }

    /// Create a virtual directory.
    #[napi]
    pub async fn mkdir(&self, path: String) -> napi::Result<()> {
        let vp = make_vp(&path)?;
        let mut ws = self.inner.lock().await;
        ws.mkdir(&vp).map_err(to_napi_err)
    }

    /// Remove a virtual directory.
    ///
    /// @param path - Virtual path to remove.
    /// @param recursive - If true, remove all children recursively.
    #[napi]
    pub async fn rmdir(&self, path: String, recursive: bool) -> napi::Result<()> {
        let vp = make_vp(&path)?;
        let mut ws = self.inner.lock().await;
        ws.rmdir(&vp, recursive).map_err(to_napi_err)
    }

    /// Rename a virtual entry (last path component only).
    #[napi]
    pub async fn rename(&self, path: String, new_name: String) -> napi::Result<()> {
        let vp = make_vp(&path)?;
        let mut ws = self.inner.lock().await;
        ws.rename(&vp, &new_name).map_err(to_napi_err)
    }

    /// Check whether a virtual path exists.
    #[napi]
    pub async fn exists(&self, path: String) -> napi::Result<bool> {
        let vp = make_vp(&path)?;
        let ws = self.inner.lock().await;
        Ok(ws.exists(&vp))
    }

    /// Get metadata for a virtual path.
    #[napi]
    pub async fn stat(&self, path: String) -> napi::Result<StatResult> {
        let vp = make_vp(&path)?;
        let ws = self.inner.lock().await;
        let s = ws.stat(&vp).map_err(to_napi_err)?;
        Ok(StatResult {
            virtual_path: s.virtual_path.as_str().to_string(),
            source_path: s.source_path.as_path().to_string_lossy().to_string(),
            size_bytes: s.size_bytes as i64,
            mtime_ns: s.mtime_ns as i64,
            entry_type: format!("{:?}", s.entry_type),
            materialized: s.materialized,
        })
    }

    /// Read the raw bytes of a file at the given virtual path.
    #[napi]
    pub async fn read_bytes(&self, path: String) -> napi::Result<Buffer> {
        let vp = make_vp(&path)?;
        let ws = self.inner.lock().await;
        let bytes = ws.read_bytes(&vp).await.map_err(to_napi_err)?;
        Ok(bytes.into())
    }

    /// Refresh the workspace by detecting source changes.
    #[napi]
    pub async fn refresh(&self) -> napi::Result<RefreshSummary> {
        let mut ws = self.inner.lock().await;
        let summary = ws.refresh().await.map_err(to_napi_err)?;
        Ok(RefreshSummary {
            added: summary.added as u32,
            refreshed: summary.refreshed as u32,
            removed: summary.removed as u32,
            errors: summary.errors.len() as u32,
        })
    }

    /// Get workspace status summary.
    #[napi]
    pub async fn status(&self) -> napi::Result<StatusResult> {
        let ws = self.inner.lock().await;
        let s = ws.status();
        Ok(StatusResult {
            total_entries: s.total_entries as u32,
            source_roots: s.source_roots as u32,
            materialized_root: s.materialized_root.to_string_lossy().to_string(),
            last_updated_epoch_secs: s.last_updated_epoch_secs as i64,
        })
    }

    /// Export the source-to-virtual (or reverse) path mapping.
    ///
    /// @param reverse - If true, export virtual-to-source mapping.
    /// @param relativeTo - Base path for relativizing source paths.
    #[napi]
    pub async fn export_mapping(
        &self,
        reverse: Option<bool>,
        relative_to: Option<String>,
    ) -> napi::Result<HashMap<String, String>> {
        let direction = if reverse.unwrap_or(false) {
            MappingDirection::VirtualToSource
        } else {
            MappingDirection::SourceToVirtual
        };
        let base = relative_to.map(PathBuf::from);
        let ws = self.inner.lock().await;
        let mapping = ws
            .export_mapping(direction, base.as_deref())
            .map_err(to_napi_err)?;
        Ok(mapping.into_iter().collect())
    }

    /// Map multiple source directories in a single batch.
    ///
    /// @param mappings - Array of [sourcePath, mountPoint] tuples.
    #[napi]
    pub async fn map_batch(&self, mappings: Vec<Vec<String>>) -> napi::Result<BatchMapSummary> {
        let parsed: Vec<_> = mappings
            .into_iter()
            .map(|pair| {
                if pair.len() != 2 {
                    return Err(napi::Error::new(
                        napi::Status::InvalidArg,
                        "each mapping must be a [source, mount] pair",
                    ));
                }
                let sp = SourcePath::new(PathBuf::from(&pair[0]));
                let vp = make_vp(&pair[1])?;
                Ok((sp, vp))
            })
            .collect::<napi::Result<_>>()?;
        let mut ws = self.inner.lock().await;
        let summary = ws.map_batch(parsed).await.map_err(to_napi_err)?;
        Ok(BatchMapSummary {
            entries_added: summary.entries_added as u32,
            reflinked: summary.reflinked as u32,
            copied: summary.copied as u32,
            symlinked: summary.symlinked as u32,
            hardlinked: summary.hardlinked as u32,
            dirs_created: summary.dirs_created as u32,
            errors: summary
                .errors
                .into_iter()
                .map(|(k, v)| vec![k, v])
                .collect(),
        })
    }

    /// Match virtual paths against a glob pattern.
    ///
    /// @param pattern - Glob pattern (e.g. "/docs/*.txt", "/docs/**\/*.md").
    #[napi]
    pub async fn rglob(&self, pattern: String) -> napi::Result<Vec<String>> {
        let ws = self.inner.lock().await;
        let entries = ws.rglob(&pattern).map_err(to_napi_err)?;
        Ok(entries
            .into_iter()
            .map(|e| e.virtual_path.as_str().to_string())
            .collect())
    }

    /// List all snapshot names.
    #[napi]
    pub async fn list_snapshots(&self) -> napi::Result<Vec<String>> {
        let ws = self.inner.lock().await;
        ws.list_snapshots().map_err(to_napi_err)
    }

    /// Destroy a named snapshot.
    #[napi]
    pub async fn destroy_snapshot(&self, name: String) -> napi::Result<()> {
        let ws = self.inner.lock().await;
        ws.destroy_snapshot(&name).map_err(to_napi_err)
    }
}
