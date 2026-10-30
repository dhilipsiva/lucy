//! Identity is captured for the provider actually used, then carried with its
//! proposal across approval waits. An external caller's model is not inferred.
use super::{Result, core::Service, providers};
use crate::self_model::grouped::{Conditions, ModelIdentity};
use crate::store::digest;
use serde_json::json;
use std::path::Path;

impl Service {
    pub(super) fn forecast_planner(&self) -> Result<(Option<providers::Provider>, ModelIdentity)> {
        #[cfg(test)]
        if self.has_script() {
            return Ok((
                None,
                ModelIdentity::Known {
                    name: "scripted-mechanism-fixture".into(),
                    revision: Some(digest(b"fixture-v1")),
                },
            ));
        }
        let provider = providers::selected()?;
        let identity = providers::identity_for(provider.as_ref())?;
        let revision = identity
            .rsplit_once('@')
            .map(|(_, r)| r)
            .filter(|r| r.len() == 64 && r.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(str::to_owned);
        Ok((
            provider,
            ModelIdentity::Known {
                name: identity,
                revision,
            },
        ))
    }
}

fn configured_path(name: &str, default: &str) -> Vec<(String, Option<String>)> {
    let value = std::env::var_os(name).unwrap_or_else(|| default.into());
    std::env::split_paths(&value)
        .map(|p| {
            (
                digest(p.as_os_str().as_encoded_bytes()),
                p.canonicalize()
                    .ok()
                    .map(|p| digest(p.as_os_str().as_encoded_bytes())),
            )
        })
        .collect()
}

pub(super) fn conditions(
    workspace: &Path,
    tool: &str,
    model_identity: ModelIdentity,
) -> Result<Conditions> {
    let mut configuration = json!({"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"executor":"workspace-permissions-v1"});
    if matches!(tool, "command" | "git") {
        configuration["command_path"] = json!(configured_path(
            "LUCY_COMMAND_PATH",
            "/run/current-system/sw/bin"
        ));
    }
    if tool == "gui" {
        configuration["input_helper"] = json!(configured_path("LUCY_INPUT_HELPER", ""));
    }
    if tool == "privileged" {
        configuration["hardware_helper"] = json!(configured_path("LUCY_DMI_HELPER", ""));
    }
    Ok(Conditions {
        workspace: digest(workspace.as_os_str().as_encoded_bytes()),
        method: "native-executor-v1".into(),
        model_identity,
        implementation: crate::task_model::study::implementation()?,
        environment: digest(configuration.to_string().as_bytes()),
    })
}
