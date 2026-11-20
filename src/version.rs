use std::{
    collections::BTreeSet,
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use crate::{
    Result,
    project::io_error,
    storage::{is_book_path, storage_error},
};

pub struct Checkpoint {
    pub id: String,
    pub committed_at: String,
    pub created: bool,
}

pub(crate) fn initialize(root: &Path) -> Result<()> {
    if root.join(".git").exists() {
        return Ok(());
    }
    run(root, ["init", "--quiet"])?;
    Ok(())
}

pub(crate) fn checkpoint(root: &Path, message: &str) -> Result<Checkpoint> {
    let staged = changed_paths(run(
        root,
        ["diff", "--cached", "--name-only", "--no-renames", "-z"],
    )?);
    if let Some(path) = staged.iter().find(|path| is_book_path(path)) {
        return Err(storage_error(
            &root.join(path),
            "Die Datei hat eine extern vorgemerkte Fassung",
        ));
    }
    let untracked = changed_paths(run(
        root,
        ["ls-files", "--others", "--exclude-standard", "-z"],
    )?);
    let dirty = changed_paths(run(root, ["ls-files", "--modified", "--deleted", "-z"])?);
    let new_files: BTreeSet<_> = untracked
        .into_iter()
        .filter(|path| is_book_path(path))
        .collect();
    let paths: BTreeSet<_> = dirty
        .into_iter()
        .filter(|path| is_book_path(path))
        .chain(new_files.iter().cloned())
        .collect();
    if !new_files.is_empty() {
        run(
            root,
            ["add", "--intent-to-add", "--"]
                .into_iter()
                .map(OsString::from)
                .chain(
                    new_files
                        .iter()
                        .map(|path| path.as_os_str())
                        .map(OsString::from),
                ),
        )?;
    }
    if !paths.is_empty() {
        run(
            root,
            ["commit", "--only", "--no-gpg-sign", "-m", message, "--"]
                .into_iter()
                .map(OsString::from)
                .chain(
                    paths
                        .iter()
                        .map(|path| path.as_os_str())
                        .map(OsString::from),
                ),
        )?;
    }
    let head = run(root, ["log", "-1", "--format=%H%n%cI"])?;
    let head = String::from_utf8_lossy(&head.stdout);
    let (id, committed_at) = head
        .trim_end()
        .split_once('\n')
        .ok_or_else(|| storage_error(root, "Die Git-Version konnte nicht gelesen werden"))?;
    Ok(Checkpoint {
        id: id.into(),
        committed_at: committed_at.into(),
        created: !paths.is_empty(),
    })
}

fn changed_paths(output: Output) -> Vec<PathBuf> {
    output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| {
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;
                PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
            }
            #[cfg(not(unix))]
            PathBuf::from(String::from_utf8_lossy(bytes).as_ref())
        })
        .collect()
}

fn run(root: &Path, args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>) -> Result<Output> {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let output = command
        .arg("-C")
        .arg(root)
        .args([
            "--literal-pathspecs",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
        .args(args)
        .output()
        .map_err(|error| io_error(root, error))?;
    if !output.status.success() {
        return Err(storage_error(
            root,
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    Ok(output)
}
