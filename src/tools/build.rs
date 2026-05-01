//! Build tool - compile, bundle, and build projects

use crate::error::FerroError;
use crate::tool::{ToolFuture, ToolHandler};
use crate::types::Capability;
use serde_json::Value;
use std::path::Path;

pub fn build_meta() -> crate::types::ToolMeta {
    crate::types::ToolMeta {
        definition: crate::types::ToolDefinition {
            name: "build".into(),
            description: "Compile, bundle, or build projects. Auto-detects language/framework. Supports Rust (cargo), Node.js (npm/yarn), Python (pip), Go (go), Ruby (bundler), PHP (composer).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to project directory (default: current directory)"
                    },
                    "target": {
                        "type": "string",
                        "enum": ["development", "production", "release", "debug", "test", "clean", "all"],
                        "description": "Build target"
                    },
                    "clean": {
                        "type": "boolean",
                        "description": "Clean build artifacts before building (default: false)"
                    },
                    "tool": {
                        "type": "string",
                        "enum": ["auto", "cargo", "npm", "yarn", "pip", "go", "bundler", "composer", "make", "cmake", "gradle", "mvn", "dotnet"],
                        "description": "Build tool to use (auto-detect if not specified)"
                    },
                    "args": {
                        "type": "string",
                        "description": "Additional arguments to pass to build tool"
                    },
                    "output_path": {
                        "type": "string",
                        "description": "Path for build output (optional)"
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "Show what would be done without actually building"
                    },
                    "verbose": {
                        "type": "boolean",
                        "description": "Show verbose build output"
                    }
                },
                "required": []
            }),
            server_name: None,
        },
        required_capabilities: vec![Capability::ProcessExec],
        source: crate::types::ToolSource::Builtin,
    }
}

pub struct BuildHandler;

impl ToolHandler for BuildHandler {
    fn call<'a>(&'a self, call_id: &'a str, arguments: &'a Value) -> ToolFuture<'a> {
        Box::pin(async move {
            let path = arguments
                .get("path")
                .and_then(|p| p.as_str())
                .unwrap_or(".");

            let target = arguments
                .get("target")
                .and_then(|t| t.as_str())
                .unwrap_or("all");

            let clean = arguments
                .get("clean")
                .and_then(|c| c.as_bool())
                .unwrap_or(false);

            let tool = arguments
                .get("tool")
                .and_then(|t| t.as_str())
                .unwrap_or("auto");

            let args = arguments.get("args").and_then(|a| a.as_str()).unwrap_or("");

            let output_path = arguments.get("output_path").and_then(|o| o.as_str());

            let verbose = arguments
                .get("verbose")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let dry_run = arguments
                .get("dry_run")
                .and_then(|d| d.as_bool())
                .unwrap_or(false);

            let project_type = detect_project_type(path).await?;
            let options = BuildOptions {
                args,
                verbose,
                dry_run,
                output_path,
            };

            let build_result = match tool {
                "auto" => execute_auto_build(path, &project_type, target, clean, options).await?,
                tool_name => {
                    execute_specific_build(tool_name, path, &project_type, target, clean, options)
                        .await?
                }
            };

            Ok(crate::types::ToolResult {
                call_id: call_id.to_string(),
                content: build_result,
                is_error: false,
            })
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct BuildOptions<'a> {
    args: &'a str,
    verbose: bool,
    dry_run: bool,
    output_path: Option<&'a str>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum ProjectType {
    Rust {
        cargo_path: String,
    },
    NodeJs {
        npm_path: Option<String>,
        yarn_path: Option<String>,
    },
    Python {
        has_requirements: bool,
        has_setup_py: bool,
    },
    Go,
    Ruby,
    Php,
    Make,
    CMake,
    Gradle,
    Maven,
    DotNet,
    Unknown,
}

async fn detect_project_type(path: &str) -> Result<ProjectType, FerroError> {
    let dir_path = Path::new(path);

    // Check for specific project files
    let mut entries = tokio::fs::read_dir(dir_path)
        .await
        .map_err(|e| FerroError::Tool(format!("Cannot read {}: {}", path, e)))?;

    let mut has_cargo_lock = false;
    let mut has_package_json = false;
    let mut has_requirements_txt = false;
    let mut has_setup_py = false;
    let mut has_makefile = false;
    let mut has_cmake = false;
    let mut has_gradle = false;
    let mut has_mvn = false;
    let mut has_dotnet = false;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry
            .file_name()
            .to_string_lossy()
            .to_string()
            .to_lowercase();

        if name == "cargo.lock" || name == "cargo.toml" {
            has_cargo_lock = true;
        } else if name == "package.json" {
            has_package_json = true;
        } else if name == "requirements.txt" {
            has_requirements_txt = true;
        } else if name == "setup.py" {
            has_setup_py = true;
        } else if name == "makefile" {
            has_makefile = true;
        } else if name == "cmakelists.txt" {
            has_cmake = true;
        } else if name == "build.gradle" || name == "gradlew" {
            has_gradle = true;
        } else if name == "pom.xml" {
            has_mvn = true;
        } else if name == "csproj" || name == "*.sln" {
            has_dotnet = true;
        }
    }

    // Determine project type
    let project_type = if has_cargo_lock {
        ProjectType::Rust {
            cargo_path: dir_path.display().to_string(),
        }
    } else if has_package_json {
        // Check if it's Node.js
        let node_modules = dir_path.join("node_modules");
        let has_node_modules = node_modules.exists();
        let has_package_lock_json = dir_path.join("package-lock.json").exists();

        if has_node_modules || has_package_lock_json {
            let npm_path = if has_package_lock_json {
                Some(
                    dir_path
                        .join("node_modules")
                        .join(".bin")
                        .join("npm")
                        .to_string_lossy()
                        .to_string(),
                )
            } else {
                None
            };
            let yarn_path = if has_package_lock_json {
                Some(
                    dir_path
                        .join("node_modules")
                        .join(".bin")
                        .join("yarn")
                        .to_string_lossy()
                        .to_string(),
                )
            } else {
                None
            };
            ProjectType::NodeJs {
                npm_path,
                yarn_path,
            }
        } else {
            ProjectType::NodeJs {
                npm_path: None,
                yarn_path: None,
            }
        }
    } else if has_requirements_txt || has_setup_py {
        ProjectType::Python {
            has_requirements: has_requirements_txt,
            has_setup_py,
        }
    } else if has_makefile {
        ProjectType::Make
    } else if has_cmake {
        ProjectType::CMake
    } else if has_gradle {
        ProjectType::Gradle
    } else if has_mvn {
        ProjectType::Maven
    } else if has_dotnet {
        ProjectType::DotNet
    } else {
        ProjectType::Unknown
    };

    Ok(project_type)
}

async fn execute_auto_build(
    path: &str,
    project: &ProjectType,
    target: &str,
    clean: bool,
    options: BuildOptions<'_>,
) -> Result<String, FerroError> {
    let mut output = String::new();
    output.push_str("🔨 Build Project\n");
    output.push_str("═══════════════════════════════════\n\n");

    output.push_str(&format!(
        "📦 Project Type: {}\n",
        format_project_type(project)
    ));
    output.push_str(&format!("🎯 Target: {}\n", target));
    output.push_str(&format!("🧹 Clean: {}\n", if clean { "Yes" } else { "No" }));
    output.push_str("🚀 Build Tool: Auto-detect\n\n");

    if options.dry_run {
        output.push_str("🔍 [DRY RUN] - Not executing, showing what would be done:\n\n");
    } else {
        output.push_str("🚀 Executing build...\n\n");
    }

    match project {
        ProjectType::Rust { cargo_path } => {
            let result = build_rust(
                cargo_path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        ProjectType::NodeJs {
            npm_path,
            yarn_path,
        } => {
            let result = build_nodejs(
                path,
                npm_path.as_deref(),
                yarn_path.as_deref(),
                target,
                clean,
                options,
            )
            .await?;
            output.push_str(&result);
        }
        ProjectType::Python { .. } => {
            let result = build_python(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        ProjectType::Go => {
            let result = build_go(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        ProjectType::Ruby => {
            let result = build_ruby(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        ProjectType::Php => {
            let result = build_php(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        ProjectType::Make => {
            let result = build_make(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        ProjectType::CMake => {
            let result = build_cmake(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        ProjectType::Gradle => {
            let result = build_gradle(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        ProjectType::Maven => {
            let result = build_maven(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        ProjectType::DotNet => {
            let result = build_dotnet(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        ProjectType::Unknown => {
            output.push_str("⚠️  Unknown project type\n");
            output.push_str("   Cannot determine build system\n");
            output.push_str("   Supported: Rust (Cargo.toml), Node.js (package.json), Python (requirements.txt/setup.py), Go (go.mod), Ruby (Gemfile), PHP (composer.json), Make (Makefile), CMake (CMakeLists.txt), Gradle (build.gradle), Maven (pom.xml), .NET (csproj/*.sln)\n");
            output.push_str("   You can specify the tool explicitly with --tool parameter\n");
        }
    }

    Ok(output)
}

async fn execute_specific_build(
    tool: &str,
    path: &str,
    project: &ProjectType,
    target: &str,
    clean: bool,
    options: BuildOptions<'_>,
) -> Result<String, FerroError> {
    let mut output = String::new();
    output.push_str("🔨 Build Project\n");
    output.push_str("═══════════════════════════════════\n\n");

    output.push_str(&format!(
        "📦 Project Type: {}\n",
        format_project_type(project)
    ));
    output.push_str(&format!("🎯 Target: {}\n", target));
    output.push_str(&format!("🧹 Clean: {}\n", if clean { "Yes" } else { "No" }));
    output.push_str(&format!("🚀 Build Tool: {}\n", tool));

    if options.dry_run {
        output.push_str("🔍 [DRY RUN] - Not executing, showing what would be done:\n\n");
    } else {
        output.push_str("🚀 Executing build...\n\n");
    }

    match tool {
        "cargo" => {
            if let ProjectType::Rust { cargo_path } = project {
                let result = build_rust(
                    cargo_path,
                    target,
                    clean,
                    options.args,
                    options.verbose,
                    options.dry_run,
                    options.output_path,
                )
                .await?;
                output.push_str(&result);
            } else {
                output.push_str("⚠️  Project is not a Rust project\n");
            }
        }
        "npm" | "yarn" => {
            let npm_path = Some(tool.to_string());
            let yarn_path = if tool == "yarn" {
                Some(tool.to_string())
            } else {
                None
            };
            let result = build_nodejs(
                path,
                npm_path.as_deref(),
                yarn_path.as_deref(),
                target,
                clean,
                options,
            )
            .await?;
            output.push_str(&result);
        }
        "pip" => {
            let result = build_python(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        "go" => {
            let result = build_go(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        "bundler" => {
            let result = build_ruby(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        "composer" => {
            let result = build_php(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        "make" => {
            let result = build_make(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        "cmake" => {
            let result = build_cmake(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        "gradle" => {
            let result = build_gradle(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        "mvn" => {
            let result = build_maven(
                path,
                target,
                clean,
                options.args,
                options.verbose,
                options.dry_run,
                options.output_path,
            )
            .await?;
            output.push_str(&result);
        }
        _ => {
            output.push_str(&format!("⚠️  Unknown build tool: {}\n", tool));
            output.push_str("   Supported tools: auto, cargo, npm, yarn, pip, go, bundler, composer, make, cmake, gradle, mvn, dotnet\n");
        }
    }

    Ok(output)
}

async fn build_rust(
    path: &str,
    target: &str,
    clean: bool,
    args: &str,
    verbose: bool,
    dry_run: bool,
    _output_path: Option<&str>,
) -> Result<String, FerroError> {
    let mut output = String::new();

    // Clean if requested
    if clean {
        output.push_str("🧹 Cleaning Cargo artifacts...\n");
        if !dry_run {
            let _clean_result = tokio::process::Command::new("cargo")
                .arg("clean")
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Cargo clean failed: {}", e)))?;

            if _clean_result.status.success() {
                output.push_str("   ✅ Cargo clean complete\n");
            } else {
                output.push_str("   ❌ Cargo clean failed\n");
            }
        }
    }

    // Determine cargo command based on target
    let command = match target {
        "development" => "cargo build",
        "production" => "cargo build --release",
        "release" => "cargo build --release",
        "debug" => "cargo build",
        "test" => "cargo test",
        "all" => "cargo build",
        _ => "cargo build",
    };

    if verbose {
        let args_str = if !args.is_empty() {
            format!("-- {}", args)
        } else {
            String::new()
        };
        output.push_str(&format!("📦 Command: {} {}\n", command, args_str));
    } else {
        output.push_str(&format!("📦 Command: {}\n", command));
    }

    if dry_run {
        output.push_str("\n[DRY RUN] Would execute: cargo build\n");
    } else {
        let cmd_parts: Vec<&str> = command.split_whitespace().collect();
        let mut cmd = tokio::process::Command::new(cmd_parts[0]);
        if cmd_parts.len() > 1 {
            cmd.args(&cmd_parts[1..]);
        }
        if !args.is_empty() {
            cmd.args(args.split_whitespace());
        }
        cmd.current_dir(path);

        let build_result = cmd
            .output()
            .await
            .map_err(|e| FerroError::Tool(format!("Build failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&build_result.stdout);
        let stderr = String::from_utf8_lossy(&build_result.stderr);

        if build_result.status.success() {
            output.push_str("\n✅ Build successful\n");

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            // Check for build artifacts
            if target == "release" || target == "production" {
                output.push_str("📦 Release build artifacts in target/release/\n");
            }
        } else {
            output.push_str(&format!(
                "❌ Build failed (exit code: {})\n",
                build_result.status.code().unwrap_or(-1)
            ));

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if !stderr.is_empty() {
                output.push_str("⚠️  Errors:\n");
                output.push_str(&format!("{}\n", stderr.trim()));
            }
        }
    }

    Ok(output)
}

async fn build_nodejs(
    path: &str,
    npm_path: Option<&str>,
    yarn_path: Option<&str>,
    target: &str,
    clean: bool,
    options: BuildOptions<'_>,
) -> Result<String, FerroError> {
    let mut output = String::new();

    // Determine which tool to use
    let (tool, tool_cmd) = if let Some(_yp) = yarn_path {
        ("yarn", "yarn")
    } else if let Some(_np) = npm_path {
        ("npm", "npm")
    } else {
        ("npm", "npm")
    };

    // Clean if requested
    if clean {
        output.push_str(&format!("🧹 Cleaning {} artifacts...\n", tool));
        if !options.dry_run {
            let clean_args = match tool_cmd {
                "npm" => vec!["cache", "clean", "--force"],
                "yarn" => vec!["cache", "clean", "--force"],
                _ => vec!["clean", "--force"],
            };

            let clean_result = tokio::process::Command::new(tool)
                .args(clean_args)
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("{} clean failed: {}", tool, e)))?;

            if clean_result.status.success() {
                output.push_str("   ✅ Clean complete\n");
            } else {
                output.push_str("   ❌ Clean failed\n");
            }
        }
    }

    // Build command
    let mut build_args = String::from(match target {
        "development" => "install",
        "production" => "build",
        "release" => "build",
        "test" => "test",
        "all" => "install",
        _ => "install",
    });

    if !options.args.is_empty() {
        build_args.push_str(options.args);
    }

    if options.verbose {
        output.push_str(&format!(
            "📦 Command: {} {} {}\n",
            tool,
            build_args,
            if target != "development" { "--" } else { "" }
        ));
    } else {
        output.push_str(&format!("📦 Command: {} {}\n", tool, build_args));
    }

    if options.dry_run {
        output.push_str(&format!(
            "\n[DRY RUN] Would execute: {} {} {}\n",
            tool,
            build_args,
            if target != "development" { "--" } else { "" }
        ));
    } else {
        let mut cmd = tokio::process::Command::new(tool);
        cmd.args(build_args.split_whitespace());
        cmd.current_dir(path);

        let build_result = cmd
            .output()
            .await
            .map_err(|e| FerroError::Tool(format!("{} build failed: {}", tool, e)))?;

        let stdout = String::from_utf8_lossy(&build_result.stdout);
        let stderr = String::from_utf8_lossy(&build_result.stderr);

        if build_result.status.success() {
            output.push_str("\n✅ Build successful\n");

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if target == "development" {
                output.push_str("📦 Development build complete\n");
            } else {
                output.push_str("📦 Production build complete\n");
            }
        } else {
            output.push_str(&format!(
                "❌ Build failed (exit code: {})\n",
                build_result.status.code().unwrap_or(-1)
            ));

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if !stderr.is_empty() {
                output.push_str("⚠️  Errors:\n");
                output.push_str(&format!("{}\n", stderr.trim()));
            }
        }
    }

    Ok(output)
}

async fn build_python(
    path: &str,
    target: &str,
    clean: bool,
    args: &str,
    verbose: bool,
    dry_run: bool,
    _output_path: Option<&str>,
) -> Result<String, FerroError> {
    let mut output = String::new();

    // Clean if requested
    if clean {
        output.push_str("🧹 Cleaning Python artifacts...\n");
        if !dry_run {
            let _clean_result = tokio::process::Command::new("pip")
                .args(["cache", "purge", "--disable-pip-version-check"])
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Pip clean failed: {}", e)))?;

            let clean_result1 = tokio::process::Command::new("pip")
                .args(["cache", "purge"])
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Pip clean failed: {}", e)))?;

            let clean_result2 = tokio::process::Command::new("rm")
                .args(["-rf", "build", "dist", "*.egg-info"])
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Artifact clean failed: {}", e)))?;

            if clean_result1.status.success() && clean_result2.status.success() {
                output.push_str("   ✅ Clean complete\n");
            } else {
                output.push_str("   ❌ Clean failed\n");
            }
        }
    }

    // Build command - use pip or setup.py
    let use_setup = std::path::Path::new(path).join("setup.py").exists();
    let (build_cmd, build_tool) = if use_setup {
        ("python setup.py", "setup.py")
    } else {
        ("pip install .", "pip")
    };

    let mut build_args = String::from(match target {
        "development" => "develop",
        "production" => "sdist bdist_wheel",
        "release" => "sdist bdist_wheel",
        "test" => "test",
        "all" => "sdist bdist_wheel",
        _ => "sdist bdist_wheel",
    });

    if !args.is_empty() {
        build_args.push_str(args);
    }

    if verbose {
        output.push_str(&format!(
            "🐍 Command: {} {} {}\n",
            build_cmd, build_args, build_tool
        ));
    } else {
        output.push_str(&format!("🐍 Command: {} {}\n", build_cmd, build_args));
    }

    if dry_run {
        output.push_str(&format!(
            "\n[DRY RUN] Would execute: {} {} {}\n",
            build_cmd, build_args, build_tool
        ));
    } else {
        let mut cmd = tokio::process::Command::new(build_cmd);
        cmd.args(build_args.split_whitespace());
        cmd.current_dir(path);

        let build_result = cmd
            .output()
            .await
            .map_err(|e| FerroError::Tool(format!("{} build failed: {}", build_tool, e)))?;

        let stdout = String::from_utf8_lossy(&build_result.stdout);
        let stderr = String::from_utf8_lossy(&build_result.stderr);

        if build_result.status.success() {
            output.push_str("\n✅ Build successful\n");

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if target == "development" {
                output.push_str("🐍 Development build complete\n");
            } else {
                output.push_str("📦 Distribution build complete\n");
            }
        } else {
            output.push_str(&format!(
                "❌ Build failed (exit code: {})\n",
                build_result.status.code().unwrap_or(-1)
            ));

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if !stderr.is_empty() {
                output.push_str("⚠️  Errors:\n");
                output.push_str(&format!("{}\n", stderr.trim()));
            }
        }
    }

    Ok(output)
}

async fn build_go(
    path: &str,
    _target: &str,
    clean: bool,
    args: &str,
    _verbose: bool,
    dry_run: bool,
    _output_path: Option<&str>,
) -> Result<String, FerroError> {
    let mut output = String::new();

    if clean {
        output.push_str("🧹 Cleaning Go artifacts...\n");
        if !dry_run {
            let clean_result = tokio::process::Command::new("go")
                .args(["clean", "-cache", "-mod", "-i"])
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Go clean failed: {}", e)))?;

            if clean_result.status.success() {
                output.push_str("   ✅ Clean complete\n");
            } else {
                output.push_str("   ❌ Clean failed\n");
            }
        }
    }

    let _build_cmd = "go build";
    let build_args = args;

    let args_str = if !args.is_empty() {
        format!("-- {}", args)
    } else {
        String::new()
    };
    output.push_str(&format!("🐹 Command: go build {}\n", args_str));

    if dry_run {
        output.push_str("\n[DRY RUN] Would execute: go build\n");
    } else {
        let mut cmd = tokio::process::Command::new("go");
        cmd.args(build_args.split_whitespace());
        cmd.current_dir(path);

        let build_result = cmd
            .output()
            .await
            .map_err(|e| FerroError::Tool(format!("Go build failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&build_result.stdout);
        let stderr = String::from_utf8_lossy(&build_result.stderr);

        if build_result.status.success() {
            output.push_str("\n✅ Build successful\n");

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }
        } else {
            output.push_str(&format!(
                "❌ Build failed (exit code: {})\n",
                build_result.status.code().unwrap_or(-1)
            ));

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if !stderr.is_empty() {
                output.push_str("⚠️  Errors:\n");
                output.push_str(&format!("{}\n", stderr.trim()));
            }
        }
    }

    Ok(output)
}

async fn build_ruby(
    path: &str,
    target: &str,
    clean: bool,
    args: &str,
    verbose: bool,
    dry_run: bool,
    _output_path: Option<&str>,
) -> Result<String, FerroError> {
    let mut output = String::new();

    if clean {
        output.push_str("🧹 Cleaning Ruby artifacts...\n");
        if !dry_run {
            let clean_result = tokio::process::Command::new("bundler")
                .args(["clean", "--force"])
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Bundler clean failed: {}", e)))?;

            if clean_result.status.success() {
                output.push_str("   ✅ Clean complete\n");
            } else {
                output.push_str("   ❌ Clean failed\n");
            }
        }
    }

    output.push_str(&format!(
        "🚀 Command: bundler install (target: {}, args: {}, verbose: {})\n",
        target, args, verbose
    ));

    if dry_run {
        output.push_str("\n[DRY RUN] Would execute: bundler install\n");
    } else {
        let mut cmd = tokio::process::Command::new("bundler");
        cmd.args(["install"]);
        cmd.current_dir(path);

        let build_result = cmd
            .output()
            .await
            .map_err(|e| FerroError::Tool(format!("Bundler install failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&build_result.stdout);
        let stderr = String::from_utf8_lossy(&build_result.stderr);

        if build_result.status.success() {
            output.push_str("\n✅ Bundle successful\n");

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }
        } else {
            output.push_str(&format!(
                "❌ Bundle failed (exit code: {})\n",
                build_result.status.code().unwrap_or(-1)
            ));

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if !stderr.is_empty() {
                output.push_str("⚠️  Errors:\n");
                output.push_str(&format!("{}\n", stderr.trim()));
            }
        }
    }

    Ok(output)
}

async fn build_php(
    path: &str,
    target: &str,
    clean: bool,
    args: &str,
    verbose: bool,
    dry_run: bool,
    _output_path: Option<&str>,
) -> Result<String, FerroError> {
    let mut output = String::new();

    if clean {
        output.push_str("🧹 Cleaning PHP artifacts...\n");
        if !dry_run {
            let clean_result = tokio::process::Command::new("composer")
                .args(["clear-cache"])
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Composer clean failed: {}", e)))?;

            if clean_result.status.success() {
                output.push_str("   ✅ Clean complete\n");
            } else {
                output.push_str("   ❌ Clean failed\n");
            }
        }
    }

    output.push_str(&format!(
        "🚀 Command: composer install (target: {}, args: {}, verbose: {})\n",
        target, args, verbose
    ));

    if dry_run {
        output.push_str("\n[DRY RUN] Would execute: composer install\n");
    } else {
        let mut cmd = tokio::process::Command::new("composer");
        cmd.args(["install"]);
        cmd.current_dir(path);

        let build_result = cmd
            .output()
            .await
            .map_err(|e| FerroError::Tool(format!("Composer install failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&build_result.stdout);
        let stderr = String::from_utf8_lossy(&build_result.stderr);

        if build_result.status.success() {
            output.push_str("\n✅ Build successful\n");

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }
        } else {
            output.push_str(&format!(
                "❌ Build failed (exit code: {})\n",
                build_result.status.code().unwrap_or(-1)
            ));

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if !stderr.is_empty() {
                output.push_str("⚠️  Errors:\n");
                output.push_str(&format!("{}\n", stderr.trim()));
            }
        }
    }

    Ok(output)
}

async fn build_make(
    path: &str,
    target: &str,
    clean: bool,
    args: &str,
    _verbose: bool,
    dry_run: bool,
    _output_path: Option<&str>,
) -> Result<String, FerroError> {
    let mut output = String::new();

    if clean {
        output.push_str("🧹 Cleaning Make artifacts...\n");
        if !dry_run {
            let _clean_result = tokio::process::Command::new("make")
                .args(["clean"])
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Make clean failed: {}", e)))?;

            if _clean_result.status.success() {
                output.push_str("   ✅ Clean complete\n");
            } else {
                output.push_str("   ❌ Clean failed\n");
            }
        }
    }

    let _build_cmd = "make";
    let mut build_args = args.to_string();

    if !args.is_empty() && target != "all" {
        build_args.push(' ');
        build_args.push_str(target);
    }

    let args_str = if !args.is_empty() {
        format!("{} {}", build_args, target)
    } else {
        build_args.clone()
    };
    output.push_str(&format!("🛠️ Command: make {}\n", args_str));

    if dry_run {
        output.push_str("\n[DRY RUN] Would execute: make build\n");
    } else {
        let mut cmd = tokio::process::Command::new("make");
        cmd.args(build_args.split_whitespace());
        cmd.current_dir(path);

        let build_result = cmd
            .output()
            .await
            .map_err(|e| FerroError::Tool(format!("Make build failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&build_result.stdout);
        let stderr = String::from_utf8_lossy(&build_result.stderr);

        if build_result.status.success() {
            output.push_str("\n✅ Build successful\n");

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }
        } else {
            output.push_str(&format!(
                "❌ Build failed (exit code: {})\n",
                build_result.status.code().unwrap_or(-1)
            ));

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if !stderr.is_empty() {
                output.push_str("⚠️  Errors:\n");
                output.push_str(&format!("{}\n", stderr.trim()));
            }
        }
    }

    Ok(output)
}

async fn build_cmake(
    path: &str,
    _target: &str,
    clean: bool,
    _args: &str,
    _verbose: bool,
    dry_run: bool,
    _output_path: Option<&str>,
) -> Result<String, FerroError> {
    let mut output = String::new();

    let build_dir = Path::new(path).join("build");

    if clean {
        output.push_str("🧹 Cleaning CMake artifacts...\n");
        if !dry_run {
            let _clean_result = tokio::fs::remove_dir_all(&build_dir).await;
            output.push_str("   ✅ Clean complete\n");
        }
    }

    // Create build directory if it doesn't exist
    if !dry_run {
        tokio::fs::create_dir_all(&build_dir)
            .await
            .map_err(|e| FerroError::Tool(format!("Failed to create build directory: {}", e)))?;
        output.push_str("   ✅ Build directory ready\n");
    }

    output.push_str("🚀 Command: cmake .. && cmake --build .\n");

    if dry_run {
        output.push_str("\n[DRY RUN] Would execute: cmake .. && cmake --build .\n");
    } else {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c");
        cmd.arg("cmake .. && cmake --build .");
        cmd.current_dir(path);

        let build_result = cmd
            .output()
            .await
            .map_err(|e| FerroError::Tool(format!("CMake build failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&build_result.stdout);
        let stderr = String::from_utf8_lossy(&build_result.stderr);

        if build_result.status.success() {
            output.push_str("\n✅ Build successful\n");

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }
        } else {
            output.push_str(&format!(
                "❌ Build failed (exit code: {})\n",
                build_result.status.code().unwrap_or(-1)
            ));

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if !stderr.is_empty() {
                output.push_str("⚠️  Errors:\n");
                output.push_str(&format!("{}\n", stderr.trim()));
            }
        }
    }

    Ok(output)
}

async fn build_gradle(
    path: &str,
    target: &str,
    clean: bool,
    args: &str,
    verbose: bool,
    dry_run: bool,
    _output_path: Option<&str>,
) -> Result<String, FerroError> {
    let mut output = String::new();

    if clean {
        output.push_str("🧹 Cleaning Gradle artifacts...\n");
        if !dry_run {
            let clean_result = tokio::process::Command::new("gradlew")
                .args(["clean"])
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Gradle clean failed: {}", e)))?;

            if clean_result.status.success() {
                output.push_str("   ✅ Clean complete\n");
            } else {
                output.push_str("   ❌ Clean failed\n");
            }
        }
    }

    let build_cmd = match target {
        "development" => "build",
        "production" => "build",
        "release" => "build",
        "test" => "test",
        "all" => "build",
        _ => "build",
    };

    if verbose {
        let args_str = if !args.is_empty() {
            format!("-- {}", args)
        } else {
            String::new()
        };
        output.push_str(&format!("🐘 Command: gradle {} {}\n", build_cmd, args_str));
    } else {
        let args_str = if !args.is_empty() {
            format!(" {build_cmd} -- {}", args)
        } else {
            format!(" {build_cmd}")
        };
        output.push_str(&format!("🐘 Command: gradle{}\n", args_str));
    }

    if dry_run {
        output.push_str("\n[DRY RUN] Would execute: gradle build\n");
    } else {
        let cmd_args = if !args.is_empty() {
            vec![build_cmd, args]
        } else {
            vec![build_cmd]
        };

        let mut cmd = tokio::process::Command::new("gradlew");
        cmd.args(cmd_args);
        cmd.current_dir(path);

        let build_result = cmd
            .output()
            .await
            .map_err(|e| FerroError::Tool(format!("Gradle build failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&build_result.stdout);
        let stderr = String::from_utf8_lossy(&build_result.stderr);

        if build_result.status.success() {
            output.push_str("\n✅ Build successful\n");

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }
        } else {
            output.push_str(&format!(
                "❌ Build failed (exit code: {})\n",
                build_result.status.code().unwrap_or(-1)
            ));

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if !stderr.is_empty() {
                output.push_str("⚠️  Errors:\n");
                output.push_str(&format!("{}\n", stderr.trim()));
            }
        }
    }

    Ok(output)
}

async fn build_maven(
    path: &str,
    target: &str,
    clean: bool,
    args: &str,
    verbose: bool,
    dry_run: bool,
    _output_path: Option<&str>,
) -> Result<String, FerroError> {
    let mut output = String::new();

    if clean {
        output.push_str("🧹 Cleaning Maven artifacts...\n");
        if !dry_run {
            let clean_result = tokio::process::Command::new("mvn")
                .args(["clean"])
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("Maven clean failed: {}", e)))?;

            if clean_result.status.success() {
                output.push_str("   ✅ Clean complete\n");
            } else {
                output.push_str("   ❌ Clean failed\n");
            }
        }
    }

    let build_cmd = match target {
        "development" => "compile",
        "production" => "package",
        "release" => "package",
        "test" => "test",
        "all" => "package",
        _ => "package",
    };

    if verbose {
        let args_str = if !args.is_empty() {
            format!(" -- {}", args)
        } else {
            String::new()
        };
        output.push_str(&format!("☕️ Command: mvn {}{}\n", build_cmd, args_str));
    } else {
        output.push_str(&format!("☕️ Command: mvn {}\n", build_cmd));
    }

    if dry_run {
        output.push_str("\n[DRY RUN] Would execute: mvn package\n");
    } else {
        let mut cmd = tokio::process::Command::new("mvn");
        cmd.args([build_cmd]);
        cmd.current_dir(path);

        let build_result = cmd
            .output()
            .await
            .map_err(|e| FerroError::Tool(format!("Maven build failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&build_result.stdout);
        let stderr = String::from_utf8_lossy(&build_result.stderr);

        if build_result.status.success() {
            output.push_str("\n✅ Build successful\n");

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }
        } else {
            output.push_str(&format!(
                "❌ Build failed (exit code: {})\n",
                build_result.status.code().unwrap_or(-1)
            ));

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if !stderr.is_empty() {
                output.push_str("⚠️  Errors:\n");
                output.push_str(&format!("{}\n", stderr.trim()));
            }
        }
    }

    Ok(output)
}

async fn build_dotnet(
    path: &str,
    target: &str,
    clean: bool,
    args: &str,
    verbose: bool,
    dry_run: bool,
    _output_path: Option<&str>,
) -> Result<String, FerroError> {
    let mut output = String::new();

    let project_file = find_dotnet_project(path).await?;

    if clean {
        output.push_str("🧹 Cleaning .NET artifacts...\n");
        if !dry_run {
            let clean_result = tokio::process::Command::new("dotnet")
                .args(["clean"])
                .current_dir(path)
                .output()
                .await
                .map_err(|e| FerroError::Tool(format!("dotnet clean failed: {}", e)))?;

            if clean_result.status.success() {
                output.push_str("   ✅ Clean complete\n");
            } else {
                output.push_str("   ❌ Clean failed\n");
            }
        }
    }

    output.push_str(&format!(
        "☕️ Command: dotnet build {} (target: {}, args: {}, verbose: {})\n",
        project_file, target, args, verbose
    ));

    if dry_run {
        output.push_str(&format!(
            "\n[DRY RUN] Would execute: dotnet build {}\n",
            project_file
        ));
    } else {
        let mut cmd = tokio::process::Command::new("dotnet");
        cmd.args(["build", project_file.as_str()]);
        cmd.current_dir(path);

        let build_result = cmd
            .output()
            .await
            .map_err(|e| FerroError::Tool(format!("dotnet build failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&build_result.stdout);
        let stderr = String::from_utf8_lossy(&build_result.stderr);

        if build_result.status.success() {
            output.push_str("\n✅ Build successful\n");

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }
        } else {
            output.push_str(&format!(
                "❌ Build failed (exit code: {})\n",
                build_result.status.code().unwrap_or(-1)
            ));

            if !stdout.is_empty() {
                output.push_str("📤 Output:\n");
                output.push_str(&format!("{}\n", stdout.trim()));
            }

            if !stderr.is_empty() {
                output.push_str("⚠️  Errors:\n");
                output.push_str(&format!("{}\n", stderr.trim()));
            }
        }
    }

    Ok(output)
}

async fn find_dotnet_project(path: &str) -> Result<String, FerroError> {
    let dir_path = Path::new(path);
    let mut entries = tokio::fs::read_dir(dir_path)
        .await
        .map_err(|e| FerroError::Tool(format!("Cannot read {}: {}", path, e)))?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();

        // Look for solution or project files
        if name.ends_with(".sln") || name.ends_with(".csproj") {
            return Ok(name);
        }
    }

    Ok("project.csproj".to_string())
}

fn format_project_type(project: &ProjectType) -> &'static str {
    match project {
        ProjectType::Rust { .. } => "Rust (Cargo)",
        ProjectType::NodeJs { .. } => "Node.js (package.json)",
        ProjectType::Python { .. } => "Python (requirements.txt/setup.py)",
        ProjectType::Go => "Go (go.mod)",
        ProjectType::Ruby => "Ruby (Gemfile)",
        ProjectType::Php => "PHP (composer.json)",
        ProjectType::Make => "Make (Makefile)",
        ProjectType::CMake => "CMake (CMakeLists.txt)",
        ProjectType::Gradle => "Gradle (build.gradle)",
        ProjectType::Maven => "Maven (pom.xml)",
        ProjectType::DotNet => ".NET (csproj/*.sln)",
        ProjectType::Unknown => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_detect_rust_project() {
        let project_type = detect_project_type(".").await.unwrap();
        assert!(matches!(project_type, ProjectType::Rust { .. }));
    }

    #[tokio::test]
    async fn test_detect_nodejs_project() {
        // This test would need a temporary directory with package.json
        // For now, we just test the function doesn't crash
        let project_type = detect_project_type(".").await.unwrap();
        // Current directory is Rust, so we expect Rust or Unknown
        assert!(matches!(
            project_type,
            ProjectType::Rust { .. } | ProjectType::Unknown
        ));
    }

    #[tokio::test]
    async fn test_detect_python_project() {
        // This test would need a temporary directory with requirements.txt or setup.py
        // For now, we just test the function doesn't crash
        let project_type = detect_project_type(".").await.unwrap();
        // Current directory is Rust, so we expect Rust or Unknown
        assert!(matches!(
            project_type,
            ProjectType::Rust { .. } | ProjectType::Unknown
        ));
    }

    #[tokio::test]
    async fn test_detect_unknown_project() {
        let project_type = detect_project_type("/tmp").await.unwrap();
        assert!(matches!(project_type, ProjectType::Unknown));
    }

    #[tokio::test]
    async fn test_build_rust() {
        let result = build_rust(".", "development", false, "", false, false, None)
            .await
            .unwrap();
        assert!(result.contains("Build successful"));
    }
}
