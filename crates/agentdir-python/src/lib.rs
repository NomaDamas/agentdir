use std::path::PathBuf;

use pyo3::exceptions::{PyFileNotFoundError, PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use agentdir::error::AgentdirError;
use agentdir::types::{
    MappingDirection, MaterializeStrategy, SourcePath, VirtualPath as RustVirtualPath,
};
use agentdir::workspace::Workspace as RustWorkspace;

fn to_py_err(e: AgentdirError) -> PyErr {
    match e {
        AgentdirError::Io(io) => PyIOError::new_err(io.to_string()),
        AgentdirError::EntryNotFound(msg) => PyFileNotFoundError::new_err(msg),
        AgentdirError::EntryExists(msg) => PyValueError::new_err(msg),
        AgentdirError::InvalidPath(msg) => PyValueError::new_err(msg),
        other => PyRuntimeError::new_err(other.to_string()),
    }
}

fn make_vp(s: &str) -> Result<RustVirtualPath, PyErr> {
    RustVirtualPath::new(s).map_err(to_py_err)
}

fn make_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime")
}

#[pyclass]
pub struct Workspace {
    inner: RustWorkspace,
}

#[pymethods]
impl Workspace {
    #[staticmethod]
    #[pyo3(signature = (path, strategy="reflink"))]
    fn init(path: &str, strategy: &str) -> PyResult<Self> {
        let strat = parse_strategy(strategy)?;
        let ws = RustWorkspace::init_with_strategy(PathBuf::from(path), strat).map_err(to_py_err)?;
        Ok(Self { inner: ws })
    }

    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        let ws = RustWorkspace::open(PathBuf::from(path)).map_err(to_py_err)?;
        Ok(Self { inner: ws })
    }

    fn map(&mut self, source: &str, mount: &str) -> PyResult<PyObject> {
        let sp = SourcePath::new(PathBuf::from(source));
        let vp = make_vp(mount)?;
        let rt = make_runtime();
        let summary = rt.block_on(self.inner.map(sp, vp)).map_err(to_py_err)?;
        Python::with_gil(|py| {
            let d = PyDict::new(py);
            d.set_item("entries_added", summary.entries_added)?;
            d.set_item("reflinked", summary.reflinked)?;
            d.set_item("copied", summary.copied)?;
            d.set_item("symlinked", summary.symlinked)?;
            d.set_item("hardlinked", summary.hardlinked)?;
            d.set_item("dirs_created", summary.dirs_created)?;
            d.set_item("errors", summary.errors)?;
            Ok(d.into())
        })
    }

    fn unmap(&mut self, mount: &str) -> PyResult<PyObject> {
        let vp = make_vp(mount)?;
        let summary = self.inner.unmap(&vp).map_err(to_py_err)?;
        Python::with_gil(|py| {
            let d = PyDict::new(py);
            d.set_item("entries_removed", summary.entries_removed)?;
            Ok(d.into())
        })
    }

    fn mv(&mut self, from: &str, to: &str) -> PyResult<()> {
        let fp = make_vp(from)?;
        let tp = make_vp(to)?;
        self.inner.mv(&fp, &tp).map_err(to_py_err)
    }

    fn cp(&mut self, from: &str, to: &str) -> PyResult<()> {
        let fp = make_vp(from)?;
        let tp = make_vp(to)?;
        self.inner.cp(&fp, &tp).map_err(to_py_err)
    }

    fn mkdir(&mut self, path: &str) -> PyResult<()> {
        let vp = make_vp(path)?;
        self.inner.mkdir(&vp).map_err(to_py_err)
    }

    fn rmdir(&mut self, path: &str, recursive: bool) -> PyResult<()> {
        let vp = make_vp(path)?;
        self.inner.rmdir(&vp, recursive).map_err(to_py_err)
    }

    fn rename(&mut self, path: &str, new_name: &str) -> PyResult<()> {
        let vp = make_vp(path)?;
        self.inner.rename(&vp, new_name).map_err(to_py_err)
    }

    fn exists(&self, path: &str) -> PyResult<bool> {
        let vp = make_vp(path)?;
        Ok(self.inner.exists(&vp))
    }

    fn stat(&self, path: &str) -> PyResult<PyObject> {
        let vp = make_vp(path)?;
        let s = self.inner.stat(&vp).map_err(to_py_err)?;
        Python::with_gil(|py| {
            let d = PyDict::new(py);
            d.set_item("virtual_path", s.virtual_path.as_str())?;
            d.set_item("source_path", s.source_path.as_path().to_string_lossy().to_string())?;
            d.set_item("size_bytes", s.size_bytes)?;
            d.set_item("mtime_ns", s.mtime_ns as u64)?;
            d.set_item("entry_type", format!("{:?}", s.entry_type))?;
            d.set_item("materialized", s.materialized)?;
            Ok(d.into())
        })
    }

    fn read_bytes(&self, path: &str) -> PyResult<Vec<u8>> {
        let vp = make_vp(path)?;
        let rt = make_runtime();
        rt.block_on(self.inner.read_bytes(&vp)).map_err(to_py_err)
    }

    fn refresh(&mut self) -> PyResult<PyObject> {
        let rt = make_runtime();
        let summary = rt.block_on(self.inner.refresh()).map_err(to_py_err)?;
        Python::with_gil(|py| {
            let d = PyDict::new(py);
            d.set_item("added", summary.added)?;
            d.set_item("refreshed", summary.refreshed)?;
            d.set_item("removed", summary.removed)?;
            d.set_item("errors", summary.errors.len())?;
            Ok(d.into())
        })
    }

    fn status(&self) -> PyResult<PyObject> {
        let s = self.inner.status();
        Python::with_gil(|py| {
            let d = PyDict::new(py);
            d.set_item("total_entries", s.total_entries)?;
            d.set_item("source_roots", s.source_roots)?;
            d.set_item("materialized_root", s.materialized_root.to_string_lossy().to_string())?;
            d.set_item("last_updated_epoch_secs", s.last_updated_epoch_secs)?;
            Ok(d.into())
        })
    }

    #[pyo3(signature = (reverse=false, relative_to=None))]
    fn export_mapping(
        &self,
        reverse: bool,
        relative_to: Option<&str>,
    ) -> PyResult<PyObject> {
        let direction = if reverse {
            MappingDirection::VirtualToSource
        } else {
            MappingDirection::SourceToVirtual
        };
        let base = relative_to.map(PathBuf::from);
        let mapping = self
            .inner
            .export_mapping(direction, base.as_deref())
            .map_err(to_py_err)?;
        Python::with_gil(|py| {
            let d = PyDict::new(py);
            for (k, v) in &mapping {
                d.set_item(k, v)?;
            }
            Ok(d.into())
        })
    }

    fn map_batch(&mut self, mappings: Vec<(String, String)>) -> PyResult<PyObject> {
        let parsed: Vec<_> = mappings
            .into_iter()
            .map(|(src, virt)| {
                let sp = SourcePath::new(PathBuf::from(src));
                let vp = make_vp(&virt)?;
                Ok((sp, vp))
            })
            .collect::<PyResult<_>>()?;
        let rt = make_runtime();
        let summary = rt
            .block_on(self.inner.map_batch(parsed))
            .map_err(to_py_err)?;
        Python::with_gil(|py| {
            let d = PyDict::new(py);
            d.set_item("entries_added", summary.entries_added)?;
            d.set_item("reflinked", summary.reflinked)?;
            d.set_item("copied", summary.copied)?;
            d.set_item("symlinked", summary.symlinked)?;
            d.set_item("hardlinked", summary.hardlinked)?;
            d.set_item("dirs_created", summary.dirs_created)?;
            Ok(d.into())
        })
    }

    fn list_snapshots(&self) -> PyResult<Vec<String>> {
        self.inner.list_snapshots().map_err(to_py_err)
    }

    fn destroy_snapshot(&self, name: &str) -> PyResult<()> {
        self.inner.destroy_snapshot(name).map_err(to_py_err)
    }
}

fn parse_strategy(s: &str) -> PyResult<MaterializeStrategy> {
    match s {
        "reflink" => Ok(MaterializeStrategy::Reflink),
        "symlink" => Ok(MaterializeStrategy::Symlink),
        "hardlink" => Ok(MaterializeStrategy::Hardlink),
        "virtual" => Ok(MaterializeStrategy::Virtual),
        other => Err(PyValueError::new_err(format!(
            "unknown strategy '{other}'; expected reflink, symlink, hardlink, or virtual"
        ))),
    }
}

#[pymodule]
fn agentdir_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Workspace>()?;
    Ok(())
}
