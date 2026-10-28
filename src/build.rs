use crate::config::Config;
use ond_protocol::{CompilationManifest, PackageArtifact, interface::Export};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn build(config: &Config, config_path: &Path) -> Result<PathBuf, String> {
    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
    let project = fs::canonicalize(parent.join(&config.project.path))
        .map_err(|e| format!("project.path: {e}"))?;
    let manifest: toml::Value = toml::from_str(
        &fs::read_to_string(project.join("ond.toml")).map_err(|e| format!("ond.toml読込: {e}"))?,
    )
    .map_err(|e| format!("ond.toml: {e}"))?;
    if manifest
        .get("project")
        .and_then(|p| p.get("final"))
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        return Err("project.pathの対象はfinal=falseのprojectである必要があります".into());
    }
    let directory = prepare_directory(&project)?;
    for name in [
        "ond.toml",
        "main.ond",
        "ond.lock",
        "program.ondbuild",
        "program.bin",
        "program.onddebug",
    ] {
        let path = directory.join(name);
        check_contained(&path, &directory)?;
    }
    sync_lock(&project, &directory)?;
    let mut generated = toml::Table::new();
    generated.insert(
        "project".into(),
        toml::Value::Table(toml::Table::from_iter([
            ("name".into(), toml::Value::String(String::new())),
            ("final".into(), toml::Value::Boolean(true)),
        ])),
    );
    let mut dependency = toml::Table::from_iter([(
        "path".into(),
        toml::Value::String(
            project
                .to_str()
                .ok_or("project.pathはUTF-8が必要です")?
                .to_owned(),
        ),
    )]);
    if let Some(role) = &config.project.role {
        dependency.insert("role".into(), toml::Value::String(role.clone()));
    }
    generated.insert(
        "dependencies".into(),
        toml::Value::Table(toml::Table::from_iter([(
            "subject".into(),
            toml::Value::Table(dependency),
        )])),
    );
    let link = format!(
        "\n[link]\nstartup = 'external'\nstack-size = {}\n[link.ram]\nbase = 0\nsize = {}\n[link.exit]\nmode = 'jump'\naddress = {}\n",
        config.vm.stack_size,
        config.vm.ram_size - 8,
        config.exit_address()?
    );
    write_changed(&directory.join("main.ond"), source(config).as_bytes())?;
    let manifest_path = directory.join("program.ondbuild");
    let output = directory.join("program.bin");
    compile_with_names(&project, &directory, config, |name| {
        generated["project"]["name"] = toml::Value::String(name.to_owned());
        let text = toml::to_string(&generated).map_err(|e| e.to_string())? + &link;
        write_changed(&directory.join("ond.toml"), text.as_bytes())?;
        run(Command::new("ond")
            .arg("compile")
            .arg(&directory)
            .arg("-o")
            .arg(&manifest_path))
    })?;
    validate_registered_functions(config, &manifest_path)?;
    run(Command::new("ond")
        .arg("link")
        .arg(&manifest_path)
        .arg("--project")
        .arg(&directory)
        .arg("-o")
        .arg(&output))?;
    Ok(output)
}
fn validate_registered_functions(config: &Config, manifest_path: &Path) -> Result<(), String> {
    let manifest: CompilationManifest = serde_json::from_slice(
        &fs::read(manifest_path).map_err(|e| format!("compile manifest読込: {e}"))?,
    )
    .map_err(|e| format!("compile manifest: {e}"))?;
    manifest.contract.validate()?;
    let parent = manifest_path
        .parent()
        .ok_or("compile manifest所在directoryがありません")?;
    let mut checked = HashSet::new();
    for case in &config.cases {
        if !checked.insert((&case.package, &case.function)) {
            continue;
        }
        let mut matching = manifest
            .packages
            .iter()
            .filter(|reference| reference.package == case.package);
        let reference = matching
            .next()
            .ok_or_else(|| format!("登録packageの成果物がありません: {}", case.package))?;
        if matching.next().is_some() {
            return Err(format!(
                "登録packageの成果物が重複しています: {}",
                case.package
            ));
        }
        let path = parent.join(&reference.path);
        let bytes =
            fs::read(&path).map_err(|e| format!("package成果物 {}: {e}", path.display()))?;
        if reference.content_hash != format!("{:x}", Sha256::digest(&bytes)) {
            return Err(format!(
                "package成果物のcontent_hash不一致: {}",
                case.package
            ));
        }
        let artifact: PackageArtifact = serde_json::from_slice(&bytes)
            .map_err(|e| format!("package成果物 {}: {e}", case.package))?;
        if artifact.package != case.package
            || artifact.interface.package != case.package
            || artifact.debug.package != case.package
            || artifact.build_id != reference.build_id
            || artifact.compiler_id != manifest.compiler_id
            || artifact.contract != manifest.contract
        {
            return Err(format!("package成果物の識別情報不一致: {}", case.package));
        }
        artifact.interface.validate()?;
        match artifact.interface.exports.get(&case.function) {
            Some(Export::Function(signature))
                if signature.parameters.is_empty() && signature.returns.is_empty() => {}
            _ => {
                return Err(format!(
                    "case {}: {}.{} は公開トップレベル関数宣言 func() ではありません",
                    case.name, case.package, case.function
                ));
            }
        }
    }
    Ok(())
}
fn write_changed(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if fs::read(path).ok().as_deref() != Some(bytes) {
        fs::write(path, bytes).map_err(|e| format!("生成file {}: {e}", path.display()))?;
    }
    Ok(())
}
fn run(command: &mut Command) -> Result<(), String> {
    let output = command.output().map_err(|e| format!("ond起動: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "ondビルド失敗 ({}):\n{}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}
fn quoted(value: &str) -> String {
    let mut result = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => result.push_str(&format!("\\U{:08x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}
pub fn source(config: &Config) -> String {
    let mut text = String::from("package main\n");
    let mut packages: Vec<&str> = Vec::new();
    for case in &config.cases {
        if !packages.contains(&case.package.as_str()) {
            let i = packages.len();
            packages.push(&case.package);
            text.push_str(&format!("import p{i} {}\n", quoted(&case.package)));
        }
    }
    let count = config.cases.len();
    text.push_str(&format!(
        "\nfunc main() {{\n    var callbacks: [{count}]func() = [{count}]func(){{"
    ));
    for case in &config.cases {
        let i = packages.iter().position(|p| *p == case.package).unwrap();
        text.push_str(&format!("p{i}.{},", case.function));
    }
    text.push_str(&format!(
        "}}\n    var selected: u32 = *({} as *u32)\n    callbacks[selected]()\n}}\n",
        config.vm.ram_size - 8
    ));
    text
}

fn check_contained(path: &Path, root: &Path) -> Result<Option<PathBuf>, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            let resolved =
                fs::canonicalize(path).map_err(|e| format!("生成先 {}: {e}", path.display()))?;
            if resolved == root || !resolved.starts_with(root) {
                return Err(format!(
                    "生成先が許可directory外またはdirectory rootを参照しています: {}",
                    path.display()
                ));
            }
            Ok(Some(resolved))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("生成先 {}: {error}", path.display())),
    }
}
fn prepare_directory(project: &Path) -> Result<PathBuf, String> {
    let target = project.join("target");
    let directory = target.join("ond-unit");
    let resolved_target = check_contained(&target, project)?.unwrap_or(target);
    check_contained(&directory, &resolved_target)?;
    fs::create_dir_all(&directory).map_err(|e| format!("生成project作成: {e}"))?;
    fs::canonicalize(&directory).map_err(|e| format!("生成project: {e}"))
}
fn sync_lock(project: &Path, directory: &Path) -> Result<(), String> {
    let source = project.join("ond.lock");
    let destination = directory.join("ond.lock");
    match fs::read(&source) {
        Ok(bytes) => write_changed(&destination, &bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match fs::remove_file(&destination) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("古い生成lockの除去: {error}")),
            }
        }
        Err(error) => Err(format!("対象ond.lock読込: {error}")),
    }
}
fn compile_with_names(
    project: &Path,
    directory: &Path,
    config: &Config,
    mut compile: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    let mut names = namespace_names(project, directory, config)?;
    let mut name = generated_name(&names);
    loop {
        match compile(&name) {
            Ok(()) => return Ok(()),
            Err(error) => {
                // 通常compilerが取得した依存に実際の衝突がある場合だけ再生成する。
                let discovered = namespace_names(project, directory, config)?;
                if !discovered.contains(&name) {
                    return Err(error);
                }
                names.extend(discovered);
                name = generated_name(&names);
            }
        }
    }
}
fn namespace_names(
    project: &Path,
    directory: &Path,
    config: &Config,
) -> Result<HashSet<String>, String> {
    let mut names = config
        .cases
        .iter()
        .filter_map(|case| case.package.split('/').next())
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    // prepare_directoryが返した正規pathを除外し、生成名自身を衝突と数えない。
    let mut visited = HashSet::from([directory.to_path_buf()]);
    collect_names(project, &mut visited, &mut names)?;
    collect_cached_names(directory, &mut visited, &mut names)?;
    Ok(names)
}
fn generated_name(names: &HashSet<String>) -> String {
    let mut candidate = String::from("ond_unit_generated");
    while names.contains(&candidate) {
        candidate.push('_');
    }
    candidate
}
fn collect_names(
    project: &Path,
    visited: &mut HashSet<PathBuf>,
    names: &mut HashSet<String>,
) -> Result<(), String> {
    let project = fs::canonicalize(project).map_err(|e| format!("依存project: {e}"))?;
    if !visited.insert(project.clone()) {
        return Ok(());
    }
    let manifest: toml::Value = toml::from_str(
        &fs::read_to_string(project.join("ond.toml")).map_err(|e| format!("ond.toml読込: {e}"))?,
    )
    .map_err(|e| format!("ond.toml: {e}"))?;
    let name = manifest
        .get("project")
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
        .ok_or("対象projectのnameがありません")?;
    names.insert(name.to_owned());
    if let Some(dependencies) = manifest.get("dependencies").and_then(|v| v.as_table()) {
        for dependency in dependencies.values() {
            let path = dependency
                .as_str()
                .filter(|s| !looks_like_git(s))
                .or_else(|| dependency.get("path").and_then(|v| v.as_str()));
            if let Some(path) = path {
                collect_names(&project.join(path), visited, names)?;
            }
        }
    }
    collect_cached_names(&project, visited, names)
}
fn collect_cached_names(
    project: &Path,
    visited: &mut HashSet<PathBuf>,
    names: &mut HashSet<String>,
) -> Result<(), String> {
    // Git取得・lock解決はcompilerへ委ね、既存cacheのpath依存も名前探索する。
    let cache = project.join(".ond/dependencies");
    match fs::read_dir(&cache) {
        Ok(entries) => {
            for entry in entries {
                let path = entry.map_err(|e| format!("依存cache: {e}"))?.path();
                // 未使用または不完全なcacheのgraphはbuildの成立条件にしない。
                let _ = collect_names(&path, visited, names);
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("依存cache: {error}")),
    }
}
fn looks_like_git(value: &str) -> bool {
    value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("ssh://")
        || value.starts_with("git@")
        || value.ends_with(".git")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root(label: &str) -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "build-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&path).unwrap();
        fs::canonicalize(path).unwrap()
    }
    #[test]
    fn lock_matches_target_and_stale_generated_lock_is_removed() {
        let project = root("lock");
        let generated = prepare_directory(&project).unwrap();
        let lock = b"version=2\npackage=[]\n";
        fs::write(project.join("ond.lock"), lock).unwrap();
        sync_lock(&project, &generated).unwrap();
        assert_eq!(fs::read(generated.join("ond.lock")).unwrap(), lock);
        assert_eq!(fs::read(project.join("ond.lock")).unwrap(), lock);
        let unlocked = root("no-lock");
        sync_lock(&unlocked, &generated).unwrap();
        assert!(!generated.join("ond.lock").exists());
        assert_eq!(fs::read(project.join("ond.lock")).unwrap(), lock);
    }
    #[test]
    fn namespace_avoids_path_dependencies_and_case_imports() {
        let root = root("names");
        fs::create_dir(root.join("dependency")).unwrap();
        fs::write(
            root.join("ond.toml"),
            "[project]\nname='ond_unit_generated'\n[dependencies]\nother={path='dependency'}\n",
        )
        .unwrap();
        fs::write(
            root.join("dependency/ond.toml"),
            "[project]\nname='ond_unit_generated_'\n[dependencies]\nparent={path='..'}\n",
        )
        .unwrap();
        let config:Config=toml::from_str("version=1\n[project]\npath='.'\n[vm]\nram_size=1024\nstack_size=128\n[[cases]]\nname='case'\npackage='ond_unit_generated__/cases'\nfunction='Run'\n").unwrap();
        assert_eq!(
            generated_name(
                &namespace_names(&root, &root.join("target/ond-unit"), &config).unwrap()
            ),
            "ond_unit_generated___"
        );
    }
    #[test]
    fn compiler_discovery_renames_only_new_namespace_collisions() {
        let project = root("discovery");
        fs::write(project.join("ond.toml"), "[project]\nname='checks'\n").unwrap();
        let directory = prepare_directory(&project).unwrap();
        let config: Config = toml::from_str("version=1\n[project]\npath='.'\n[vm]\nram_size=1024\nstack_size=128\n[[cases]]\nname='case'\npackage='checks/cases'\nfunction='Run'\n").unwrap();
        let mut attempted = Vec::new();
        compile_with_names(&project, &directory, &config, |name| {
            attempted.push(name.to_owned());
            if attempted.len() <= 2 {
                let cache = directory.join(format!(".ond/dependencies/git-{}", attempted.len()));
                fs::create_dir_all(cache.join("nested")).unwrap();
                // 二件目の衝突は取得したGit projectのpath依存にある。
                fs::write(
                    cache.join("ond.toml"),
                    if attempted.len() == 1 {
                        format!("[project]\nname='{name}'\n")
                    } else {
                        "[project]\nname='gitlib'\n[dependencies]\nnested={path='nested'}\n".into()
                    },
                )
                .unwrap();
                fs::write(
                    cache.join("nested/ond.toml"),
                    format!("[project]\nname='{name}'\n"),
                )
                .unwrap();
                return Err("compiler discovered a dependency collision".into());
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(
            attempted,
            [
                "ond_unit_generated",
                "ond_unit_generated_",
                "ond_unit_generated__"
            ]
        );
        // Warm cacheは最初から衝突しない同じ名前を選び、不完全なstale cacheも妨げない。
        fs::create_dir_all(directory.join(".ond/dependencies/stale")).unwrap();
        compile_with_names(&project, &directory, &config, |name| {
            assert_eq!(name, "ond_unit_generated__");
            Ok(())
        })
        .unwrap();
        for error in [
            "unrelated compiler failure",
            "dependency namespace `ond_unit_generated__` resolves to both a and b",
        ] {
            let mut calls = 0;
            assert_eq!(
                compile_with_names(&project, &directory, &config, |_| {
                    calls += 1;
                    Err(error.into())
                })
                .unwrap_err(),
                error
            );
            assert_eq!(calls, 1);
        }
    }
    #[test]
    fn generated_project_back_edges_do_not_retry_compiler_errors() {
        for back_edge in ["target/ond-unit", "target/ond-unit/../ond-unit", "cached"] {
            let project = root("back-edge");
            let directory = prepare_directory(&project).unwrap();
            let dependencies = if back_edge == "cached" {
                let cache = directory.join(".ond/dependencies/stale");
                fs::create_dir_all(&cache).unwrap();
                fs::write(
                    cache.join("ond.toml"),
                    "[project]\nname='cached_library'\n[dependencies]\nback={path='../../..'}\n",
                )
                .unwrap();
                String::new()
            } else {
                format!("[dependencies]\nback={{path='{back_edge}'}}\n")
            };
            fs::write(
                project.join("ond.toml"),
                format!("[project]\nname='checks'\n{dependencies}"),
            )
            .unwrap();
            fs::write(
                directory.join("ond.toml"),
                "[project]\nname='previous_generated'\nfinal=true\n",
            )
            .unwrap();
            let config: Config = toml::from_str("version=1\n[project]\npath='.'\n[vm]\nram_size=1024\nstack_size=128\n[[cases]]\nname='case'\npackage='checks/cases'\nfunction='Run'\n").unwrap();
            let mut calls = 0;
            let error = compile_with_names(&project, &directory, &config, |name| {
                calls += 1;
                assert_eq!(calls, 1, "生成project自身の名前による再試行: {back_edge}");
                fs::write(
                    directory.join("ond.toml"),
                    format!("[project]\nname='{name}'\nfinal=true\n"),
                )
                .unwrap();
                Err("ordinary compiler graph error".into())
            })
            .unwrap_err();
            assert_eq!(error, "ordinary compiler graph error");
            assert_eq!(calls, 1);
        }
    }
    #[cfg(unix)]
    #[test]
    fn dangling_generated_links_cannot_create_external_sources() {
        use std::os::unix::fs::symlink;
        let project = root("dangling");
        let outside = root("outside");
        fs::write(project.join("ond.toml"), "[project]\nname='checks'\n").unwrap();
        let directory = prepare_directory(&project).unwrap();
        let config: Config = toml::from_str("version=1\n[project]\npath='.'\n[vm]\nram_size=1024\nstack_size=128\n[[cases]]\nname='case'\npackage='checks/cases'\nfunction='Run'\n").unwrap();
        for name in [
            "ond.toml",
            "main.ond",
            "ond.lock",
            "program.ondbuild",
            "program.bin",
            "program.onddebug",
        ] {
            let destination = outside.join(name);
            let link = directory.join(name);
            symlink(&destination, &link).unwrap();
            assert!(build(&config, &project.join("ond-unit.toml")).is_err());
            assert!(!destination.exists());
            fs::remove_file(link).unwrap();
        }
        fs::remove_dir(&directory).unwrap();
        symlink(outside.join("missing-directory"), &directory).unwrap();
        assert!(prepare_directory(&project).is_err());
        assert!(!outside.join("missing-directory").exists());
        fs::remove_file(&directory).unwrap();
        fs::remove_dir(project.join("target")).unwrap();
        symlink(outside.join("missing-target"), project.join("target")).unwrap();
        assert!(prepare_directory(&project).is_err());
        assert!(!outside.join("missing-target").exists());
    }
    #[cfg(windows)]
    #[test]
    fn generated_directory_junctions_cannot_escape_project() {
        for boundary in ["target", "target/ond-unit"] {
            let outside = root("junction-outside");
            let project = root("junction");
            let link = project.join(boundary);
            fs::create_dir_all(link.parent().unwrap()).unwrap();
            let output = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    "New-Item -ItemType Junction -Path $env:OND_UNIT_TEST_LINK -Target $env:OND_UNIT_TEST_TARGET -ErrorAction Stop | Out-Null",
                ])
                .env("OND_UNIT_TEST_LINK", &link)
                .env("OND_UNIT_TEST_TARGET", &outside)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(prepare_directory(&project).is_err());
            assert!(!outside.join("ond-unit").exists());
            fs::remove_dir(&outside).unwrap();
            assert!(prepare_directory(&project).is_err());
            assert!(!outside.exists());
            fs::remove_dir(&link).unwrap();
        }
    }
    #[test]
    fn registered_cases_require_function_declarations_in_normal_exports() {
        let project = root("exports");
        fs::create_dir(project.join("packages")).unwrap();
        let config:Config=toml::from_str("version=1\n[project]\npath='.'\n[vm]\nram_size=1024\nstack_size=128\n[[cases]]\nname='decl'\npackage='checks/cases'\nfunction='Run'\n").unwrap();
        let contract = ond_protocol::Contract::default();
        let signature = serde_json::json!({"parameters":[],"returns":[]});
        let mut artifact = serde_json::json!({
            "contract":contract,"compiler_id":"c".repeat(64),"package":"checks/cases","build_id":"b".repeat(64),
            "dependencies":{},"imports":[],"initializer":null,
            "interface":{"package":"checks/cases","types":[{"name":null,"kind":{"Function":signature},"methods":[]}],"exports":{"Run":{"Function":signature}}},
            "object":{"name":"checks/cases","sections":[],"symbols":[],"relocations":[]},
            "debug":{"package":"checks/cases","functions":[],"ranges":[]}
        });
        let path = project.join("program.ondbuild");
        let write = |artifact: &serde_json::Value| {
            let bytes = serde_json::to_vec(artifact).unwrap();
            fs::write(project.join("packages/cases.ondpkg"), &bytes).unwrap();
            let manifest = serde_json::json!({"contract":contract,"compiler_id":"c".repeat(64),"packages":[{"package":"checks/cases","build_id":"b".repeat(64),"path":"packages/cases.ondpkg","content_hash":format!("{:x}",Sha256::digest(&bytes))}],"program":{"entry_symbol":"main.main","initializers":[]}});
            fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        };
        write(&artifact);
        assert!(validate_registered_functions(&config, &path).is_ok());
        for export in [
            serde_json::json!({"Global":0}),
            serde_json::json!({"Type":0}),
        ] {
            artifact["interface"]["exports"]["Run"] = export;
            write(&artifact);
            assert!(
                validate_registered_functions(&config, &path)
                    .unwrap_err()
                    .contains("関数宣言")
            );
        }
        artifact["interface"]["exports"]["Run"] =
            serde_json::json!({"Function":{"parameters":[0],"returns":[]}});
        write(&artifact);
        assert!(validate_registered_functions(&config, &path).is_err());
        artifact["interface"]["package"] = serde_json::json!("wrong/package");
        write(&artifact);
        assert!(
            validate_registered_functions(&config, &path)
                .unwrap_err()
                .contains("識別情報不一致")
        );
    }
}
