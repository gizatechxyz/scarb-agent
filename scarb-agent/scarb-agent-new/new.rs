use crate::new_cairo::mk_cairo;
use crate::new_python::mk_python;
use crate::templates::get_template_engine;
use crate::{fsx, restricted_names, ProjectConfig};
use anyhow::{bail, ensure, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use indoc::formatdoc;
use itertools::Itertools;
use once_cell::sync::Lazy;
use scarb::core::{Config, PackageName};
use serde_json::json;

const DEFAULT_TARGET_DIR_NAME: &str = "target";
const RUN_SERVICE_PATH: Lazy<Utf8PathBuf> = Lazy::new(|| ["run-service.yaml"].iter().collect());
const CLOUDBUILD_PATH: Lazy<Utf8PathBuf> = Lazy::new(|| ["cloudbuild.yaml"].iter().collect());
const GITIGNORE_PATH: Lazy<Utf8PathBuf> = Lazy::new(|| [".gitignore"].iter().collect());
const PRE_COMMIT_CONFIG: Lazy<Utf8PathBuf> =
    Lazy::new(|| [".pre-commit-config.yaml"].iter().collect());
const DOCKERFILE_PATH: Lazy<Utf8PathBuf> = Lazy::new(|| ["Dockerfile"].iter().collect());

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum VersionControl {
    Git,
}

#[derive(Debug)]
pub(crate) struct InitOptions {
    pub(crate) path: Utf8PathBuf,
    pub(crate) name: Option<PackageName>,
    pub(crate) vcs: VersionControl,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct NewResult {
    pub(crate) name: PackageName,
}

pub(crate) fn new_package(
    opts: InitOptions,
    config: &Config,
    project_config: &ProjectConfig,
) -> Result<NewResult> {
    ensure!(
        !opts.path.exists(),
        formatdoc!(
            r#"
                destination `{}` already exists.
                help: use a different project name.
            "#,
            opts.path
        )
    );

    let name = infer_name(opts.name, &opts.path, config)?;

    mk(
        MkOpts {
            path: opts.path.clone(),
            name: name.clone(),
            version_control: opts.vcs,
        },
        config,
        project_config,
    )
    .with_context(|| format!("failed to create package `{name}` at: {}", opts.path))?;

    Ok(NewResult { name })
}

fn infer_name(name: Option<PackageName>, path: &Utf8Path, config: &Config) -> Result<PackageName> {
    let name = if let Some(name) = name {
        name
    } else {
        let Some(file_name) = path.file_name() else {
            bail!(formatdoc! {r#"
                cannot infer package name from path: {path}
                help: use --name to override
            "#});
        };
        PackageName::try_new(file_name)?
    };

    if restricted_names::is_internal(name.as_str()) {
        config.ui().warn(formatdoc! {r#"
            the name `{name}` is a Scarb internal package, \
            it is recommended to use a different name to avoid problems
        "#});
    }

    if restricted_names::is_windows_restricted(name.as_str()) {
        if cfg!(windows) {
            bail!("cannot use name `{name}`, it is a Windows reserved filename");
        } else {
            config.ui().warn(formatdoc! {r#"
                the name `{name}` is a Windows reserved filename, \
                this package will not work on Windows platforms
            "#})
        }
    }

    Ok(name)
}

struct MkOpts {
    path: Utf8PathBuf,
    name: PackageName,
    version_control: VersionControl,
}

fn mk(
    MkOpts {
        path,
        name,
        version_control,
    }: MkOpts,
    config: &Config,
    project_config: &ProjectConfig,
) -> Result<()> {
    // Create project directory in case we are called from `new` op.
    fsx::create_dir_all(&path)?;

    let canonical_path = fsx::canonicalize_utf8(&path).unwrap_or(path);
    init_vcs(&canonical_path, version_control)?;
    write_vcs_ignore(&canonical_path, config, version_control)?;

    // Generate README.md
    let registry = get_template_engine();
    let readme_content = registry.render(
        "readme",
        &json!({
            "name": name.to_string(),
            "preprocess": project_config.preprocess,
            "postprocess": project_config.postprocess,
            "agent_api": project_config.agent_api,
            "oracle": project_config.oracle,
        }),
    )?;

    let readme_path = canonical_path.join("README.md");
    fsx::write(&readme_path, readme_content)?;

    // Generate the run-service.yaml
    let filename = canonical_path.join(RUN_SERVICE_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        fsx::write(
            filename,
            registry.render(
                "run-service",
                &json!({
                    "name": name
                }),
            )?,
        )?;
    }

    // Generate the `cloudbuild.yaml` file.
    let filename = canonical_path.join(CLOUDBUILD_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        fsx::write(filename, registry.render("cloudbuild", &json!({}))?)?;
    }

    // Create the `.gitignore` file.
    let filename = canonical_path.join(GITIGNORE_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        fsx::write(filename, registry.render("gitignore", &json!({}))?)?;
    }

    // Create the `pre-commit` file.
    let filename = canonical_path.join(PRE_COMMIT_CONFIG.as_path());
    let pre_commit = registry.render("pre-commit", &json!({}))?;
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        fsx::write(filename, pre_commit)?;
    }

    // Create the `Dockerfile` file.
    let filename = canonical_path.join(DOCKERFILE_PATH.as_path());
    if !filename.exists() {
        fsx::create_dir_all(filename.parent().unwrap())?;

        fsx::write(filename, registry.render("dockerfile", &json!({}))?)?;
    }

    mk_python(&canonical_path, project_config)?;
    mk_cairo(&canonical_path, &name, &config, project_config)?;

    Ok(())
}

fn init_vcs(path: &Utf8Path, vcs: VersionControl) -> Result<()> {
    match vcs {
        VersionControl::Git => {
            if !path.join(".git").exists() {
                gix::init(path)?;
            }
        }
    }

    Ok(())
}

/// Write VCS ignore file.
fn write_vcs_ignore(path: &Utf8Path, config: &Config, vcs: VersionControl) -> Result<()> {
    let patterns = vec![DEFAULT_TARGET_DIR_NAME];

    let fp_ignore = match vcs {
        VersionControl::Git => path.join(".gitignore"),
    };

    if !fp_ignore.exists() {
        let ignore = patterns.join("\n") + "\n";
        fsx::write(&fp_ignore, ignore)?;
    } else {
        let lines = patterns
            .into_iter()
            .map(|pat| format!("    {pat}"))
            .join("\n");
        config
            .ui()
            .warn(formatdoc! {r#"
                file `{fp_ignore}` already exists in this directory, ensure following patterns are ignored:

                {lines}
            "#});
    }

    Ok(())
}
