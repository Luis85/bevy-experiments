//! The normal launcher is an entry point into the native host library, not a
//! second window/process. Explicit developer screens keep the existing toolbox.
use std::path::PathBuf;
use testbed_content::explorer::settings::Settings;

pub(crate) fn arguments(raw: &[String]) -> Result<Option<Vec<String>>, String> {
    let explicit = raw.iter().any(|arg| arg == "--front-end");
    if raw.iter().any(|arg| arg == "--developer-menu") {
        if explicit {
            return Err("Choose either --front-end or --developer-menu".into());
        }
        return Ok(None);
    }
    let mut settings_path = None;
    let mut forwarded = Vec::new();
    let mut it = raw.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--front-end" => {}
            "--settings" => {
                settings_path = Some(PathBuf::from(it.next().ok_or("--settings needs a file")?));
            }
            _ => forwarded.push(flag.clone()),
        }
    }
    if !explicit && !forwarded.is_empty() {
        return Ok(None);
    }
    let home = crate::launch::home_dir(&|key| std::env::var(key).ok());
    let file = settings_path.or_else(|| Settings::file(&|key| std::env::var(key).ok()));
    let settings = file
        .as_ref()
        .map(|file| Settings::load(file, &home))
        .unwrap_or_else(|| Settings::default_for(&home));
    Ok(Some(with_settings(&forwarded, &settings)))
}
fn with_settings(forwarded: &[String], settings: &Settings) -> Vec<String> {
    let mut args = vec!["--front-end".into()];
    if !forwarded.iter().any(|arg| arg == "--island-save") {
        args.extend([
            "--island-save".into(),
            settings
                .agents_dir
                .join("living-world.world.json")
                .display()
                .to_string(),
        ]);
    }
    if !forwarded.iter().any(|arg| arg == "--island-rules") {
        args.extend([
            "--island-rules".into(),
            settings
                .worlds_dir()
                .join("living_world.json")
                .display()
                .to_string(),
        ]);
    }
    if !forwarded
        .iter()
        .any(|arg| matches!(arg.as_str(), "--remote" | "--no-remote" | "--remote-port"))
    {
        args.extend(["--no-remote".into(), "--no-render-remote".into()]);
    }
    args.extend_from_slice(forwarded);
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_entry_uses_one_host_library_and_developer_routes_stay_explicit() {
        let args = arguments(&[]).unwrap().unwrap();
        assert!(args.contains(&"--front-end".into()));
        assert!(args.contains(&"--no-remote".into()));
        assert!(
            args.iter()
                .any(|arg| arg.ends_with("living-world.world.json"))
        );
        assert!(args.iter().any(|arg| arg.ends_with("living_world.json")));
        assert!(arguments(&["--developer-menu".into()]).unwrap().is_none());
        assert!(
            arguments(&["--screen".into(), "worlds".into()])
                .unwrap()
                .is_none()
        );
        assert!(arguments(&["--front-end".into(), "--developer-menu".into()]).is_err());
    }
    #[test]
    fn explicit_frontend_capture_and_save_arguments_are_forwarded_once() {
        let settings = Settings::default_for(std::path::Path::new("C:/NPC"));
        let forwarded = vec![
            "--island-save".into(),
            "C:/scratch/world.json".into(),
            "--remote".into(),
            "--screenshot".into(),
            "shot.png".into(),
        ];
        let args = with_settings(&forwarded, &settings);
        assert_eq!(
            args.iter()
                .filter(|arg| arg.as_str() == "--island-save")
                .count(),
            1
        );
        assert!(!args.contains(&"--no-remote".into()));
        assert!(args.ends_with(&forwarded));
    }
}
