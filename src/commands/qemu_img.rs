//! QEMU disk image operations
//!
//! Provides wrappers around qemu-img for disk creation, inspection, and resizing.

use anyhow::{bail, Context, Result};
use std::io;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::process::Command;

const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;

/// Relevant metadata reported by `qemu-img info`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskImageInfo {
    pub format: String,
    pub virtual_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QemuImgOutput {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

trait QemuImgRunner {
    fn run(&self, args: &[String]) -> io::Result<QemuImgOutput>;
}

struct SystemQemuImgRunner;

impl QemuImgRunner for SystemQemuImgRunner {
    fn run(&self, args: &[String]) -> io::Result<QemuImgOutput> {
        let output = Command::new("qemu-img").args(args).output()?;
        Ok(QemuImgOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

/// Convert a path to a string, returning an error if the path contains invalid UTF-8
fn path_to_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("Path contains invalid UTF-8: {:?}", path))
}

/// Create a new qcow2 disk image
pub fn create_disk(path: &Path, size: &str) -> Result<()> {
    create_disk_with_format(path, "qcow2", size)
}

/// Create a new disk image in the requested qemu-img format
pub fn create_disk_with_format(path: &Path, format: &str, size: &str) -> Result<()> {
    let path_str = path_to_str(path)?;
    let output = Command::new("qemu-img")
        .args(["create", "-f", format, path_str, size])
        .output()
        .context("Failed to run qemu-img create")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create disk: {}", stderr);
    }

    Ok(())
}

/// Convert a disk image from one format to another (e.g., DMG to qcow2)
#[allow(dead_code)]
pub fn convert_disk(source: &Path, dest: &Path, dest_format: &str) -> Result<()> {
    let source_str = path_to_str(source)?;
    let dest_str = path_to_str(dest)?;
    let output = Command::new("qemu-img")
        .args(["convert", "-O", dest_format, source_str, dest_str])
        .output()
        .context("Failed to run qemu-img convert")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to convert disk: {}", stderr);
    }

    Ok(())
}

/// Detect the format of a disk image (returns format string like "qcow2", "raw", etc.)
pub fn detect_disk_format(path: &Path) -> Option<String> {
    disk_image_info(path).ok().map(|info| info.format)
}

/// Inspect a disk image and return its actual format and virtual capacity.
pub fn disk_image_info(path: &Path) -> Result<DiskImageInfo> {
    disk_image_info_with_runner(path, &SystemQemuImgRunner)
}

fn disk_image_info_with_runner(path: &Path, runner: &impl QemuImgRunner) -> Result<DiskImageInfo> {
    let path_str = path_to_str(path)?;
    let args = vec![
        "info".to_string(),
        "--output=json".to_string(),
        path_str.to_string(),
    ];
    let output = runner.run(&args).context("Failed to run qemu-img info")?;

    if !output.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to inspect disk image: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_disk_info_json(&stdout).context("Invalid qemu-img info output")
}

/// Grow a disk image to an absolute size in GiB.
///
/// Shrinking is deliberately rejected because it can silently destroy guest data.
pub fn resize_disk(path: &Path, new_size_gib: u64) -> Result<()> {
    resize_disk_with_dependencies(path, new_size_gib, &SystemQemuImgRunner, |path| {
        std::fs::metadata(path).map(|metadata| metadata.file_type().is_block_device())
    })
}

#[cfg(test)]
fn resize_disk_with_runner(
    path: &Path,
    new_size_gib: u64,
    runner: &impl QemuImgRunner,
) -> Result<()> {
    resize_disk_with_dependencies(path, new_size_gib, runner, |_| Ok(false))
}

fn resize_disk_with_dependencies(
    path: &Path,
    new_size_gib: u64,
    runner: &impl QemuImgRunner,
    is_block_device: impl FnOnce(&Path) -> io::Result<bool>,
) -> Result<()> {
    if new_size_gib == 0 {
        bail!("Disk size must be greater than zero");
    }
    if path.starts_with("/dev") {
        bail!("Physical disks cannot be resized by VM Curator");
    }
    if is_block_device(path)
        .with_context(|| format!("Failed to inspect disk path: {}", path.display()))?
    {
        bail!("Physical disks cannot be resized by VM Curator");
    }

    let info = disk_image_info_with_runner(path, runner)?;
    validate_growth(info.virtual_size, new_size_gib)?;

    let path_str = path_to_str(path)?;
    let size = format!("{new_size_gib}G");
    let args = vec![
        "resize".to_string(),
        "-f".to_string(),
        info.format,
        path_str.to_string(),
        size,
    ];
    let output = runner.run(&args).context("Failed to run qemu-img resize")?;

    if !output.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to resize disk image: {}", stderr.trim());
    }

    Ok(())
}

fn validate_growth(current_size_bytes: u64, new_size_gib: u64) -> Result<()> {
    let new_size_bytes = new_size_gib
        .checked_mul(BYTES_PER_GIB)
        .context("Requested disk size is too large")?;
    if new_size_bytes <= current_size_bytes {
        let current_gib = current_size_bytes as f64 / BYTES_PER_GIB as f64;
        bail!("New size must be larger than the current size ({current_gib:.2} GiB)");
    }
    Ok(())
}

/// Extract the `format` field from the JSON emitted by `qemu-img info --output=json`.
///
/// Returns `None` if the JSON is malformed or has no string `format` field. Kept
/// separate from [`detect_disk_format`] so the parsing logic is unit-testable
/// without invoking `qemu-img`.
#[cfg(test)]
fn parse_format_from_info_json(stdout: &str) -> Option<String> {
    parse_disk_info_json(stdout).ok().map(|info| info.format)
}

fn parse_disk_info_json(stdout: &str) -> Result<DiskImageInfo> {
    let json: serde_json::Value = serde_json::from_str(stdout)?;
    let format = json["format"]
        .as_str()
        .context("qemu-img output has no format")?
        .to_string();
    let virtual_size = json["virtual-size"]
        .as_u64()
        .context("qemu-img output has no virtual-size")?;
    Ok(DiskImageInfo {
        format,
        virtual_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::path::PathBuf;

    #[derive(Default)]
    struct FakeQemuImgRunner {
        calls: RefCell<Vec<Vec<String>>>,
        responses: RefCell<VecDeque<io::Result<QemuImgOutput>>>,
    }

    impl FakeQemuImgRunner {
        fn with_responses(responses: Vec<io::Result<QemuImgOutput>>) -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                responses: RefCell::new(responses.into()),
            }
        }

        fn success(stdout: &str) -> io::Result<QemuImgOutput> {
            Ok(QemuImgOutput {
                success: true,
                stdout: stdout.as_bytes().to_vec(),
                stderr: Vec::new(),
            })
        }

        fn failure(stderr: &str) -> io::Result<QemuImgOutput> {
            Ok(QemuImgOutput {
                success: false,
                stdout: Vec::new(),
                stderr: stderr.as_bytes().to_vec(),
            })
        }
    }

    impl QemuImgRunner for FakeQemuImgRunner {
        fn run(&self, args: &[String]) -> io::Result<QemuImgOutput> {
            self.calls.borrow_mut().push(args.to_vec());
            self.responses.borrow_mut().pop_front().unwrap_or_else(|| {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "fake qemu-img runner has no response",
                ))
            })
        }
    }

    #[test]
    fn parse_format_qcow2() {
        let json = r#"{"virtual-size":42949672960,"filename":"disk.qcow2","format":"qcow2","actual-size":200704}"#;
        assert_eq!(parse_format_from_info_json(json), Some("qcow2".to_string()));
    }

    #[test]
    fn parse_format_raw() {
        let json = r#"{"format":"raw","virtual-size":1048576}"#;
        assert_eq!(parse_format_from_info_json(json), Some("raw".to_string()));
    }

    #[test]
    fn parse_format_missing_field() {
        let json = r#"{"virtual-size":1048576,"filename":"disk.img"}"#;
        assert_eq!(parse_format_from_info_json(json), None);
    }

    #[test]
    fn parse_format_non_string_field() {
        let json = r#"{"format":123}"#;
        assert_eq!(parse_format_from_info_json(json), None);
    }

    #[test]
    fn parse_format_malformed_json() {
        assert_eq!(parse_format_from_info_json("not json at all"), None);
        assert_eq!(parse_format_from_info_json(""), None);
    }

    #[test]
    fn parse_disk_info_includes_virtual_size() {
        let json = r#"{"virtual-size":42949672960,"format":"raw"}"#;
        assert_eq!(
            parse_disk_info_json(json).unwrap(),
            DiskImageInfo {
                format: "raw".to_string(),
                virtual_size: 40 * BYTES_PER_GIB,
            }
        );
    }

    #[test]
    fn parse_disk_info_requires_virtual_size() {
        assert!(parse_disk_info_json(r#"{"format":"raw"}"#).is_err());
    }

    #[test]
    fn resize_validation_only_allows_growth() {
        let current = 40 * BYTES_PER_GIB;
        assert!(validate_growth(current, 41).is_ok());
        assert!(validate_growth(current, 40).is_err());
        assert!(validate_growth(current, 39).is_err());
    }

    #[test]
    fn resize_validation_rejects_overflow() {
        assert!(validate_growth(1, u64::MAX).is_err());
    }

    #[test]
    fn resize_rejects_zero_without_running_qemu_img() {
        let runner = FakeQemuImgRunner::default();

        let error = resize_disk_with_runner(Path::new("disk.raw"), 0, &runner).unwrap_err();

        assert_eq!(error.to_string(), "Disk size must be greater than zero");
        assert!(runner.calls.borrow().is_empty());
    }

    #[test]
    fn resize_rejects_dev_paths_without_running_qemu_img() {
        let error = resize_disk(Path::new("/dev/example-disk"), 2).unwrap_err();
        assert!(error.to_string().contains("Physical disks"));
    }

    #[test]
    fn resize_rejects_detected_block_devices_without_running_qemu_img() {
        let runner = FakeQemuImgRunner::default();
        let probed = Cell::new(false);

        let error =
            resize_disk_with_dependencies(Path::new("/mapped/example-disk"), 2, &runner, |path| {
                assert_eq!(path, Path::new("/mapped/example-disk"));
                probed.set(true);
                Ok(true)
            })
            .unwrap_err();

        assert!(probed.get());
        assert!(error.to_string().contains("Physical disks"));
        assert!(runner.calls.borrow().is_empty());
    }

    #[test]
    fn resize_stops_when_block_device_probe_fails() {
        let runner = FakeQemuImgRunner::default();

        let error =
            resize_disk_with_dependencies(Path::new("/mapped/example-disk"), 2, &runner, |_| {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "metadata denied",
                ))
            })
            .unwrap_err();

        assert!(error.to_string().contains("Failed to inspect disk path"));
        assert!(runner.calls.borrow().is_empty());
    }

    #[test]
    fn resize_passes_detected_format_and_absolute_size() {
        let runner = FakeQemuImgRunner::with_responses(vec![
            FakeQemuImgRunner::success(r#"{"format":"raw","virtual-size":1073741824}"#),
            FakeQemuImgRunner::success(""),
        ]);

        resize_disk_with_runner(Path::new("/vms/example/disk.raw"), 2, &runner).unwrap();

        assert_eq!(
            *runner.calls.borrow(),
            vec![
                vec!["info", "--output=json", "/vms/example/disk.raw"],
                vec!["resize", "-f", "raw", "/vms/example/disk.raw", "2G"],
            ]
        );
    }

    #[test]
    fn resize_rejects_non_growth_before_resize_command() {
        for requested_size in [1, 2] {
            let runner = FakeQemuImgRunner::with_responses(vec![FakeQemuImgRunner::success(
                r#"{"format":"raw","virtual-size":2147483648}"#,
            )]);

            let error = resize_disk_with_runner(
                Path::new("/vms/example/disk.raw"),
                requested_size,
                &runner,
            )
            .unwrap_err();

            assert!(error.to_string().contains("New size must be larger"));
            assert_eq!(
                *runner.calls.borrow(),
                vec![vec!["info", "--output=json", "/vms/example/disk.raw"]]
            );
        }
    }

    #[test]
    fn resize_stops_when_disk_inspection_fails() {
        let runner =
            FakeQemuImgRunner::with_responses(vec![FakeQemuImgRunner::failure("image is corrupt")]);

        let error =
            resize_disk_with_runner(Path::new("/vms/example/disk.raw"), 2, &runner).unwrap_err();

        assert!(error.to_string().contains("image is corrupt"));
        assert_eq!(
            *runner.calls.borrow(),
            vec![vec!["info", "--output=json", "/vms/example/disk.raw"]]
        );
    }

    #[test]
    fn disk_info_reports_command_failure() {
        let runner =
            FakeQemuImgRunner::with_responses(vec![FakeQemuImgRunner::failure("image is corrupt")]);

        let error = disk_image_info_with_runner(Path::new("disk.raw"), &runner).unwrap_err();

        assert!(error.to_string().contains("image is corrupt"));
    }

    #[test]
    fn resize_reports_command_failure() {
        let runner = FakeQemuImgRunner::with_responses(vec![
            FakeQemuImgRunner::success(r#"{"format":"qcow2","virtual-size":1073741824}"#),
            FakeQemuImgRunner::failure("permission denied"),
        ]);

        let error = resize_disk_with_runner(Path::new("disk.qcow2"), 2, &runner).unwrap_err();

        assert!(error.to_string().contains("permission denied"));
        assert_eq!(runner.calls.borrow()[1][2], "qcow2");
    }

    #[test]
    fn resize_reports_spawn_failure_from_resize_command() {
        let runner = FakeQemuImgRunner::with_responses(vec![
            FakeQemuImgRunner::success(r#"{"format":"raw","virtual-size":1073741824}"#),
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cannot execute qemu-img",
            )),
        ]);

        let error = resize_disk_with_runner(Path::new("disk.raw"), 2, &runner).unwrap_err();

        assert!(error.to_string().contains("Failed to run qemu-img resize"));
        assert_eq!(runner.calls.borrow().len(), 2);
    }

    #[test]
    fn disk_info_reports_spawn_failure() {
        let runner = FakeQemuImgRunner::with_responses(vec![Err(io::Error::new(
            io::ErrorKind::NotFound,
            "qemu-img missing",
        ))]);

        let error = disk_image_info_with_runner(Path::new("disk.raw"), &runner).unwrap_err();

        assert!(error.to_string().contains("Failed to run qemu-img info"));
    }

    #[test]
    #[ignore = "requires the qemu-img executable"]
    fn resize_raw_disk_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let disk = dir.path().join("disk.raw");
        create_disk_with_format(&disk, "raw", "1G").unwrap();

        resize_disk(&disk, 2).unwrap();
        let info = disk_image_info(&disk).unwrap();
        assert_eq!(info.format, "raw");
        assert_eq!(info.virtual_size, 2 * BYTES_PER_GIB);
        assert!(resize_disk(&disk, 1).is_err());
    }

    #[test]
    fn path_to_str_valid_utf8() {
        let path = PathBuf::from("/tmp/disk.qcow2");
        assert_eq!(path_to_str(&path).unwrap(), "/tmp/disk.qcow2");
    }

    #[cfg(unix)]
    #[test]
    fn path_to_str_invalid_utf8_errors() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        // 0xFF is not valid UTF-8.
        let path = PathBuf::from(OsStr::from_bytes(b"/tmp/\xff.img"));
        assert!(path_to_str(&path).is_err());
    }
}
