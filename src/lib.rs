// Adapted from zed-effect-tsgo by RATIU5, which is based on the tsgo extension by
// Zed Industries, Inc. Licensed under the Apache License, Version 2.0; see NOTICE.

mod binary;
mod platform;

use zed_extension_api::{
    self as zed, LanguageServerId, Result, Worktree, serde_json::Value, settings::LspSettings,
};

const DEFAULT_ARGS: [&str; 2] = ["--lsp", "--stdio"];

struct EffectTsrsExtension;

impl zed::Extension for EffectTsrsExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        let binary = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.binary);
        let (path, arguments, env) = match binary {
            Some(binary) => (binary.path, binary.arguments, binary.env),
            None => (None, None, None),
        };

        let command = match path.as_deref().map(str::trim) {
            Some(path) if !path.is_empty() => path.to_owned(),
            _ => binary::find(worktree)?,
        };
        let args = arguments.unwrap_or_else(|| DEFAULT_ARGS.map(String::from).to_vec());
        let env = env.unwrap_or_default().into_iter().collect();

        Ok(zed::Command { command, args, env })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        let options = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.initialization_options);
        Ok(Some(options.unwrap_or_default()))
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.settings);
        Ok(Some(settings.unwrap_or_default()))
    }
}

zed::register_extension!(EffectTsrsExtension);
