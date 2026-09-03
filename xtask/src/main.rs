#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

#[cfg(target_os = "linux")]
use std::os::unix::fs::{PermissionsExt, symlink};

const DEBIAN_PACKAGE_NAME: &str = "slate-browser";
const DEBIAN_APP_ID: &str = "io.github.gabrieldiego.Slate";

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("check") => run_cargo(&["check", "--workspace"]),
        Some("test") => run_cargo_workspace_tests(),
        Some("run") => run_cargo(&["run", "-p", "slate"]),
        Some("fmt") => run_cargo_without_jobs(&["fmt", "--all"]),
        Some("chrome-snapshot") => chrome_snapshot(),
        Some("chrome-verify") => chrome_verify(),
        Some("package-deb") => package_deb(),
        Some("snapshot") => snapshot(),
        Some("snapshot-html") => snapshot_html(),
        Some("snapshot-local") => snapshot_local(),
        _ => {
            eprintln!(
                "usage: cargo run -p xtask -- <check|test|run|fmt|chrome-snapshot|chrome-verify|package-deb|snapshot|snapshot-html|snapshot-local>"
            );
            ExitCode::from(2)
        }
    }
}

fn package_deb() -> ExitCode {
    match try_package_deb() {
        Ok(package) => {
            println!("wrote {}", package.display());
            println!("wrote {}.sha256", package.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to package Slate: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn try_package_deb() -> Result<PathBuf, String> {
    Err("Debian packages must be built on the targeted Ubuntu Linux architecture".to_owned())
}

#[cfg(target_os = "linux")]
fn try_package_deb() -> Result<PathBuf, String> {
    verify_ubuntu_2604_host()?;
    for tool in ["dpkg-deb", "ldd", "md5sum", "readelf", "sha256sum", "strip"] {
        require_tool(tool)?;
    }

    let root = workspace_root();
    if root.to_string_lossy().chars().any(char::is_whitespace) {
        return Err(format!(
            "the workspace path cannot contain whitespace for the package build: {}",
            root.display()
        ));
    }

    let target_dir = root.join("target/slate-deb");
    build_debian_binary(&root, &target_dir)?;

    let production_dir = target_dir.join("production");
    let executable = production_dir.join("slate");
    let chrome_library = production_dir.join("libslate_chrome.so");
    require_file(&executable, "packaged Slate executable")?;
    require_file(&chrome_library, "embedded Servo/Slate chrome library")?;

    let package_root = root.join("target/package/deb/slate-browser");
    if package_root.exists() {
        fs::remove_dir_all(&package_root).map_err(|error| {
            format!(
                "could not clear package staging directory {}: {error}",
                package_root.display()
            )
        })?;
    }

    let private_lib_dir = package_root.join("usr/lib/slate");
    let binary_dir = package_root.join("usr/bin");
    let resource_dir = private_lib_dir.join("resources/resource_protocol");
    let applications_dir = package_root.join("usr/share/applications");
    let metainfo_dir = package_root.join("usr/share/metainfo");
    let documentation_dir = package_root.join("usr/share/doc/slate-browser");
    fs::create_dir_all(&binary_dir).map_err(display_io("create package binary directory"))?;
    fs::create_dir_all(&private_lib_dir)
        .map_err(display_io("create package private library directory"))?;
    fs::create_dir_all(&applications_dir)
        .map_err(display_io("create package application directory"))?;
    fs::create_dir_all(&metainfo_dir).map_err(display_io("create package metadata directory"))?;
    fs::create_dir_all(&documentation_dir)
        .map_err(display_io("create package documentation directory"))?;

    let packaged_executable = private_lib_dir.join("slate");
    let packaged_chrome_library = private_lib_dir.join("libslate_chrome.so");
    copy_file(&executable, &packaged_executable)?;
    copy_file(&chrome_library, &packaged_chrome_library)?;
    copy_shared_rust_standard_library(&root, &private_lib_dir)?;
    copy_directory(&root.join("resources/resource_protocol"), &resource_dir)?;

    let packaging_dir = root.join("packaging/debian");
    let desktop_file = applications_dir.join(format!("{DEBIAN_APP_ID}.desktop"));
    let metainfo_file = metainfo_dir.join(format!("{DEBIAN_APP_ID}.metainfo.xml"));
    copy_file(&packaging_dir.join("slate.desktop"), &desktop_file)?;
    copy_file(&packaging_dir.join("slate.metainfo.xml"), &metainfo_file)?;
    copy_file(
        &packaging_dir.join("copyright"),
        &documentation_dir.join("copyright"),
    )?;

    for (size, source_name) in [
        (32, "slate-logo-32.png"),
        (64, "slate-logo-64.png"),
        (128, "slate-logo-128.png"),
        (256, "slate-logo-cutout-256.png"),
    ] {
        let icon_dir = package_root.join(format!("usr/share/icons/hicolor/{size}x{size}/apps"));
        fs::create_dir_all(&icon_dir).map_err(display_io("create package icon directory"))?;
        copy_file(
            &root.join("crates/chrome/assets/branding").join(source_name),
            &icon_dir.join("slate.png"),
        )?;
    }

    symlink("../lib/slate/slate", binary_dir.join("slate"))
        .map_err(display_io("create /usr/bin/slate package symlink"))?;
    normalize_package_modes(&package_root)?;
    set_mode(&packaged_executable, 0o755)?;
    set_mode(&packaged_chrome_library, 0o755)?;
    for entry in fs::read_dir(&private_lib_dir).map_err(display_io("read private libraries"))? {
        let path = entry
            .map_err(display_io("read private library entry"))?
            .path();
        if path.extension().is_some_and(|extension| extension == "so") {
            set_mode(&path, 0o755)?;
        }
    }

    strip_package_binaries(&private_lib_dir)?;
    verify_elf_layout(&root, &packaged_executable, &packaged_chrome_library)?;
    validate_desktop_metadata(&desktop_file, &metainfo_file)?;

    let control_dir = package_root.join("DEBIAN");
    fs::create_dir_all(&control_dir).map_err(display_io("create package control directory"))?;
    let version = debian_version(&root)?;
    let architecture = debian_architecture()?;
    let installed_size = installed_size_kib(&package_root)?;
    let control = debian_control(&version, architecture, installed_size);
    let control_file = control_dir.join("control");
    let md5sums_file = control_dir.join("md5sums");
    fs::write(&control_file, control).map_err(display_io("write Debian control file"))?;
    write_md5sums(&package_root, &md5sums_file)?;
    set_mode(&control_dir, 0o755)?;
    set_mode(&control_file, 0o644)?;
    set_mode(&md5sums_file, 0o644)?;

    let dist_dir = root.join("dist");
    fs::create_dir_all(&dist_dir).map_err(display_io("create package output directory"))?;
    let package = dist_dir.join(format!(
        "{DEBIAN_PACKAGE_NAME}_{version}_{architecture}.deb"
    ));
    if package.exists() {
        fs::remove_file(&package).map_err(display_io("replace existing Debian package"))?;
    }

    run_required(
        Command::new("dpkg-deb")
            .args(["--root-owner-group", "--build"])
            .arg(&package_root)
            .arg(&package),
        "dpkg-deb",
    )?;
    run_required(
        Command::new("dpkg-deb").arg("--info").arg(&package),
        "dpkg-deb --info",
    )?;
    write_sha256(&package)?;
    verify_sha256(&package)?;
    Ok(package)
}

#[cfg(target_os = "linux")]
fn verify_ubuntu_2604_host() -> Result<(), String> {
    let os_release = fs::read_to_string("/etc/os-release")
        .map_err(|error| format!("could not read /etc/os-release: {error}"))?;
    let is_ubuntu = os_release.lines().any(|line| line == "ID=ubuntu");
    let is_2604 = os_release
        .lines()
        .any(|line| line == "VERSION_ID=\"26.04\"" || line == "VERSION_ID=26.04");
    if is_ubuntu && is_2604 {
        Ok(())
    } else {
        Err("this package target must be built on Ubuntu 26.04".to_owned())
    }
}

#[cfg(target_os = "linux")]
fn build_debian_binary(root: &Path, target_dir: &Path) -> Result<(), String> {
    let jobs = std::env::var("CARGO_BUILD_JOBS").unwrap_or_else(|_| "1".to_owned());
    let rustflags = format!(
        "-C prefer-dynamic -C link-arg=-Wl,-rpath,$ORIGIN -C link-arg=-Wl,-rpath,$ORIGIN/../lib/slate --remap-path-prefix={}=/usr/src/slate",
        root.display()
    );
    let mut command = cargo_command();
    command
        .current_dir(root)
        .args([
            "build",
            "--profile",
            "production",
            "-p",
            "slate",
            "--no-default-features",
            "--features",
            "ubuntu-system-libraries",
            "-j",
            jobs.as_str(),
        ])
        .env("CARGO_TARGET_DIR", target_dir)
        .env("LIBSQLITE3_SYS_USE_PKG_CONFIG", "1")
        .env("ZSTD_SYS_USE_PKG_CONFIG", "1")
        .env("RUSTFLAGS", rustflags);
    configure_local_rust_env(&mut command);
    run_required(&mut command, "Cargo production package build")
}

#[cfg(target_os = "linux")]
fn copy_shared_rust_standard_library(root: &Path, destination: &Path) -> Result<(), String> {
    let mut command = Command::new("rustc");
    command.current_dir(root).args(["--print", "target-libdir"]);
    configure_local_rust_env(&mut command);
    let target_libdir = command_output(&mut command, "rustc --print target-libdir")?;
    let target_libdir = PathBuf::from(target_libdir.trim());
    let mut copied = 0;
    for entry in
        fs::read_dir(&target_libdir).map_err(display_io("read Rust target library directory"))?
    {
        let path = entry
            .map_err(display_io("read Rust target library entry"))?
            .path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("libstd-") && name.ends_with(".so") {
            copy_file(&path, &destination.join(name))?;
            copied += 1;
        }
    }
    if copied == 1 {
        Ok(())
    } else {
        Err(format!(
            "expected exactly one shared Rust standard library in {}, found {copied}",
            target_libdir.display()
        ))
    }
}

#[cfg(target_os = "linux")]
fn strip_package_binaries(private_lib_dir: &Path) -> Result<(), String> {
    let mut command = Command::new("strip");
    command.arg("--strip-unneeded");
    for entry in fs::read_dir(private_lib_dir).map_err(display_io("read package binaries"))? {
        let path = entry
            .map_err(display_io("read package binary entry"))?
            .path();
        if path.is_file()
            && (path.file_name().is_some_and(|name| name == "slate")
                || path.extension().is_some_and(|extension| extension == "so"))
        {
            command.arg(path);
        }
    }
    run_required(&mut command, "strip")
}

#[cfg(target_os = "linux")]
fn verify_elf_layout(root: &Path, executable: &Path, chrome_library: &Path) -> Result<(), String> {
    for path in [executable, chrome_library] {
        let output = command_output(Command::new("readelf").arg("-d").arg(path), "readelf")?;
        if !output.contains("$ORIGIN") {
            return Err(format!(
                "{} has no relocatable $ORIGIN runpath",
                path.display()
            ));
        }
        if output.contains(&root.to_string_lossy().to_string())
            || output.contains(".rustup")
            || output.contains("target/slate")
        {
            return Err(format!("{} contains a build-tree runpath", path.display()));
        }
        if output.contains("libservo") {
            return Err(format!(
                "{} unexpectedly links Servo as a separate shared library",
                path.display()
            ));
        }
    }

    let dependencies = command_output(Command::new("ldd").arg(executable), "ldd")?;
    if dependencies.contains("not found") {
        return Err(format!(
            "packaged executable has missing libraries:\n{dependencies}"
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn validate_desktop_metadata(desktop: &Path, metainfo: &Path) -> Result<(), String> {
    if tool_exists("desktop-file-validate") {
        run_required(
            Command::new("desktop-file-validate").arg(desktop),
            "desktop-file-validate",
        )?;
    }
    if tool_exists("appstreamcli") {
        run_required(
            Command::new("appstreamcli")
                .args(["validate", "--no-net"])
                .arg(metainfo),
            "appstreamcli validate",
        )?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn debian_version(root: &Path) -> Result<String, String> {
    if let Ok(version) = std::env::var("SLATE_DEB_VERSION") {
        if version.trim().is_empty() {
            return Err("SLATE_DEB_VERSION cannot be empty".to_owned());
        }
        return Ok(version);
    }
    let date = command_output(
        Command::new("git")
            .current_dir(root)
            .args(["show", "-s", "--format=%cs", "HEAD"]),
        "git show",
    )?
    .trim()
    .replace('-', "");
    let revision = command_output(
        Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "--short=8", "HEAD"]),
        "git rev-parse",
    )?;
    let dirty = command_output(
        Command::new("git")
            .current_dir(root)
            .args(["status", "--porcelain"]),
        "git status",
    )?;
    let dirty_suffix = if dirty.trim().is_empty() {
        ""
    } else {
        ".dirty"
    };
    Ok(format!(
        "{}~git{}.{}{}-1",
        env!("CARGO_PKG_VERSION"),
        date,
        revision.trim(),
        dirty_suffix
    ))
}

#[cfg(target_os = "linux")]
fn debian_architecture() -> Result<&'static str, String> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("amd64"),
        "aarch64" => Ok("arm64"),
        architecture => Err(format!(
            "unsupported Debian package architecture: {architecture}"
        )),
    }
}

#[cfg(target_os = "linux")]
fn debian_control(version: &str, architecture: &str, installed_size: u64) -> String {
    format!(
        concat!(
            "Package: {package_name}\n",
            "Version: {version}\n",
            "Section: web\n",
            "Priority: optional\n",
            "Architecture: {architecture}\n",
            "Maintainer: Slate Developers <gabrieldiego@users.noreply.github.com>\n",
            "Installed-Size: {installed_size}\n",
            "Depends: libc6 (>= 2.43), libegl1, libfontconfig1, libgl1, ",
            "libgl1-mesa-dri, libsqlite3-0, libstdc++6, libwayland-client0, ",
            "libx11-6, libx11-xcb1, libxcb1, libxcb-render0, libxcb-shape0, ",
            "libxcb-xfixes0, libxkbcommon0, libxkbcommon-x11-0, libzstd1, zlib1g\n",
            "Homepage: https://github.com/gabrieldiego/slate\n",
            "Description: privacy-focused broadweb browser\n",
            " Slate is a desktop browser built around Servo with explicit support for\n",
            " conventional, distributed, and privacy-preserving web protocols.\n",
        ),
        package_name = DEBIAN_PACKAGE_NAME,
        version = version,
        architecture = architecture,
        installed_size = installed_size,
    )
}

#[cfg(target_os = "linux")]
fn installed_size_kib(root: &Path) -> Result<u64, String> {
    let bytes = regular_files(root)?
        .into_iter()
        .try_fold(0_u64, |total, path| {
            fs::metadata(&path)
                .map(|metadata| total.saturating_add(metadata.len()))
                .map_err(display_io("read staged file size"))
        })?;
    Ok(bytes.div_ceil(1024))
}

#[cfg(target_os = "linux")]
fn write_md5sums(package_root: &Path, destination: &Path) -> Result<(), String> {
    let mut lines = String::new();
    for path in regular_files(package_root)? {
        let relative = path
            .strip_prefix(package_root)
            .map_err(|error| format!("could not make package path relative: {error}"))?;
        if relative.starts_with("DEBIAN") {
            continue;
        }
        let output = command_output(
            Command::new("md5sum")
                .current_dir(package_root)
                .arg(relative),
            "md5sum",
        )?;
        let digest = output
            .split_whitespace()
            .next()
            .ok_or_else(|| format!("md5sum returned no digest for {}", relative.display()))?;
        lines.push_str(&format!("{digest}  {}\n", relative.display()));
    }
    fs::write(destination, lines).map_err(display_io("write package md5sums"))
}

#[cfg(target_os = "linux")]
fn write_sha256(package: &Path) -> Result<(), String> {
    let parent = package
        .parent()
        .ok_or_else(|| format!("package has no parent directory: {}", package.display()))?;
    let name = package
        .file_name()
        .ok_or_else(|| format!("package has no file name: {}", package.display()))?;
    let output = command_output(
        Command::new("sha256sum").current_dir(parent).arg(name),
        "sha256sum",
    )?;
    let checksum_path = PathBuf::from(format!("{}.sha256", package.display()));
    fs::write(&checksum_path, output).map_err(display_io("write package SHA-256 checksum"))?;
    set_mode(&checksum_path, 0o644)
}

#[cfg(target_os = "linux")]
fn verify_sha256(package: &Path) -> Result<(), String> {
    let parent = package
        .parent()
        .ok_or_else(|| format!("package has no parent directory: {}", package.display()))?;
    let name = package
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("package has no UTF-8 file name: {}", package.display()))?;
    run_required(
        Command::new("sha256sum")
            .current_dir(parent)
            .args(["--check", &format!("{name}.sha256")]),
        "sha256sum --check",
    )
}

#[cfg(target_os = "linux")]
fn regular_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn visit(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(path).map_err(display_io("read package directory"))? {
            let entry = entry.map_err(display_io("read package entry"))?;
            let file_type = entry
                .file_type()
                .map_err(display_io("read package entry type"))?;
            if file_type.is_dir() {
                visit(&entry.path(), files)?;
            } else if file_type.is_file() {
                files.push(entry.path());
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    visit(root, &mut files)?;
    files.sort();
    Ok(files)
}

#[cfg(target_os = "linux")]
fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    require_file_or_directory(source, "package resource directory")?;
    fs::create_dir_all(destination).map_err(display_io("create copied directory"))?;
    for entry in fs::read_dir(source).map_err(display_io("read copied directory"))? {
        let entry = entry.map_err(display_io("read copied directory entry"))?;
        let file_type = entry
            .file_type()
            .map_err(display_io("read copied directory entry type"))?;
        let destination_path = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory(&entry.path(), &destination_path)?;
        } else if file_type.is_file() {
            copy_file(&entry.path(), &destination_path)?;
        } else {
            return Err(format!(
                "refusing to package non-file resource {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    require_file(source, "package input")?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(display_io("create copied file parent"))?;
    }
    fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| format!("could not copy {}: {error}", source.display()))
}

#[cfg(target_os = "linux")]
fn normalize_package_modes(root: &Path) -> Result<(), String> {
    fn visit(path: &Path) -> Result<(), String> {
        set_mode(path, 0o755)?;
        for entry in fs::read_dir(path).map_err(display_io("read package mode directory"))? {
            let entry = entry.map_err(display_io("read package mode entry"))?;
            let file_type = entry
                .file_type()
                .map_err(display_io("read package mode entry type"))?;
            if file_type.is_dir() {
                visit(&entry.path())?;
            } else if file_type.is_file() {
                set_mode(&entry.path(), 0o644)?;
            } else if !file_type.is_symlink() {
                return Err(format!(
                    "refusing to package unsupported entry {}",
                    entry.path().display()
                ));
            }
        }
        Ok(())
    }

    visit(root)
}

#[cfg(target_os = "linux")]
fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(display_io("read packaged file permissions"))?
        .permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions).map_err(display_io("set packaged file permissions"))
}

#[cfg(target_os = "linux")]
fn require_file(path: &Path, description: &str) -> Result<(), String> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("missing {description}: {}", path.display()))
    }
}

#[cfg(target_os = "linux")]
fn require_file_or_directory(path: &Path, description: &str) -> Result<(), String> {
    if path.exists() {
        Ok(())
    } else {
        Err(format!("missing {description}: {}", path.display()))
    }
}

#[cfg(target_os = "linux")]
fn require_tool(tool: &str) -> Result<(), String> {
    if tool_exists(tool) {
        Ok(())
    } else {
        Err(format!("required Debian packaging tool is missing: {tool}"))
    }
}

#[cfg(target_os = "linux")]
fn tool_exists(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(target_os = "linux")]
fn command_output(command: &mut Command, program: &str) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|error| format!("failed to run {program}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("invalid {program} output: {error}"))
}

#[cfg(target_os = "linux")]
fn run_required(command: &mut Command, program: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("failed to run {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

#[cfg(target_os = "linux")]
fn display_io(action: &'static str) -> impl FnOnce(std::io::Error) -> String {
    move |error| format!("{action}: {error}")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask should live directly under the workspace root")
        .to_path_buf()
}

fn run_cargo(args: &[&str]) -> ExitCode {
    let jobs = std::env::var("CARGO_BUILD_JOBS").unwrap_or_else(|_| "1".to_owned());
    let mut command = cargo_command();
    command
        .current_dir(workspace_root())
        .args(cargo_args_with_jobs(args, &jobs));
    configure_local_rust_env(&mut command);
    run_command(&mut command, "cargo")
}

fn run_cargo_without_jobs(args: &[&str]) -> ExitCode {
    let mut command = cargo_command();
    command.current_dir(workspace_root()).args(args);
    configure_local_rust_env(&mut command);
    run_command(&mut command, "cargo")
}

fn run_cargo_workspace_tests() -> ExitCode {
    let test_threads = std::env::var("SLATE_TEST_THREADS").unwrap_or_else(|_| "1".to_owned());
    run_cargo(&[
        "test",
        "--workspace",
        "--",
        "--test-threads",
        test_threads.as_str(),
    ])
}

fn cargo_command() -> Command {
    let root = workspace_root();
    let limits = root.join("scripts/with-build-limits.sh");
    if limits.is_file() {
        let mut command = Command::new(limits);
        command.arg("cargo");
        command
    } else {
        Command::new("cargo")
    }
}

fn cargo_args_with_jobs(args: &[&str], jobs: &str) -> Vec<String> {
    let mut cargo_args = Vec::with_capacity(args.len() + 2);
    let mut inserted_jobs = false;
    for arg in args {
        if !inserted_jobs && *arg == "--" {
            cargo_args.push("-j".to_owned());
            cargo_args.push(jobs.to_owned());
            inserted_jobs = true;
        }
        cargo_args.push((*arg).to_owned());
    }

    if !inserted_jobs {
        cargo_args.push("-j".to_owned());
        cargo_args.push(jobs.to_owned());
    }

    cargo_args
}

fn configure_local_rust_env(command: &mut Command) {
    let root = workspace_root();
    let rustup_home = root.join(".rustup");
    if rustup_home.is_dir() {
        command.env("RUSTUP_HOME", rustup_home);
    }
    let cargo_home = root.join(".cargo");
    if cargo_home.is_dir() {
        command.env("CARGO_HOME", cargo_home);
    }
}

fn run_command(command: &mut Command, program: &str) -> ExitCode {
    match command.status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            let code = status.code().unwrap_or(1).clamp(1, 255);
            let code = u8::try_from(code).unwrap_or(1);
            ExitCode::from(code)
        }
        Err(error) => {
            eprintln!("failed to run {program}: {error}");
            ExitCode::from(1)
        }
    }
}

fn snapshot() -> ExitCode {
    run_snapshot(Path::new("target/slate-ui.png"), None)
}

fn chrome_snapshot() -> ExitCode {
    let path = Path::new("target/slate-chrome-headless.png");
    let output = path.to_string_lossy();
    let result = run_cargo(&[
        "run",
        "-p",
        "slate-chrome",
        "--bin",
        "slate-chrome-snapshot",
        "--",
        output.as_ref(),
    ]);
    if result == ExitCode::SUCCESS {
        println!("wrote {}", path.display());
    }
    result
}

fn chrome_verify() -> ExitCode {
    let path = Path::new("target/slate-chrome-verification");
    let output = path.to_string_lossy();
    let result = run_cargo(&[
        "run",
        "-p",
        "slate-chrome",
        "--bin",
        "slate-chrome-snapshot",
        "--",
        "--verify",
        output.as_ref(),
    ]);
    if result == ExitCode::SUCCESS {
        println!("wrote {}/report.json", path.display());
    }
    result
}

fn snapshot_html() -> ExitCode {
    run_snapshot(
        Path::new("target/slate-ui-html.png"),
        Some("slate://tests/hello"),
    )
}

fn snapshot_local() -> ExitCode {
    run_snapshot(
        Path::new("target/slate-ui-local.png"),
        Some("examples/local-page.html"),
    )
}

fn run_snapshot(path: &Path, address: Option<&str>) -> ExitCode {
    let output = path.to_string_lossy();
    let mut args = vec![
        "run",
        "-p",
        "slate",
        "--",
        "--headless",
        "--exit",
        "--output",
        output.as_ref(),
    ];
    if let Some(address) = address {
        args.push(address);
    }

    let result = run_cargo(&args);
    if result == ExitCode::SUCCESS {
        println!("wrote {}", path.display());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::cargo_args_with_jobs;

    #[cfg(target_os = "linux")]
    use super::debian_control;

    #[test]
    fn cargo_jobs_are_added_to_plain_subcommands() {
        assert_eq!(
            cargo_args_with_jobs(&["check", "--workspace"], "1"),
            ["check", "--workspace", "-j", "1"]
        );
    }

    #[test]
    fn cargo_jobs_are_inserted_before_run_separator() {
        assert_eq!(
            cargo_args_with_jobs(&["run", "-p", "slate", "--", "--headless"], "2"),
            ["run", "-p", "slate", "-j", "2", "--", "--headless"]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn debian_control_description_uses_continuation_lines() {
        let control = debian_control("1.2.3-1", "amd64", 42);
        assert!(control.contains("\nDescription: privacy-focused broadweb browser\n Slate"));
        assert!(control.contains("\n conventional, distributed"));
        assert!(!control.contains("\nSlate is"));
    }
}
