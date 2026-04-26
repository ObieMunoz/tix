use std::path::PathBuf;

pub fn tix_config_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("tix"));
    }
    dirs::home_dir().map(|h| h.join(".config").join("tix"))
}
