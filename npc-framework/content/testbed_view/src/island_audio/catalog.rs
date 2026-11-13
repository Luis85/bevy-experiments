//! Which sound file plays what: only the audio files listed in
//! `assets/living-world-manifest.json` under `living/audio/` that exist on
//! disk, found by folder and file stem (`living/audio/work/chop_1.ogg`), or
//! by words in their names. A sound with no file is `None`: a loop made in
//! code for an ambience layer ([`super::synth`]), silence for anything
//! else. Read once at startup; a missing file is never loaded, so it logs
//! nothing. The older, unlisted `assets/audio/` folder is retired.
use std::path::Path;

/// The manifest's audio entries (`living/audio/...`), in its order.
pub(crate) fn manifest_audio(manifest: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(manifest) else {
        return Vec::new();
    };
    value
        .get("files")
        .and_then(|files| files.as_array())
        .map(|files| {
            files
                .iter()
                .filter_map(|file| file.get("path")?.as_str())
                .filter(|path| path.starts_with("living/audio/") && path.ends_with(".ogg"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The first of `paths` in folder `folder` (`ambience`, `work`, `ui`,
/// `music`, or any when empty) whose file name holds one of `words`, in the
/// order of `words` (the first word is the best match).
pub(crate) fn pick<'a>(paths: &'a [String], folder: &str, words: &[&str]) -> Option<&'a str> {
    let in_folder =
        |path: &&String| folder.is_empty() || path.contains(&format!("/audio/{folder}/"));
    words.iter().find_map(|word| {
        paths
            .iter()
            .filter(in_folder)
            .find(|path| {
                Path::new(path.as_str())
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.to_ascii_lowercase().contains(word))
            })
            .map(String::as_str)
    })
}

/// The file `living/audio/<folder>/<stem>.ogg`, when it is listed.
pub(crate) fn stem<'a>(paths: &'a [String], folder: &str, stem: &str) -> Option<&'a str> {
    let want = format!("living/audio/{folder}/{stem}.ogg");
    paths.iter().find(|path| **path == want).map(String::as_str)
}

/// The manifest's sound files that exist under the asset folder `root`.
pub(crate) fn available(root: &Path) -> Vec<String> {
    std::fs::read_to_string(root.join("living-world-manifest.json"))
        .ok()
        .map(|text| manifest_audio(&text))
        .unwrap_or_default()
        .into_iter()
        .filter(|path| root.join(path).is_file())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_lists_its_audio_and_names_pick_the_sound() {
        let manifest = r#"{"version":1,"files":[
            {"path":"living/props/flag_red.gltf"},
            {"path":"living/audio/ambience/shore-waves-loop.ogg"},
            {"path":"living/audio/ambience/rain-light.ogg"},
            {"path":"living/audio/work/hammer_01.ogg"},
            {"path":"living/audio/work/hammer_02.ogg"},
            {"path":"living/audio/ui/click.ogg"},
            {"path":"living/audio/licence.txt"}]}"#;
        let paths = manifest_audio(manifest);
        assert_eq!(paths.len(), 5);
        assert_eq!(
            pick(&paths, "ambience", &["wave", "surf"]),
            Some("living/audio/ambience/shore-waves-loop.ogg")
        );
        assert_eq!(pick(&paths, "ambience", &["wind"]), None);
        assert_eq!(pick(&paths, "work", &["wave"]), None, "the folder counts");
        assert_eq!(
            stem(&paths, "work", "hammer_02"),
            Some("living/audio/work/hammer_02.ogg")
        );
        assert_eq!(stem(&paths, "work", "hammer"), None, "the whole stem");
        assert!(manifest_audio("not json").is_empty());
    }

    #[test]
    fn a_missing_folder_has_no_sounds() {
        assert!(available(Path::new("no/such/folder")).is_empty());
    }
}
